use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::auth::{load_tokens, try_refresh};
use crate::error::{ApiError, ApiResult};

const ENV_URL: &str = "TODO_API_URL";
const ENV_KEY: &str = "TODO_API_KEY";
const ENV_CONTEXT: &str = "TODO_CONTEXT";
const ENV_CONFIG_PATH: &str = "TODO_CONFIG";
const DEFAULT_CONTEXT: &str = "inbox";
const DEFAULT_BASE_URL: &str = "http://localhost:8000";

#[derive(Debug, Clone, Deserialize, Default)]
struct FileConfig {
    base_url: Option<String>,
    api_key: Option<String>,
    context_slug: Option<String>,
    keycloak_url: Option<String>,
    oidc_realm: Option<String>,
    oidc_client_id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AuthConfig {
    ApiKey(String),
    Bearer(String),
    /// OIDC is configured but no valid token is cached — login required.
    OidcLoginRequired,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub base_url: String,
    pub auth: AuthConfig,
    pub context_slug: String,
    /// OIDC settings, present when keycloak is configured.
    pub oidc: Option<OidcConfig>,
}

#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub keycloak_url: String,
    pub realm: String,
    pub client_id: String,
}

impl Config {
    /// Load config with precedence: env vars > `$TODO_CONFIG` or `~/.config/todo-tui/config.toml`
    /// > built-in defaults. Prefers a valid Bearer token (from saved tokens) over API key.
    pub fn load() -> ApiResult<Self> {
        let file = load_file_config(None)?;
        Self::resolve_with_file(file)
    }

    pub fn load_with_file(path: impl AsRef<Path>) -> ApiResult<Self> {
        let file = load_file_config(Some(path.as_ref()))?;
        Self::resolve_with_file(file)
    }

    fn resolve_with_file(file: FileConfig) -> ApiResult<Self> {
        let base_url = env_var_nonempty(ENV_URL)
            .or_else(|| file.base_url.clone())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        let context_slug = env_var_nonempty(ENV_CONTEXT)
            .or_else(|| file.context_slug.clone())
            .unwrap_or_else(|| DEFAULT_CONTEXT.to_string());

        let oidc = resolve_oidc_config(&file);
        let auth = resolve_auth(&file, oidc.as_ref())?;

        Ok(Self {
            base_url,
            auth,
            context_slug,
            oidc,
        })
    }
}

fn resolve_oidc_config(file: &FileConfig) -> Option<OidcConfig> {
    let keycloak_url =
        env_var_nonempty("TODO_KEYCLOAK_URL").or_else(|| file.keycloak_url.clone())?;
    let realm = env_var_nonempty("TODO_OIDC_REALM")
        .or_else(|| file.oidc_realm.clone())
        .unwrap_or_else(|| "todo".to_string());
    let client_id = env_var_nonempty("TODO_OIDC_CLIENT_ID")
        .or_else(|| file.oidc_client_id.clone())
        .unwrap_or_else(|| "todo-tui".to_string());
    Some(OidcConfig {
        keycloak_url,
        realm,
        client_id,
    })
}

fn resolve_auth(file: &FileConfig, oidc: Option<&OidcConfig>) -> ApiResult<AuthConfig> {
    // When OIDC is configured, only Bearer tokens are accepted.
    if let Some(oidc_cfg) = oidc {
        if let Some(tokens) = load_tokens() {
            if !tokens.is_expired() {
                return Ok(AuthConfig::Bearer(tokens.access_token));
            }
            if let Some(rt) = &tokens.refresh_token {
                if let Ok(refreshed) = try_refresh(
                    &oidc_cfg.keycloak_url,
                    &oidc_cfg.realm,
                    &oidc_cfg.client_id,
                    rt,
                ) {
                    if let Ok(()) = crate::auth::save_tokens(&refreshed) {
                        return Ok(AuthConfig::Bearer(refreshed.access_token));
                    }
                }
            }
        }
        return Ok(AuthConfig::OidcLoginRequired);
    }

    // No OIDC — fall back to API key.
    let api_key = env_var_nonempty(ENV_KEY).or_else(|| file.api_key.clone());
    match api_key {
        Some(k) => Ok(AuthConfig::ApiKey(k)),
        None => Err(ApiError::Config(format!(
            "no valid auth: set {ENV_KEY}, or run ':auth login' in the TUI"
        ))),
    }
}

fn load_file_config(explicit_path: Option<&Path>) -> ApiResult<FileConfig> {
    let path = match explicit_path {
        Some(p) => Some(p.to_path_buf()),
        None => config_path_from_env().or_else(default_config_path),
    };
    let Some(path) = path else {
        return Ok(FileConfig::default());
    };
    if !path.exists() {
        return Ok(FileConfig::default());
    }
    let raw = fs::read_to_string(&path)
        .map_err(|e| ApiError::Config(format!("reading {}: {}", path.display(), e)))?;
    toml::from_str(&raw).map_err(|e| ApiError::Config(format!("parsing {}: {}", path.display(), e)))
}

fn config_path_from_env() -> Option<PathBuf> {
    env::var_os(ENV_CONFIG_PATH).map(PathBuf::from)
}

fn env_var_nonempty(key: &str) -> Option<String> {
    env::var(key).ok().filter(|v| !v.is_empty())
}

fn default_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("todo-tui").join("config.toml"))
}
