use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Todo {
    pub title: String,
    pub done: bool,
    pub created_at: DateTime<Utc>,
}

impl Todo {
    pub fn new(title: String) -> Self {
        Self {
            title,
            done: false,
            created_at: Utc::now(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Note {
    pub title: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Note {
    pub fn new(title: String) -> Self {
        let now = Utc::now();
        Self {
            title,
            body: String::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub todos: Vec<Todo>,
    #[serde(default)]
    pub notes: Vec<Note>,
}

pub fn data_path() -> Result<PathBuf> {
    let dir = dirs::data_local_dir()
        .context("could not resolve local data directory")?
        .join("todo-tui");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("data.json"))
}

pub fn load() -> Result<Store> {
    let path = data_path()?;
    if !path.exists() {
        return Ok(Store::default());
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(Store::default());
    }
    let store: Store = serde_json::from_str(&raw)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(store)
}

pub fn save(store: &Store) -> Result<()> {
    let path = data_path()?;
    let tmp = path.with_extension("json.tmp");
    let raw = serde_json::to_string_pretty(store)?;
    fs::write(&tmp, raw)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}
