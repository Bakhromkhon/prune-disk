mod output;
mod redact;
mod scan_ui;
mod size;
mod tui;
mod util;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use humansize::{DECIMAL, format_size};
use prune_core::{
    Category, Risk, Rule, ScanControl, ScanOptions, ValidatedRuleSet, builtin_rules,
    load_rules_from_dir, scan_with_control,
};
use prune_history::{HistoryMode, HistoryStore, SnapshotSummary};
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "prune",
    version,
    about = "Make room. Keep what matters.",
    long_about = "Prune is a read-only disk scanner today. It will become a safe cleaner with a memory, a plan, and an undo button."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan for cleanup candidates (read-only)
    #[command(
        after_help = "With terminal stdin and stderr, q/Esc/Ctrl+C cancels cooperatively: the partial report is printed before exit code 130. In-flight filesystem calls must finish first.\nJSON and noninteractive scans have no progress or key handling; Ctrl+C uses the operating system's default signal behavior and may terminate without a report."
    )]
    Scan(ScanArgs),
    /// List or validate rules
    Rules(RulesArgs),
    /// Show environment and safety information
    Doctor,
    /// Manage explicitly saved local scan history (metadata only)
    History(HistoryArgs),
}

#[derive(clap::Args)]
struct ScanArgs {
    /// Maximum scan workers (1..=16)
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u8).range(1..=16))]
    workers: u8,
    /// Output JSON instead of a table or TUI
    #[arg(long)]
    json: bool,
    /// Replace home-directory prefixes with ~ in JSON output only
    #[arg(long, requires = "json")]
    redact_home: bool,
    /// Read rules from a directory instead of the built-in set
    #[arg(long, value_name = "DIR")]
    rules_dir: Option<PathBuf>,
    /// Only show findings at least this large (for example 100MB)
    #[arg(long, default_value = "0", value_parser = size::parse_size)]
    min_size: u64,
    /// Filter by category
    #[arg(long, value_enum)]
    category: Option<CategoryArg>,
    /// Only include rules at or below this risk level
    #[arg(long, value_enum)]
    max_risk: Option<RiskArg>,
    /// Disable the interactive TUI
    #[arg(long)]
    no_tui: bool,
    /// Save this scan's metadata to the local history database
    #[arg(long)]
    remember: bool,
}

#[derive(clap::Args)]
struct HistoryArgs {
    #[command(subcommand)]
    command: HistoryCommand,
}

#[derive(Subcommand)]
enum HistoryCommand {
    /// List locally saved scans without creating a history database
    List,
    /// Compare two complete snapshots with the same local scope and filters
    Diff { older: i64, newer: i64 },
    /// Delete one saved metadata snapshot
    Delete { id: i64 },
    /// Delete all saved metadata; requires explicit confirmation
    Clear {
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CategoryArg {
    Developer,
    Browser,
    System,
    Trash,
    Apps,
    Media,
    Other,
}

impl From<CategoryArg> for Category {
    fn from(value: CategoryArg) -> Self {
        match value {
            CategoryArg::Developer => Category::Developer,
            CategoryArg::Browser => Category::Browser,
            CategoryArg::System => Category::System,
            CategoryArg::Trash => Category::Trash,
            CategoryArg::Apps => Category::Apps,
            CategoryArg::Media => Category::Media,
            CategoryArg::Other => Category::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum RiskArg {
    Safe,
    Low,
    Medium,
    High,
    Never,
}

impl From<RiskArg> for Risk {
    fn from(value: RiskArg) -> Self {
        match value {
            RiskArg::Safe => Risk::Safe,
            RiskArg::Low => Risk::Low,
            RiskArg::Medium => Risk::Medium,
            RiskArg::High => Risk::High,
            RiskArg::Never => Risk::Never,
        }
    }
}

#[derive(clap::Args)]
struct RulesArgs {
    #[command(subcommand)]
    command: RulesCommand,
}

#[derive(Subcommand)]
enum RulesCommand {
    /// List rules as a table or JSON
    List {
        #[arg(long)]
        json: bool,
        #[arg(long, value_name = "DIR")]
        rules_dir: Option<PathBuf>,
    },
    /// Validate rule files and their safety invariants
    Validate {
        #[arg(long, value_name = "DIR")]
        rules_dir: Option<PathBuf>,
    },
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("error: {}", util::escape_terminal(&format!("{error:#}")));
            std::process::exit(1);
        }
    }
}

fn run() -> Result<i32> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let redact_home = raw_args_request_home_redaction();
            let message = error.to_string();
            let message = if redact_home {
                if let Some(home) = dirs::home_dir().filter(|home| home.is_absolute()) {
                    redact::redact_home_in_string(&message, &home)
                } else {
                    message
                }
            } else {
                message
            };
            if error.use_stderr() {
                eprintln!("{}", util::escape_terminal(&message));
            } else {
                print!("{message}");
            }
            std::process::exit(error.exit_code());
        }
    };
    match cli.command {
        Commands::Scan(args) => {
            let redact_errors = args.redact_home;
            run_scan(args).map_err(|error| {
                if !redact_errors {
                    return error;
                }
                let Some(home) = dirs::home_dir().filter(|home| home.is_absolute()) else {
                    return error;
                };
                let message = redact::redact_home_in_string(&format!("{error:#}"), &home);
                anyhow::anyhow!("{message}")
            })
        }
        Commands::Rules(args) => run_rules(args).map(|()| 0),
        Commands::Doctor => run_doctor().map(|()| 0),
        Commands::History(args) => run_history(args),
    }
}

