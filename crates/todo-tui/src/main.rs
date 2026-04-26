mod app;
mod editor;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::stdout;
use todo_fs::config::Config;
use todo_fs::store::Store;
use todo_fs::validate;
#[derive(Parser)]
#[command(name = "todo", about = "Local markdown-based todo and notes manager")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Validate the file-based store structure and front-matter content
    Validate,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Print the active configuration and config file path
    Show,
    /// Write a default config file if none exists
    Init,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Config { action }) => run_config(action),
        Some(Command::Validate) => run_validate(),
        None => run_tui(),
    }
}

fn run_config(action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Show => {
            let config = Config::load()?;
            let path = Config::default_config_path();
            println!("config file: {}", path.display());
            println!();
            println!("root_dir = \"{}\"", config.root_dir.display());
            println!("context_slug = \"{}\"", config.context_slug);
        }
        ConfigAction::Init => {
            let path = Config::default_config_path();
            if path.exists() {
                println!("config already exists at {}", path.display());
            } else {
                let config = Config::default();
                config.save()?;
                println!("created config at {}", path.display());
                println!("root_dir = \"{}\"", config.root_dir.display());
            }
        }
    }
    Ok(())
}

fn run_validate() -> Result<()> {
    let config = Config::load()?;
    // Opening the store initialises the git repo if it doesn't exist yet.
    let _store = Store::new(config.root_dir.clone(), &config.context_slug)?;
    let root = &config.root_dir;
    println!("validating {}", root.display());
    println!();

    let mut summary = validate::validate(root)?;

    for err in &summary.structural_errors {
        println!("  error  {err}");
    }
    if !summary.structural_errors.is_empty() {
        println!();
    }

    // Sort by context then kind then slug for deterministic output.
    summary.items.sort_by(|a, b| {
        a.context
            .cmp(&b.context)
            .then(a.kind.to_string().cmp(&b.kind.to_string()))
            .then(a.slug.cmp(&b.slug))
    });

    for report in &summary.items {
        let tag = if report.has_errors() {
            "error"
        } else if !report.warnings.is_empty() {
            "warn "
        } else {
            "ok   "
        };
        println!(
            "  {}  [{}] {}/{}",
            tag, report.context, report.kind, report.slug
        );
        for e in &report.errors {
            println!("         - {e}");
        }
        for w in &report.warnings {
            println!("         ~ {w}");
        }
    }

    println!();
    let total = summary.items.len();
    let errors = summary.error_count();
    let warnings = summary.warning_count();
    println!(
        "  {} item{} checked — {} error{}, {} warning{}",
        total,
        if total == 1 { "" } else { "s" },
        errors,
        if errors == 1 { "" } else { "s" },
        warnings,
        if warnings == 1 { "" } else { "s" },
    );

    if summary.has_errors() {
        std::process::exit(1);
    }
    Ok(())
}

fn run_tui() -> Result<()> {
    let config = Config::load()?;
    let store = Store::new(config.root_dir.clone(), &config.context_slug)?;
    let mut app = app::App::bootstrap(store)?;

    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let result = app.run(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}
