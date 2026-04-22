//! Blocking HTTP client for the todo-api FastAPI service.
//!
//! The TUI (and any other single-threaded Rust consumer) can call these methods directly without
//! pulling in a tokio runtime. Each method does one request, parses JSON into the shared DTOs from
//! [`todo_store`], and returns a typed [`ApiError`] on failure.

pub mod auth;
pub mod config;
pub mod error;

mod client;

pub use client::{Client, PatchedNote, PatchedTodo};
pub use config::{AuthConfig, Config, OidcConfig};
pub use error::{ApiError, ApiResult};
pub use todo_store::{Context, EmptyPayload, Event, EventKind, Note, Todo, UserInfo};
