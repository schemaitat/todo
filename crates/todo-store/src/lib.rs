use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Todo {
    pub id: Uuid,
    pub title: String,
    pub done: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Todo {
    pub fn new(title: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            title,
            done: false,
            created_at: Utc::now(),
            deleted_at: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Note {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Note {
    pub fn new(title: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title,
            body: String::new(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub ts: DateTime<Utc>,
    pub kind: EventKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EventKind {
    TodoCreated { id: Uuid, title: String },
    TodoRenamed { id: Uuid, title: String },
    TodoToggled { id: Uuid, done: bool },
    TodoDeleted { id: Uuid },
    NoteCreated { id: Uuid, title: String },
    NoteRenamed { id: Uuid, title: String },
    NoteEdited { id: Uuid, body: String },
    NoteDeleted { id: Uuid },
}

fn data_dir() -> Result<PathBuf> {
    let dir = dirs::data_local_dir()
        .context("could not resolve local data directory")?
        .join("todo-tui");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn data_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("data.json"))
}

pub fn events_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("events.jsonl"))
}

pub fn load() -> Result<Store> {
    let path = data_path()?;
    if !path.exists() {
        return Ok(Store::default());
    }
    let raw = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(Store::default());
    }
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn save(store: &Store) -> Result<()> {
    let path = data_path()?;
    let tmp = path.with_extension("json.tmp");
    let raw = serde_json::to_string_pretty(store)?;
    fs::write(&tmp, raw)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn append_event(kind: EventKind) -> Result<()> {
    let path = events_path()?;
    let event = Event {
        ts: Utc::now(),
        kind,
    };
    let line = serde_json::to_string(&event)?;
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(file, "{}", line)?;
    Ok(())
}

pub fn load_events() -> Result<Vec<Event>> {
    let path = events_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(&path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<Event>(&line) {
            events.push(event);
        }
    }
    Ok(events)
}
