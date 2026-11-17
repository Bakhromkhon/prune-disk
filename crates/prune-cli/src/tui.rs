use crate::util::{display_path, escape_terminal};
use anyhow::Result;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use humansize::{DECIMAL, format_size};
use prune_core::{Risk, ScanReport};
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Widget, Wrap};
use ratatui::{Frame, Terminal};
use std::io::{self, Stdout};

type PruneTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Restores the terminal even if the TUI returns an error or panics.
struct TuiGuard;

impl TuiGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        // Arm cleanup before any fallible terminal initialization. Even a failed
        // write can have partially entered the alternate screen or hidden the cursor.
        let guard = Self;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide)?;
        Ok(guard)
    }
}

impl Drop for TuiGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, Show);
    }
}

/// Run the interactive findings browser.
pub fn run(report: &ScanReport) -> Result<()> {
    let _guard = TuiGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal: PruneTerminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut app = App::new(report);

    loop {
        terminal.draw(|frame| app.draw(frame))?;
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if app.handle_key(key) {
                    break;
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }

    terminal.clear()?;
    Ok(())
}

#[derive(Clone, Copy)]
enum SortMode {
    Size,
    Name,
    Risk,
}

impl SortMode {
    fn label(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Name => "name",
            Self::Risk => "risk",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Size => Self::Name,
            Self::Name => Self::Risk,
            Self::Risk => Self::Size,
        }
    }
}

struct App<'a> {
    report: &'a ScanReport,
    selected: usize,
    table_state: TableState,
    show_details: bool,
    sort: SortMode,
    indices: Vec<usize>,
    show_warnings: bool,
    details_scroll: u16,
}

impl<'a> App<'a> {
    fn new(report: &'a ScanReport) -> Self {
        let mut app = Self {
            report,
            selected: 0,
            table_state: TableState::default(),
            show_details: true,
            sort: SortMode::Size,
            indices: Vec::new(),
            show_warnings: false,
            details_scroll: 0,
        };
        app.indices = app.visible_indices();
        app
    }

