mod app;
mod editor;
mod ui;

use anyhow::{anyhow, Result};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::stdout;
use todo_api_client::Client;

fn main() -> Result<()> {
    let client = Client::from_env().map_err(|e| anyhow!("{}", e.status_line()))?;
    let mut app =
        app::App::bootstrap(client).map_err(|e| anyhow!("failed to load from api: {}", e))?;

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
