mod output;
mod scan_ui;
mod size;
mod tui;
mod util;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use prune_core::{
    Category, Risk, Rule, ScanControl, ScanOptions, ValidatedRuleSet, builtin_rules,
    load_rules_from_dir, scan_with_control,
};
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
}

#[derive(clap::Args)]
struct ScanArgs {
    /// Maximum scan workers (1..=16)
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u8).range(1..=16))]
    workers: u8,
    /// Output JSON instead of a table or TUI
    #[arg(long)]
    json: bool,
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
            if error.use_stderr() {
                eprintln!("{}", util::escape_terminal(&error.to_string()));
            } else {
                print!("{error}");
            }
            std::process::exit(error.exit_code());
        }
    };
    match cli.command {
        Commands::Scan(args) => run_scan(args),
        Commands::Rules(args) => run_rules(args).map(|()| 0),
        Commands::Doctor => run_doctor().map(|()| 0),
    }
}

fn run_scan(args: ScanArgs) -> Result<i32> {
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

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
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
