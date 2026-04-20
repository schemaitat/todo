//! Integration tests for the blocking `Client`.
//!
//! The wiremock server runs on a dedicated multi-thread runtime whose worker threads drive its
//! TCP listener. The blocking `reqwest::Client` then runs on the test's main thread, outside any
//! tokio context — dropping it there avoids the "runtime dropped in async context" panic that
//! `#[tokio::test] + spawn_blocking` triggers.

use chrono::{DateTime, Utc};
use serde_json::json;
use tokio::runtime::Runtime;
use uuid::Uuid;
use wiremock::matchers::{body_json, header_regex, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use todo_api_client::{ApiError, AuthConfig, Client, Config, EventKind, PatchedNote};

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(1)
        .build()
        .expect("runtime")
}

fn make_client(server: &MockServer) -> Client {
    Client::new(Config {
        base_url: server.uri(),
        auth: AuthConfig::ApiKey("todo_test_key".to_string()),
        context_slug: "inbox".to_string(),
        oidc: None,
    })
    .expect("client")
}

fn sample_context() -> serde_json::Value {
    json!({
        "id": "11111111-1111-1111-1111-111111111111",
        "slug": "inbox",
        "name": "inbox",
        "color": "#8888ff",
        "position": 0,
        "created_at": "2026-04-18T00:00:00Z",
        "archived_at": null,
    })
}

fn sample_todo() -> serde_json::Value {
    json!({
        "id": "22222222-2222-2222-2222-222222222222",
        "context_id": "11111111-1111-1111-1111-111111111111",
        "title": "ship it",
        "done": false,
        "created_at": "2026-04-18T00:00:00Z",
        "updated_at": "2026-04-18T00:00:00Z",
        "deleted_at": null,
    })
}

#[test]
fn list_contexts_sends_api_key() {
    let rt = runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/contexts"))
            .and(header_regex("X-API-Key", "^todo_test_key$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(vec![sample_context()]))
            .mount(&server)
            .await;
    });

    let client = make_client(&server);
    let contexts = client.list_contexts().unwrap();
    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts[0].slug, "inbox");
}

#[test]
fn create_todo_posts_title() {
    let rt = runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("POST"))
            .and(path("/contexts/inbox/todos"))
            .and(body_json(json!({"title": "ship it"})))
            .respond_with(ResponseTemplate::new(201).set_body_json(sample_todo()))
            .mount(&server)
            .await;
    });

    let client = make_client(&server);
    let todo = client.create_todo("inbox", "ship it").unwrap();
    assert_eq!(todo.title, "ship it");
}

#[test]
fn toggle_todo_patches_done_field() {
    let rt = runtime();
    let server = rt.block_on(MockServer::start());
    let id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
    let mut toggled = sample_todo();
    toggled["done"] = json!(true);
    rt.block_on(async {
        Mock::given(method("PATCH"))
            .and(path(format!("/todos/{}", id)))
            .and(body_json(json!({"done": true})))
            .respond_with(ResponseTemplate::new(200).set_body_json(toggled))
            .mount(&server)
            .await;
    });

    let client = make_client(&server);
    let todo = client.set_todo_done(id, true).unwrap();
    assert!(todo.done);
}

#[test]
fn delete_todo_returns_ok_on_204() {
    let rt = runtime();
    let server = rt.block_on(MockServer::start());
    let id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
    rt.block_on(async {
        Mock::given(method("DELETE"))
            .and(path(format!("/todos/{}", id)))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
    });

    let client = make_client(&server);
    client.delete_todo(id).unwrap();
}

#[test]
fn patch_note_if_match_header_is_set() {
    let rt = runtime();
    let server = rt.block_on(MockServer::start());
    let id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
    rt.block_on(async {
        // wiremock's header() matcher splits values on ','; HTTP dates contain commas
        // ("Sat, 18 Apr ..."), so use header_regex for exact RFC 7231 date matching.
        Mock::given(method("PATCH"))
            .and(path(format!("/notes/{}", id)))
            .and(header_regex("If-Match", r"^Sat, 18 Apr 2026 00:00:00 GMT$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": id.to_string(),
                "context_id": "11111111-1111-1111-1111-111111111111",
                "title": "t",
                "body": "b",
                "created_at": "2026-04-18T00:00:00Z",
                "updated_at": "2026-04-18T00:00:00Z",
                "deleted_at": null,
            })))
            .mount(&server)
            .await;
    });

    let client = make_client(&server);
    let ts: DateTime<Utc> = DateTime::parse_from_rfc3339("2026-04-18T00:00:00Z")
        .unwrap()
        .into();
    let patch = PatchedNote {
        title: None,
        body: Some("b".into()),
    };
    client.patch_note(id, &patch, Some(ts)).unwrap();
}

#[test]
fn precondition_failed_becomes_stale_note() {
    let rt = runtime();
    let server = rt.block_on(MockServer::start());
    let id = Uuid::new_v4();
    rt.block_on(async {
        Mock::given(method("PATCH"))
            .and(path(format!("/notes/{}", id)))
            .respond_with(ResponseTemplate::new(412).set_body_json(json!({"detail": "stale"})))
            .mount(&server)
            .await;
    });

    let client = make_client(&server);
    let patch = PatchedNote::default();
    let err = client.patch_note(id, &patch, None).unwrap_err();
    assert!(matches!(err, ApiError::StaleNote));
}

#[test]
fn unauthorized_maps_to_enum() {
    let rt = runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/contexts"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({"detail": "nope"})))
            .mount(&server)
            .await;
    });

    let client = make_client(&server);
    let err = client.list_contexts().unwrap_err();
    assert!(matches!(err, ApiError::Unauthorized));
}

#[test]
fn conflict_maps_to_enum() {
    let rt = runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("POST"))
            .and(path("/contexts"))
            .respond_with(
                ResponseTemplate::new(409).set_body_json(json!({"detail": "slug already exists"})),
            )
            .mount(&server)
            .await;
    });

    let client = make_client(&server);
    let err = client.create_context("x", "X", None).unwrap_err();
    match err {
        ApiError::Conflict(detail) => assert!(detail.contains("slug")),
        other => panic!("unexpected: {:?}", other),
    }
}

#[test]
fn list_events_passes_filters() {
    let rt = runtime();
    let server = rt.block_on(MockServer::start());
    let events = json!([
        {
            "id": 2,
            "context_id": "11111111-1111-1111-1111-111111111111",
            "entity_type": "todo",
            "entity_id": "22222222-2222-2222-2222-222222222222",
            "kind": "TodoCreated",
            "payload": {"title": "a"},
            "ts": "2026-04-18T00:00:00Z"
        }
    ]);
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/events"))
            .and(query_param("limit", "50"))
            .and(query_param("context", "inbox"))
            .respond_with(ResponseTemplate::new(200).set_body_json(events))
            .mount(&server)
            .await;
    });

    let client = make_client(&server);
    let events = client.list_events(Some("inbox"), 50).unwrap();
    assert_eq!(events.len(), 1);
    match events[0].parsed_kind().unwrap() {
        EventKind::TodoCreated { title } => assert_eq!(title, "a"),
        other => panic!("unexpected kind {:?}", other),
    }
}

#[test]
fn snapshot_plain_returns_body_text() {
    let rt = runtime();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .and(path("/snapshot"))
            .and(query_param("context", "inbox"))
            .and(query_param("format", "plain"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("# snap\n\n## Open todos (0)\n_none_\n"),
            )
            .mount(&server)
            .await;
    });

    let client = make_client(&server);
    let snap = client.snapshot_plain("inbox").unwrap();
    assert!(snap.contains("# snap"));
}
