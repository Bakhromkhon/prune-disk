use crate::util::display_path;
use anyhow::Result;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use humansize::{DECIMAL, format_size};
use prune_core::{Risk, ScanReport};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap};
use ratatui::{Frame, Terminal};
use std::io::{self, Stdout};

type PruneTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Restores the terminal even if the TUI returns an error or panics.
struct TuiGuard;

impl TuiGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide)?;
        Ok(Self)
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
                if app.handle_key(key.code) {
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
}

impl<'a> App<'a> {
    fn new(report: &'a ScanReport) -> Self {
        Self {
            report,
            selected: 0,
            table_state: TableState::default(),
            show_details: true,
            sort: SortMode::Size,
        }
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

    fn handle_key(&mut self, code: KeyCode) -> bool {
        let len = self.report.findings.len();
        if len == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(len - 1);
        }

        match code {
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
                self.sort = self.sort.next();
            }
            _ => {}
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
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(if self.show_details { 8 } else { 0 }),
                Constraint::Length(2),
            ])
            .split(frame.area());

        self.draw_header(frame, chunks[0]);
        self.draw_table(frame, chunks[1]);
        if self.show_details {
            self.draw_details(frame, chunks[2]);
        }
        self.draw_footer(frame, chunks[3]);
    }

    fn draw_header(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let title = format!(" Prune {} ", env!("CARGO_PKG_VERSION"));
        let summary = format!(
            " read-only scan · {} findings · {} · {} warnings ",
            self.report.findings.len(),
            format_size(self.report.total_bytes, DECIMAL),
            self.report.errors.len()
        );
        let line = Line::from(vec![
            Span::styled(
                title,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(summary, Style::default().fg(Color::Gray)),
        ]);
        let paragraph = Paragraph::new(line).block(Block::default().borders(Borders::BOTTOM));
        frame.render_widget(paragraph, area);
    }

    fn draw_table(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        if self.report.findings.is_empty() {
            let paragraph = Paragraph::new(
                "No findings. Your disk looks tidy, or the built-in rules do not cover this machine yet.",
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" Findings "));
            frame.render_widget(paragraph, area);
            return;
        }

        let indices = self.visible_indices();
        let rows: Vec<Row> = indices
            .iter()
            .map(|&index| {
                let finding = &self.report.findings[index];
                Row::new(vec![
                    Cell::from(format_size(finding.size_bytes, DECIMAL)),
                    Cell::from(finding.risk.as_str())
                        .style(Style::default().fg(risk_color(finding.risk))),
                    Cell::from(finding.category.as_str()),
                    Cell::from(finding.rule_name.clone()),
                    Cell::from(display_path(&finding.path)),
                ])
            })
            .collect();

        let header = Row::new(vec![
            Cell::from("Size"),
            Cell::from("Risk"),
            Cell::from("Category"),
            Cell::from("Name"),
            Cell::from("Path"),
        ])
        .style(Style::default().add_modifier(Modifier::BOLD));

        let table = Table::new(
            rows,
            [
                Constraint::Length(10),
                Constraint::Length(7),
                Constraint::Length(11),
                Constraint::Length(28),
                Constraint::Min(10),
            ],
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

    fn draw_details(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let indices = self.visible_indices();
        let Some(&index) = indices.get(self.selected) else {
            return;
        };
        let finding = &self.report.findings[index];
        let label = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);

        let lines = vec![
            Line::from(vec![
                Span::styled("Rule  ", label),
                Span::raw(finding.rule_name.as_str()),
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
                Span::raw(finding.description.as_str()),
            ]),
        ];

        let paragraph = Paragraph::new(Text::from(lines))
            .block(Block::default().borders(Borders::ALL).title(" Details "))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    fn draw_footer(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let text = format!(
            " q quit · ↑↓ navigate · enter details · s sort: {} ",
            self.sort.label()
        );
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
