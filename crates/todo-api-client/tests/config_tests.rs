use std::sync::Mutex;

use tempfile::TempDir;
use todo_api_client::{ApiError, AuthConfig, Config};

// Env vars are process-global; serialize tests that mutate them.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn clear_env() {
    for k in [
        "TODO_API_URL",
        "TODO_API_KEY",
        "TODO_CONTEXT",
        "TODO_CONFIG",
    ] {
        // SAFETY: ENV_LOCK serializes env access across these tests.
        unsafe { std::env::remove_var(k) };
    }
}

fn with_env<T>(vars: &[(&str, &str)], f: impl FnOnce() -> T) -> T {
    for (k, v) in vars {
        unsafe { std::env::set_var(k, v) };
    }
    let out = f();
    for (k, _) in vars {
        unsafe { std::env::remove_var(k) };
    }
    out
}

#[test]
fn env_overrides_file() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_env();
    let dir = TempDir::new().unwrap();
    let cfg_path = dir.path().join("config.toml");
    std::fs::write(
        &cfg_path,
        r#"
base_url = "http://file"
api_key  = "file-key"
context_slug = "file-ctx"
"#,
    )
    .unwrap();

    let cfg = with_env(
        &[
            ("TODO_API_URL", "http://env"),
            ("TODO_API_KEY", "env-key"),
            ("TODO_CONTEXT", "env-ctx"),
            ("TODO_CONFIG", cfg_path.to_str().unwrap()),
        ],
        Config::load,
    )
    .unwrap();
    assert_eq!(cfg.base_url, "http://env");
    assert!(matches!(cfg.auth, AuthConfig::ApiKey(ref k) if k == "env-key"));
    assert_eq!(cfg.context_slug, "env-ctx");
}

#[test]
fn file_fills_gaps() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_env();
    let dir = TempDir::new().unwrap();
    let cfg_path = dir.path().join("c.toml");
    std::fs::write(
        &cfg_path,
        r#"
base_url = "http://file"
api_key  = "file-key"
"#,
    )
    .unwrap();
    let cfg = with_env(&[("TODO_CONFIG", cfg_path.to_str().unwrap())], Config::load).unwrap();
    assert_eq!(cfg.base_url, "http://file");
    assert!(matches!(cfg.auth, AuthConfig::ApiKey(ref k) if k == "file-key"));
    assert_eq!(cfg.context_slug, "inbox");
}

#[test]
fn missing_key_errors() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    clear_env();
    let dir = TempDir::new().unwrap();
    let cfg_path = dir.path().join("empty.toml");
    std::fs::write(&cfg_path, "").unwrap();
    let err = with_env(&[("TODO_CONFIG", cfg_path.to_str().unwrap())], Config::load).unwrap_err();
    assert!(matches!(err, ApiError::Config(_)));
}
