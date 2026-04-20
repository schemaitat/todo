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
use todo_api_client::{auth, AuthConfig, Config};

fn main() -> Result<()> {
    let config = maybe_oidc_login(Config::load().map_err(|e| anyhow!("{}", e.status_line()))?)?;
    let mut app =
        app::App::bootstrap(config).map_err(|e| anyhow!("failed to load from api: {}", e))?;

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

/// When OIDC is configured but no valid token is cached, run the interactive browser login
/// before entering the TUI.
fn maybe_oidc_login(config: Config) -> Result<Config> {
    if !matches!(config.auth, AuthConfig::OidcLoginRequired) {
        return Ok(config);
    }
    let oidc = config
        .oidc
        .as_ref()
        .ok_or_else(|| anyhow!("OidcLoginRequired but no OIDC config"))?;
    let tokens = auth::login_interactive(&oidc.keycloak_url, &oidc.realm, &oidc.client_id)
        .map_err(|e| anyhow!("OIDC login failed: {}", e.status_line()))?;
    auth::save_tokens(&tokens)
        .map_err(|e| anyhow!("could not save tokens: {}", e.status_line()))?;
    Ok(Config {
        auth: AuthConfig::Bearer(tokens.access_token),
        ..config
    })
}