    fn visible_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.report.findings.len()).collect();
        match self.sort {
            SortMode::Size => indices.sort_by(|&left, &right| {
                self.report.findings[right]
                    .size_bytes
                    .cmp(&self.report.findings[left].size_bytes)
            }),
            SortMode::Name => indices.sort_by(|&left, &right| {
                self.report.findings[left]
                    .rule_name
                    .cmp(&self.report.findings[right].rule_name)
            }),
            SortMode::Risk => indices.sort_by(|&left, &right| {
                self.report.findings[right]
                    .risk
                    .cmp(&self.report.findings[left].risk)
            }),
        }
        indices
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c' | 'C'))
        {
            return true;
        }
        let len = self.report.findings.len();
        if len == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(len - 1);
        }

        let previous_selection = self.selected;
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if len > 0 {
                    self.selected = (self.selected + 1).min(len - 1);
                }
            }
            KeyCode::PageUp => {
                self.selected = self.selected.saturating_sub(10);
            }
            KeyCode::PageDown => {
                if len > 0 {
                    self.selected = (self.selected + 10).min(len - 1);
                }
            }
            KeyCode::Home => {
                self.selected = 0;
            }
            KeyCode::End => {
                if len > 0 {
                    self.selected = len - 1;
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.show_details = !self.show_details;
            }
            KeyCode::Char('s') => {
                let selected_index = self.indices.get(self.selected).copied();
                self.sort = self.sort.next();
                self.indices = self.visible_indices();
                self.selected = selected_index
                    .and_then(|index| self.indices.iter().position(|&item| item == index))
                    .unwrap_or(0);
            }
            KeyCode::Char('w') => {
                self.show_warnings = !self.show_warnings;
                self.details_scroll = 0;
            }
            KeyCode::Char(']') => {
                self.details_scroll = self.details_scroll.saturating_add(1);
            }
            KeyCode::Char('[') => {
                self.details_scroll = self.details_scroll.saturating_sub(1);
            }
            _ => {}
        }

        if self.selected != previous_selection && !self.show_warnings {
            self.details_scroll = 0;
        }
        false
    }

    fn draw(&mut self, frame: &mut Frame) {
        let len = self.report.findings.len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(3),
                Constraint::Length(if self.show_details || self.show_warnings {
                    (frame.area().height / 3).clamp(3, 10)
                } else {
                    0
                }),
                Constraint::Length(2),
            ])
            .split(frame.area());

        self.draw_header(frame, chunks[0]);
        self.draw_table(frame, chunks[1]);
        if self.show_details || self.show_warnings {
            self.draw_details(frame, chunks[2]);
        }
        self.draw_footer(frame, chunks[3]);
    }

    fn draw_header(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let title = format!(" Prune {} ", env!("CARGO_PKG_VERSION"));
        let status = if self.report.cancelled {
            "cancelled"
        } else if self.report.partial {
            "partial"
        } else {
            "complete"
        };
        let summary = format!(
            " {status} · {} findings · logical size: {} · {} warnings + {} suppressed (w) ",
            self.report.findings.len(),
            format_size(self.report.total_bytes, DECIMAL),
            self.report.errors.len(),
            self.report.suppressed_errors
        );
        let line = if area.width < 90 {
            Line::from(format!(
                "{status} · {} findings · {} warnings + {} suppressed (w)",
                self.report.findings.len(),
                self.report.errors.len(),
                self.report.suppressed_errors
            ))
        } else {
            Line::from(vec![
                Span::styled(
                    title,
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(summary, Style::default().fg(Color::Gray)),
            ])
        };
        let paragraph = Paragraph::new(line).block(Block::default().borders(Borders::BOTTOM));
        frame.render_widget(paragraph, area);
    }

    fn draw_table(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        if self.report.findings.is_empty() {
            let paragraph = Paragraph::new(
                "No findings matched the scanned rules. Coverage and access may be limited.",
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" Findings "));
            frame.render_widget(paragraph, area);
            return;
        }

        let compact = area.width < 90;
        let rows: Vec<Row> = self
            .indices
            .iter()
            .map(|&index| {
                let finding = &self.report.findings[index];
                let mut cells = vec![
                    Cell::from(format_size(finding.size_bytes, DECIMAL)),
                    Cell::from(finding.risk.as_str())
                        .style(Style::default().fg(risk_color(finding.risk))),
                ];
                if !compact {
                    cells.push(Cell::from(finding.category.as_str()));
                }
                cells.push(Cell::from(escape_terminal(&finding.rule_name)));
                if !compact {
                    cells.push(Cell::from(display_path(&finding.path)));
                }
                Row::new(cells)
            })
            .collect();

        let mut headings = vec![Cell::from("Size"), Cell::from("Risk")];
        if !compact {
            headings.push(Cell::from("Category"));
        }
        headings.push(Cell::from("Name"));
        if !compact {
            headings.push(Cell::from("Path"));
        }
        let header = Row::new(headings).style(Style::default().add_modifier(Modifier::BOLD));

        let table = Table::new(
            rows,
            if compact {
                vec![
                    Constraint::Length(10),
                    Constraint::Length(7),
                    Constraint::Min(1),
                ]
            } else {
                vec![
                    Constraint::Length(10),
                    Constraint::Length(7),
                    Constraint::Length(11),
                    Constraint::Length(28),
                    Constraint::Min(10),
                ]
            },
        )
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(" Findings "))
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

        self.table_state.select(Some(self.selected));
        frame.render_stateful_widget(table, area, &mut self.table_state);
    }

    fn draw_details(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        if self.show_warnings {
            let mut lines = if self.report.errors.is_empty() {
                vec![Line::from("No scan warnings.")]
            } else {
                self.report
                    .errors
                    .iter()
                    .map(|error| {
                        Line::from(format!(
                            "{}: {}",
                            escape_terminal(&error.path),
                            escape_terminal(&error.message)
                        ))
                    })
                    .collect()
            };
            if self.report.suppressed_errors > 0 {
                lines.push(Line::from(format!(
                    "Additional warnings suppressed: {}",
                    self.report.suppressed_errors
                )));
            }
            let paragraph = Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Warnings · w close · [ ] scroll "),
                )
                .wrap(Wrap { trim: false });
            self.render_details(frame, area, paragraph);
            return;
        }
        let Some(&index) = self.indices.get(self.selected) else {
            return;
        };
        let finding = &self.report.findings[index];
        let label = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);

        let lines = vec![
            Line::from(vec![
                Span::styled("Rule  ", label),
                Span::raw(escape_terminal(&finding.rule_name)),
            ]),
            Line::from(vec![
                Span::styled("Path  ", label),
                Span::raw(display_path(&finding.path)),
            ]),
            Line::from(vec![
                Span::styled("Risk  ", label),
                Span::styled(
                    finding.risk.as_str(),
                    Style::default()
                        .fg(risk_color(finding.risk))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(
                    "  ·  {}  ·  {} files  ·  recreate: {}",
                    finding.category, finding.file_count, finding.recreate
                )),
            ]),
            Line::from(vec![
                Span::styled("Why   ", label),
                Span::raw(escape_terminal(&finding.description)),
            ]),
        ];

        let paragraph = Paragraph::new(Text::from(lines))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Details · [ ] scroll "),
            )
            .wrap(Wrap { trim: false });
        self.render_details(frame, area, paragraph);
    }

    fn render_details(
        &mut self,
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        paragraph: Paragraph<'_>,
    ) {
        let inner = Block::default().borders(Borders::ALL).inner(area);
        // Use the actual wrapper, including Unicode widths, rather than estimating
        // from string lengths. Keep the last visible text on screen; the final
        // partially filled viewport is allowed, an entirely blank one is not.
        if !inner.is_empty() {
            while self.details_scroll > 0 {
                let mut probe = Buffer::empty(area);
                paragraph
                    .clone()
                    .scroll((self.details_scroll, 0))
                    .render(area, &mut probe);
                let has_text = (inner.top()..inner.bottom()).any(|y| {
                    (inner.left()..inner.right()).any(|x| !probe[(x, y)].symbol().trim().is_empty())
                });
                if has_text {
                    break;
                }
                self.details_scroll -= 1;
            }
        } else {
            self.details_scroll = 0;
        }
        frame.render_widget(paragraph.scroll((self.details_scroll, 0)), area);
    }

    fn draw_footer(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let text = if area.width < 90 {
            " q quit · w warnings · s sort · [ ] scroll ".to_string()
        } else {
            format!(
                " q/Ctrl+C quit · ↑↓ move · enter details · w warnings · [ ] scroll · s: {} ",
                self.sort.label()
            )
        };
        let paragraph = Paragraph::new(text)
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::TOP));
        frame.render_widget(paragraph, area);
    }
}

