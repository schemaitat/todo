use reqwest::StatusCode;
use thiserror::Error;

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("could not deserialize response: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("unauthorized — check TODO_API_KEY")]
    Unauthorized,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("item changed elsewhere — reopen and retry")]
    StaleNote,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("api error {status}: {body}")]
    Http { status: StatusCode, body: String },

    #[error("invalid url: {0}")]
    Url(#[from] url::ParseError),

    #[error("config error: {0}")]
    Config(String),
}

impl ApiError {
    /// True if the error is a transport/network problem (as opposed to a 4xx/5xx response).
    /// The TUI uses this to drop into read-only mode without nuking cached data.
    pub fn is_network(&self) -> bool {
        matches!(self, ApiError::Network(_))
    }

    /// User-facing one-liner suitable for a status bar.
    pub fn status_line(&self) -> String {
        match self {
            ApiError::Network(_) => "offline — read-only".to_string(),
            ApiError::Unauthorized => "unauthorized — check TODO_API_KEY".to_string(),
            ApiError::StaleNote => "item changed elsewhere — reopen".to_string(),
            ApiError::NotFound(detail) => format!("not found: {}", detail),
            ApiError::Conflict(detail) => format!("conflict: {}", detail),
            ApiError::BadRequest(detail) => format!("bad request: {}", detail),
            ApiError::Http { status, body } => format!("api {}: {}", status, truncate(body, 80)),
            ApiError::Serde(e) => format!("parse error: {}", e),
            ApiError::Url(e) => format!("bad url: {}", e),
            ApiError::Config(e) => format!("config: {}", e),
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}
