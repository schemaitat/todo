use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::blocking::{Client as HttpClient, RequestBuilder, Response};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::StatusCode;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use url::Url;
use uuid::Uuid;

use crate::config::Config;
use crate::error::{ApiError, ApiResult};
use todo_store::{Context, Event, Note, Todo};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const API_KEY_HEADER: &str = "X-API-Key";

#[derive(Debug, Clone)]
pub struct Client {
    http: HttpClient,
    base: Url,
    context_slug: String,
}

/// Body passed to [`Client::patch_note`]; each field is optional so partial updates stay expressive.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PatchedNote {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

impl Client {
    pub fn from_env() -> ApiResult<Self> {
        Self::new(Config::load()?)
    }

    pub fn new(config: Config) -> ApiResult<Self> {
        let mut headers = HeaderMap::new();
        let mut auth = HeaderValue::from_str(&config.api_key)
            .map_err(|e| ApiError::Config(format!("invalid api key: {}", e)))?;
        auth.set_sensitive(true);
        headers.insert(API_KEY_HEADER, auth);
        let http = HttpClient::builder()
            .timeout(DEFAULT_TIMEOUT)
            .default_headers(headers)
            .build()?;
        let base = Url::parse(&config.base_url)?;
        Ok(Self {
            http,
            base,
            context_slug: config.context_slug,
        })
    }

    pub fn active_context_slug(&self) -> &str {
        &self.context_slug
    }

    pub fn set_active_context(&mut self, slug: impl Into<String>) {
        self.context_slug = slug.into();
    }

    pub fn ping(&self) -> ApiResult<()> {
        let url = self.url(["health"])?;
        self.http.get(url).send()?.error_for_status()?;
        Ok(())
    }

    // --- contexts ---------------------------------------------------------

    pub fn list_contexts(&self) -> ApiResult<Vec<Context>> {
        self.get_json::<Vec<Context>>(["contexts"])
    }

    pub fn create_context(
        &self,
        slug: &str,
        name: &str,
        color: Option<&str>,
    ) -> ApiResult<Context> {
        let body = json!({ "slug": slug, "name": name, "color": color });
        self.post_json(["contexts"], &body)
    }

    pub fn archive_context(&self, slug: &str) -> ApiResult<()> {
        self.delete(["contexts", slug])
    }

    // --- todos ------------------------------------------------------------

    pub fn list_todos(&self, context_slug: &str) -> ApiResult<Vec<Todo>> {
        self.get_json::<Vec<Todo>>(["contexts", context_slug, "todos"])
    }

    pub fn create_todo(&self, context_slug: &str, title: &str) -> ApiResult<Todo> {
        self.post_json(
            ["contexts", context_slug, "todos"],
            &json!({ "title": title }),
        )
    }

    pub fn rename_todo(&self, id: Uuid, title: &str) -> ApiResult<Todo> {
        self.patch_json(["todos", &id.to_string()], &json!({ "title": title }))
    }

    pub fn set_todo_done(&self, id: Uuid, done: bool) -> ApiResult<Todo> {
        self.patch_json(["todos", &id.to_string()], &json!({ "done": done }))
    }

    pub fn delete_todo(&self, id: Uuid) -> ApiResult<()> {
        self.delete(["todos", &id.to_string()])
    }

    // --- notes ------------------------------------------------------------

    pub fn list_notes(&self, context_slug: &str) -> ApiResult<Vec<Note>> {
        self.get_json::<Vec<Note>>(["contexts", context_slug, "notes"])
    }

    pub fn create_note(&self, context_slug: &str, title: &str) -> ApiResult<Note> {
        self.post_json(
            ["contexts", context_slug, "notes"],
            &json!({ "title": title }),
        )
    }

    pub fn rename_note(&self, id: Uuid, title: &str) -> ApiResult<Note> {
        self.patch_json(["notes", &id.to_string()], &json!({ "title": title }))
    }

    pub fn patch_note(
        &self,
        id: Uuid,
        patch: &PatchedNote,
        if_match: Option<DateTime<Utc>>,
    ) -> ApiResult<Note> {
        let url = self.url(["notes", &id.to_string()])?;
        let mut builder = self.http.patch(url).json(patch);
        if let Some(ts) = if_match {
            let fmt = ts.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
            builder = builder.header("If-Match", fmt);
        }
        Self::decode_json(send(builder)?)
    }