fn risk_color(risk: Risk) -> Color {
    match risk {
        Risk::Safe => Color::Green,
        Risk::Low => Color::Cyan,
        Risk::Medium => Color::Yellow,
        Risk::High => Color::Red,
        Risk::Never => Color::Magenta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use serde_json::json;

    fn report() -> ScanReport {
        serde_json::from_value(json!({
            "schema_version": 1, "platform": "test", "scanned_at": 0,
            "total_bytes": 30, "total_files": 2,
            "byte_accounting": "logical", "partial": true,
            "cancelled": false, "suppressed_errors": 0,
            "findings": [
                {
                    "rule_id": "a", "rule_name": "alpha",
                    "category": "other", "risk": "safe", "recreate": "unknown",
                    "path": "/alpha", "size_bytes": 10, "file_count": 1,
                    "last_modified": null, "description": "first"
                },
                {
                    "rule_id": "z", "rule_name": "zeta",
                    "category": "other", "risk": "high", "recreate": "unknown",
                    "path": "/zeta", "size_bytes": 20, "file_count": 1,
                    "last_modified": null, "description": "second"
                }
            ],
            "errors": [{"path": "/bad\npath", "message": "\u{1b}[31mdenied\u{202e}"}]
        }))
        .unwrap()
    }

    fn render(app: &mut App<'_>, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn ctrl_c_requires_control_modifier() {
        let report = report();
        let mut app = App::new(&report);
        assert!(!app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)));
        assert!(!app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT)));
        assert!(app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)));
    }

    #[test]
    fn cancelled_status_and_suppressed_errors_are_visible() {
        let mut report = report();
        report.cancelled = true;
        report.partial = true;
        report.suppressed_errors = 123;
        let mut app = App::new(&report);
        for width in [80, 160] {
            let screen = render(&mut app, width, 24);
            assert!(screen.contains("cancelled"));
            assert!(screen.contains("123 suppressed"));
        }
        app.show_warnings = true;
        assert!(render(&mut app, 100, 24).contains("Additional warnings suppressed: 123"));
    }

    #[test]
    fn sorting_preserves_selected_finding_and_render_order() {
        let report = report();
        let mut app = App::new(&report);
        assert_eq!(app.indices, vec![1, 0]);
        for expected_order in [[1, 0], [0, 1], [1, 0], [1, 0]] {
            assert_eq!(app.indices[app.selected], 1);
            assert_eq!(app.indices, expected_order);
            let screen = render(&mut app, 110, 30);
            let (table, details) = screen.split_once(" Details ").unwrap();
            let first_name = &report.findings[expected_order[0]].rule_name;
            let second_name = &report.findings[expected_order[1]].rule_name;
            assert!(table.find(first_name).unwrap() < table.find(second_name).unwrap());
            let highlighted = table.lines().find(|line| line.contains('▸')).unwrap();
            assert!(highlighted.contains("zeta"));
            assert!(details.contains("Rule  zeta"));
            assert!(details.contains("Path  /zeta"));
            assert!(details.contains("Why   second"));
            assert!(!details.contains("Rule  alpha"));
            app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        }
        // Return to size order, then select the other finding.
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.indices[app.selected], 0);
        let screen = render(&mut app, 110, 30);
        let (_, details) = screen.split_once(" Details ").unwrap();
        assert!(details.contains("Rule  alpha"));
        assert!(details.contains("Why   first"));
    }

    #[test]
    fn short_warning_remains_visible_after_repeated_scroll_down() {
        let mut report = report();
        report.errors[0].path = "/short".into();
        report.errors[0].message = "denied".into();
        let mut app = App::new(&report);
        app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE));
        for _ in 0..100 {
            app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
            let screen = render(&mut app, 100, 24);
            assert!(screen.contains("/short: denied"));
            assert_eq!(app.details_scroll, 0);
        }
    }

    #[test]
    fn wrapped_warning_stops_at_visible_content_and_resets_on_resize() {
        let mut report = report();
        report.errors[0].path = "/wrapped".into();
        report.errors[0].message = format!("{} final-marker", "文 words ".repeat(30));
        let mut app = App::new(&report);
        app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE));
        for _ in 0..150 {
            app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
            render(&mut app, 42, 18);
        }
        assert!(app.details_scroll > 0);
        let final_screen = render(&mut app, 42, 18);
        assert!(final_screen.contains("final-marker"));
        let scroll = app.details_scroll;
        app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
        assert_eq!(render(&mut app, 42, 18), final_screen);
        assert_eq!(app.details_scroll, scroll);
        let wide_screen = render(&mut app, 200, 36);
        assert!(wide_screen.contains("final-marker"));
        assert!(app.details_scroll < scroll);
    }

    #[test]
    fn renders_escaped_names_paths_descriptions_and_warnings() {
        let mut report = report();
        report.findings[1].rule_name = "zeta\nname".into();
        report.findings[1].path = "/cache/\rfile".into();
        report.findings[1].description = "why\x1b[31m\u{202e}".into();
        let mut app = App::new(&report);
        let screen = render(&mut app, 140, 36);
        assert!(screen.contains("zeta\\nname"));
        assert!(screen.contains("/cache/\\rfile"));
        assert!(screen.contains("why\\u{1b}[31m\\u{202e}"));
        assert!(!screen.contains('\x1b'));
        assert!(!screen.contains('\u{202e}'));
        app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE));
        let screen = render(&mut app, 140, 36);
        assert!(screen.contains("/bad\\npath: \\u{1b}[31mdenied\\u{202e}"));
        assert!(screen.contains("Warnings"));
    }

    #[test]
    fn compact_and_empty_views_render_and_scroll_without_panicking() {
        let mut report = report();
        let mut app = App::new(&report);
        let screen = render(&mut app, 42, 16);
        assert!(screen.contains("Name"));
        assert!(!screen.contains("Category"));
        app.handle_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE));
        assert_eq!(app.details_scroll, 1);
        app.handle_key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE));
        assert_eq!(app.details_scroll, 0);
        report.findings.clear();
        let mut app = App::new(&report);
        for (width, height) in [(1, 1), (20, 5), (100, 24)] {
            render(&mut app, width, height);
        }
        assert!(!render(&mut app, 100, 24).contains("disk looks tidy"));
        app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE));
        assert!(render(&mut app, 100, 24).contains("denied"));
    }
}
