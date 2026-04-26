use chrono::{DateTime, Utc};

#[derive(Clone, Debug)]
pub struct Todo {
    pub slug: String,
    pub title: String,
    /// One-line summary stored in the front matter `description` field.
    pub description: String,
    /// Body of the CONTENT.md file below the front matter.
    pub body: String,
    pub done: bool,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct Note {
    pub slug: String,
    pub title: String,
    /// One-line summary stored in the front matter `description` field.
    pub description: String,
    /// Body of the CONTENT.md file below the front matter.
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}
