use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

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
}

#[derive(Debug, Clone)]
pub struct Config {
    pub base_url: String,
    pub api_key: String,
    pub context_slug: String,
}

impl Config {
    /// Load config with precedence: env vars > `$TODO_CONFIG` or `~/.config/todo-tui/config.toml`
    /// > built-in defaults (except `api_key`, which has no default and must be set somewhere).
    pub fn load() -> ApiResult<Self> {
        let file = load_file_config(None)?;
        Self::resolve_with_file(file)
    }

    /// Load explicitly from the given config file path (useful for tests).
    pub fn load_with_file(path: impl AsRef<Path>) -> ApiResult<Self> {
        let file = load_file_config(Some(path.as_ref()))?;
        Self::resolve_with_file(file)
    }

    fn resolve_with_file(file: FileConfig) -> ApiResult<Self> {
        let base_url = env::var(ENV_URL)
            .ok()
            .or(file.base_url)
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let api_key = env::var(ENV_KEY).ok().or(file.api_key).ok_or_else(|| {
            ApiError::Config(format!(
                "missing api key — set {} or write it to the config file",
                ENV_KEY
            ))
        })?;
        let context_slug = env::var(ENV_CONTEXT)
            .ok()
            .or(file.context_slug)
            .unwrap_or_else(|| DEFAULT_CONTEXT.to_string());
        Ok(Self {
            base_url,
            api_key,
            context_slug,
        })
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

fn default_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("todo-tui").join("config.toml"))
}
