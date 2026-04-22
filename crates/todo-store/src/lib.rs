//! Shared DTO types for `todo-tui`, `todo-api-client`, and the FastAPI service.
//!
//! The types here intentionally mirror the JSON shapes returned by the API so the client can
//! `serde_json::from_value` responses directly without an intermediate adapter layer.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Context {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub color: String,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Todo {
    pub id: Uuid,
    pub context_id: Uuid,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub done: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Note {
    pub id: Uuid,
    pub context_id: Uuid,
    pub title: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,
}

/// An event as returned by the API: `kind` is the string tag, `payload` is a free-form JSON blob.
///
/// Use [`Event::parsed_kind`] to interpret known kinds into the typed [`EventKind`] enum.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    pub id: i64,
    #[serde(default)]
    pub context_id: Option<Uuid>,
    pub entity_type: String,
    #[serde(default)]
    pub entity_id: Option<Uuid>,
    pub kind: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    pub ts: DateTime<Utc>,
}

impl Event {
    /// Returns the typed variant if the server reported a kind this client knows about.
    pub fn parsed_kind(&self) -> Option<EventKind> {
        let wire = serde_json::json!({
            "kind": self.kind,
            "payload": self.payload,
        });
        serde_json::from_value(wire).ok()
    }
}

/// Typed event kind, matching the API's `kind` strings and `payload` object.
///
/// Using `adjacently tagged` serde layout means an event line like
/// `{"kind":"TodoCreated","payload":{"title":"..."}}` round-trips through this enum.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "payload")]
pub enum EventKind {
    TodoCreated {
        title: String,
    },
    TodoRenamed {
        title: String,
    },
    TodoToggled {
        done: bool,
    },
    TodoDescriptionEdited {
        #[serde(default)]
        length: Option<usize>,
    },
    TodoDeleted(#[serde(default)] EmptyPayload),
    NoteCreated {
        title: String,
    },
    NoteRenamed {
        title: String,
    },
    NoteEdited {
        #[serde(default)]
        length: Option<usize>,
    },
    NoteDeleted(#[serde(default)] EmptyPayload),
    ContextCreated {
        slug: String,
        name: String,
    },
    ContextRenamed {
        slug: String,
        name: String,
    },
    ContextArchived {
        slug: String,
    },
    TodoMoved {
        to_slug: String,
    },
    NoteMoved {
        to_slug: String,
    },
}

/// Empty `{}` payload; kept as a separate type so `TodoDeleted` / `NoteDeleted` round-trip cleanly.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmptyPayload {}

/// Mirrors `UserOut` from the FastAPI `/me` endpoint.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserInfo {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_todo_created_event() {
        let raw = serde_json::json!({
            "id": 1,
            "context_id": "11111111-1111-1111-1111-111111111111",
            "entity_type": "todo",
            "entity_id": "22222222-2222-2222-2222-222222222222",
            "kind": "TodoCreated",
            "payload": {"title": "write tests"},
            "ts": "2026-04-18T18:00:00Z"
        });
        let ev: Event = serde_json::from_value(raw).unwrap();
        match ev.parsed_kind().unwrap() {
            EventKind::TodoCreated { title } => assert_eq!(title, "write tests"),
            other => panic!("unexpected kind: {:?}", other),
        }
    }

    #[test]
    fn parses_todo_deleted_with_empty_payload() {
        let raw = serde_json::json!({
            "id": 2,
            "context_id": null,
            "entity_type": "todo",
            "entity_id": null,
            "kind": "TodoDeleted",
            "payload": {},
            "ts": "2026-04-18T18:00:00Z"
        });
        let ev: Event = serde_json::from_value(raw).unwrap();
        assert!(matches!(ev.parsed_kind(), Some(EventKind::TodoDeleted(_))));
    }

    #[test]
    fn unknown_kind_returns_none() {
        let raw = serde_json::json!({
            "id": 3,
            "context_id": null,
            "entity_type": "alien",
            "entity_id": null,
            "kind": "AlienEvent",
            "payload": {},
            "ts": "2026-04-18T18:00:00Z"
        });
        let ev: Event = serde_json::from_value(raw).unwrap();
        assert!(ev.parsed_kind().is_none());
    }
}