    pub fn delete_note(&self, id: Uuid) -> ApiResult<()> {
        self.delete(["notes", &id.to_string()])
    }

    // --- events -----------------------------------------------------------

    pub fn list_events(&self, context_slug: Option<&str>, limit: usize) -> ApiResult<Vec<Event>> {
        let mut url = self.url(["events"])?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("limit", &limit.to_string());
            if let Some(slug) = context_slug {
                q.append_pair("context", slug);
            }
        }
        let resp = send(self.http.get(url))?;
        Self::decode_json(resp)
    }

    // --- snapshot ---------------------------------------------------------

    pub fn snapshot_plain(&self, context_slug: &str) -> ApiResult<String> {
        self.snapshot(context_slug, "plain")
    }

    pub fn snapshot_html(&self, context_slug: &str) -> ApiResult<String> {
        self.snapshot(context_slug, "html")
    }

    fn snapshot(&self, context_slug: &str, fmt: &str) -> ApiResult<String> {
        let mut url = self.url(["snapshot"])?;
        url.query_pairs_mut()
            .append_pair("context", context_slug)
            .append_pair("format", fmt);
        let resp = send(self.http.get(url))?;
        let resp = check_status(resp)?;
        Ok(resp.text()?)
    }

    // --- internals --------------------------------------------------------

    fn url<'a>(&self, segments: impl IntoIterator<Item = &'a str>) -> ApiResult<Url> {
        let mut url = self.base.clone();
        {
            let mut seg = url
                .path_segments_mut()
                .map_err(|_| ApiError::Config("base_url cannot be a base".to_string()))?;
            for s in segments {
                seg.push(s);
            }
        }
        Ok(url)
    }

    fn get_json<T: DeserializeOwned>(
        &self,
        path: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> ApiResult<T> {
        let url = self.url_str(path)?;
        Self::decode_json(send(self.http.get(url))?)
    }

    fn post_json<T: DeserializeOwned>(
        &self,
        path: impl IntoIterator<Item = impl AsRef<str>>,
        body: &impl Serialize,
    ) -> ApiResult<T> {
        let url = self.url_str(path)?;
        Self::decode_json(send(self.http.post(url).json(body))?)
    }

    fn patch_json<T: DeserializeOwned>(
        &self,
        path: impl IntoIterator<Item = impl AsRef<str>>,
        body: &impl Serialize,
    ) -> ApiResult<T> {
        let url = self.url_str(path)?;
        Self::decode_json(send(self.http.patch(url).json(body))?)
    }

    fn delete(&self, path: impl IntoIterator<Item = impl AsRef<str>>) -> ApiResult<()> {
        let url = self.url_str(path)?;
        check_status(send(self.http.delete(url))?)?;
        Ok(())
    }

    fn url_str(&self, path: impl IntoIterator<Item = impl AsRef<str>>) -> ApiResult<Url> {
        let iter = path.into_iter();
        let collected: Vec<String> = iter.map(|s| s.as_ref().to_string()).collect();
        self.url(collected.iter().map(|s| s.as_str()))
    }

    fn decode_json<T: DeserializeOwned>(resp: Response) -> ApiResult<T> {
        let resp = check_status(resp)?;
        let body = resp.text()?;
        if body.is_empty() {
            return Err(ApiError::Serde(serde::de::Error::custom(
                "empty response body",
            )));
        }
        serde_json::from_str(&body).map_err(Into::into)
    }
}

fn send(builder: RequestBuilder) -> ApiResult<Response> {
    builder.send().map_err(ApiError::from)
}

fn check_status(resp: Response) -> ApiResult<Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().unwrap_or_default();
    Err(match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ApiError::Unauthorized,
        StatusCode::NOT_FOUND => ApiError::NotFound(extract_detail(&body)),
        StatusCode::CONFLICT => ApiError::Conflict(extract_detail(&body)),
        StatusCode::PRECONDITION_FAILED => ApiError::StaleNote,
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => {
            ApiError::BadRequest(extract_detail(&body))
        }
        _ => ApiError::Http { status, body },
    })
}

fn extract_detail(body: &str) -> String {
    #[derive(serde::Deserialize)]
    struct D {
        detail: Option<serde_json::Value>,
    }
    match serde_json::from_str::<D>(body) {
        Ok(d) => match d.detail {
            Some(serde_json::Value::String(s)) => s,
            Some(v) => v.to_string(),
            None => body.to_string(),
        },
        Err(_) => body.to_string(),
    }
}