fn raw_args_request_home_redaction() -> bool {
    std::env::args_os().any(|argument| {
        let argument = argument.to_string_lossy();
        argument == "--redact-home" || argument.starts_with("--redact-home=")
    })
}

fn run_scan(args: ScanArgs) -> Result<i32> {
    let home_for_redaction = if args.redact_home {
        Some(
            dirs::home_dir()
                .filter(|home| home.is_absolute())
                .ok_or_else(|| {
                    anyhow::anyhow!("cannot determine an absolute home directory for --redact-home")
                })?,
        )
    } else {
        None
    };
    let rules = load_rules(args.rules_dir.as_deref())?;
    let options = ScanOptions {
        os: std::env::consts::OS.to_string(),
        rules: ValidatedRuleSet::new(rules).context("invalid scan rules")?,
        min_size: args.min_size,
        categories: args
            .category
            .map(|category| vec![category.into()])
            .unwrap_or_default(),
        max_risk: args.max_risk.map(Into::into),
    };

    let control = ScanControl::default().with_worker_count(usize::from(args.workers));
    let report = if !args.json && io::stderr().is_terminal() && io::stdin().is_terminal() {
        scan_ui::run(&options, &control)?
    } else {
        scan_with_control(&options, &control, |_| {})
    };
    let exit_code = if report.cancelled { 130 } else { 0 };
    if args.remember {
        let mut history = HistoryStore::open_default(HistoryMode::CreateOrOpen)?
            .context("failed to open local history")?;
        let id = history
            .save_report(&report)
            .context("failed to save scan metadata to local history")?;
        eprintln!("Saved scan #{id} to local history (metadata only).");
    }

    if args.json {
        let json = if let Some(home) = home_for_redaction {
            let mut value = serde_json::to_value(&report)?;
            redact::redact_home_in_json(&mut value, &home);
            serde_json::to_string_pretty(&value)?
        } else {
            serde_json::to_string_pretty(&report)?
        };
        println!("{json}");
        return Ok(exit_code);
    }

    if !report.cancelled && !args.no_tui && io::stdout().is_terminal() && io::stdin().is_terminal()
    {
        tui::run(&report)?;
    } else {
        output::print_report(&report);
    }

    Ok(exit_code)
}

fn run_history(args: HistoryArgs) -> Result<i32> {
    match args.command {
        HistoryCommand::List => {
            let Some(history) = HistoryStore::open_default(HistoryMode::ReadOnly)? else {
                println!("No saved scans. Run `prune scan --remember` to opt in to local history.");
                return Ok(0);
            };
            let scans = history.list()?;
            if scans.is_empty() {
                println!("No saved scans. Run `prune scan --remember` to opt in to local history.");
            } else {
                output_history_list(&scans);
            }
        }
        HistoryCommand::Diff { older, newer } => {
            let history = HistoryStore::open_default(HistoryMode::ReadOnly)?
                .context("No local history database; save a scan with --remember first")?;
            let diff = history.diff(older, newer)?;
            output_history_diff(&diff);
        }
        HistoryCommand::Delete { id } => {
            let Some(mut history) = HistoryStore::open_default(HistoryMode::ReadWrite)? else {
                anyhow::bail!("No local history database");
            };
            history.delete(id)?;
            println!("Deleted history snapshot #{id} (metadata only).");
        }
        HistoryCommand::Clear { yes: false } => {
            anyhow::bail!("Clearing local history deletes saved metadata; pass --yes to confirm");
        }
        HistoryCommand::Clear { yes: true } => {
            let Some(mut history) = HistoryStore::open_default(HistoryMode::ReadWrite)? else {
                println!("No local history database.");
                return Ok(0);
            };
            let removed = history.clear()?;
            println!("Cleared {removed} local history snapshots (metadata only).");
        }
    }
    Ok(0)
}

