use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::DEFAULT_CONTEXT;

/// Sparse on-disk representation — unknown fields are ignored so the file
/// can co-exist with the old API-client config without failing to parse.
#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    root_dir: Option<PathBuf>,
    context_slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Root directory containing `<context>/todos/` and `<context>/notes/` subdirectories.
    pub root_dir: PathBuf,
    /// Active context slug (default: "inbox").
    pub context_slug: String,
    /// Path this config was loaded from; used by `save()`. Not persisted.
    #[serde(skip)]
    loaded_from: PathBuf,
}

impl Config {
    pub fn load() -> Result<Self> {
        Self::load_from(Self::default_config_path())
    }

    pub fn load_from(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self {
                loaded_from: path.to_path_buf(),
                ..Self::default()
            });
        }
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let file: FileConfig =
            toml::from_str(&raw).with_context(|| format!("parsing config {}", path.display()))?;
        let defaults = Self::default();
        Ok(Self {
            root_dir: file.root_dir.unwrap_or(defaults.root_dir),
            context_slug: file.context_slug.unwrap_or(defaults.context_slug),
            loaded_from: path.to_path_buf(),
        })
    }

    /// Write the config back to the path it was loaded from.
    pub fn save(&self) -> Result<()> {
        let path = if self.loaded_from.as_os_str().is_empty() {
            Self::default_config_path()
        } else {
            self.loaded_from.clone()
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("creating config dir")?;
        }
        let content = toml::to_string_pretty(self).context("serializing config")?;
        std::fs::write(&path, content).with_context(|| format!("writing config {}", path.display()))
    }

    pub fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("todo-tui")
            .join("config.toml")
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root_dir: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".local")
                .join("share")
                .join("todo-tui"),
            context_slug: DEFAULT_CONTEXT.to_string(),
            loaded_from: PathBuf::new(),
        }
    }
}