fn output_history_list(scans: &[SnapshotSummary]) {
    println!(
        "{:<6} {:<19} {:<10} {:<10} {:<11} FINDINGS",
        "ID", "SCANNED (UTC)", "PLATFORM", "STATUS", "LOGICAL SIZE"
    );
    for scan in scans {
        println!(
            "{:<6} {:<19} {:<10} {:<10} {:<11} {}",
            scan.id,
            format_utc(scan.scanned_at),
            util::truncate(&util::escape_terminal(&scan.platform), 10),
            completeness_label(scan.completeness),
            format_size(scan.total_bytes, DECIMAL),
            scan.finding_count
        );
    }
    println!(
        "History stores local metadata and relative paths only. Maximum {} snapshots; oldest data is never removed automatically.",
        HistoryStore::max_snapshots()
    );
}

fn output_history_diff(diff: &prune_history::Diff) {
    println!(
        "Comparing complete scans #{} → #{} · {} → {}",
        diff.older.id,
        diff.newer.id,
        format_utc(diff.older.scanned_at),
        format_utc(diff.newer.scanned_at)
    );
    println!(
        "Logical size change: {}{} · {} new · {} no longer observed · {} changed",
        if diff.logical_byte_delta < 0 {
            "−"
        } else {
            "+"
        },
        format_size(diff.logical_byte_delta.unsigned_abs() as u64, DECIMAL),
        diff.added,
        diff.no_longer_observed,
        diff.changed
    );
    println!("This is a metadata comparison, not a deletion or free-space measurement.");
    if diff.changes.is_empty() {
        println!("No path or logical-size changes were observed.");
        return;
    }
    println!();
    for change in &diff.changes {
        let (before, after) = match (change.before_bytes, change.after_bytes) {
            (None, Some(after)) => ("—".to_string(), format_size(after, DECIMAL)),
            (Some(before), None) => (format_size(before, DECIMAL), "—".to_string()),
            (Some(before), Some(after)) => {
                (format_size(before, DECIMAL), format_size(after, DECIMAL))
            }
            (None, None) => ("—".to_string(), "—".to_string()),
        };
        println!(
            "{:<24} {}  {} → {}  {}",
            change.kind.label(),
            util::truncate(&util::escape_terminal(&change.rule_name), 24),
            before,
            after,
            util::escape_terminal(&change.path)
        );
    }
    if diff.omitted_changes > 0 {
        println!(
            "… and {} more changes (use a narrower scan scope to inspect fewer paths).",
            diff.omitted_changes
        );
    }
}

fn format_utc(seconds: u64) -> String {
    let Ok(seconds) = i64::try_from(seconds) else {
        return "timestamp out of range".to_string();
    };
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let shifted = days.saturating_add(719_468);
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    }
    .div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    let hour = day_seconds / 3_600;
    let minute = day_seconds % 3_600 / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

fn completeness_label(completeness: prune_core::ScanCompleteness) -> &'static str {
    match completeness {
        prune_core::ScanCompleteness::Complete => "полный",
        prune_core::ScanCompleteness::Partial => "частичный",
        prune_core::ScanCompleteness::Cancelled => "отменён",
        prune_core::ScanCompleteness::Unknown => "неизвестный",
    }
}

fn run_rules(args: RulesArgs) -> Result<()> {
    match args.command {
        RulesCommand::List { json, rules_dir } => {
            let rules = load_rules(rules_dir.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rules)?);
            } else {
                output::print_rules(&rules);
            }
        }
        RulesCommand::Validate { rules_dir } => {
            let rules = load_rules(rules_dir.as_deref())?;
            println!("ok: {} rules are valid", rules.len());
        }
    }
    Ok(())
}

fn run_doctor() -> Result<()> {
    let rules = builtin_rules().context("failed to load built-in rules")?;
    println!("Prune {}", env!("CARGO_PKG_VERSION"));
    println!(
        "Platform: {} ({})",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!(
        "Home: {}",
        if dirs::home_dir().is_some() {
            "available (redacted)"
        } else {
            "unknown"
        }
    );
    println!("Built-in rules: {}", rules.len());
    println!("Mode: read-only (v0.1)");
    println!("Network: no intentional network functionality (static checks are heuristic)");
    println!("Telemetry: none");
    Ok(())
}

fn load_rules(dir: Option<&Path>) -> Result<Vec<Rule>> {
    match dir {
        Some(dir) => load_rules_from_dir(dir)
            .with_context(|| format!("failed to load rules from {}", dir.display())),
        None => builtin_rules().context("failed to load built-in rules"),
    }
}
