use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};

const CALLBACK_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcTokens {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Unix timestamp (seconds) after which the access token is considered expired.
    pub expires_at: u64,
}

impl OidcTokens {
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now >= self.expires_at.saturating_sub(30)
    }
}

/// Load tokens from `~/.config/todo-tui/tokens.json`.
pub fn load_tokens() -> Option<OidcTokens> {
    let path = tokens_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Persist tokens to `~/.config/todo-tui/tokens.json`.
pub fn save_tokens(tokens: &OidcTokens) -> ApiResult<()> {
    let path = tokens_path().ok_or_else(|| ApiError::Config("cannot find config dir".into()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ApiError::Config(format!("creating config dir: {e}")))?;
    }
    let json = serde_json::to_string_pretty(tokens)
        .map_err(|e| ApiError::Config(format!("serializing tokens: {e}")))?;
    std::fs::write(&path, json)
        .map_err(|e| ApiError::Config(format!("writing {}: {e}", path.display())))?;
    Ok(())
}

/// Remove saved tokens (logout).
pub fn clear_tokens() {
    if let Some(path) = tokens_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// Revoke the refresh token at Keycloak's end_session endpoint so the server-side
/// session and refresh token are invalidated. Best-effort: errors are ignored because
/// we still want local credentials cleared regardless.
pub fn revoke_refresh_token(keycloak_url: &str, realm: &str, client_id: &str, refresh_token: &str) {
    let logout_url = format!("{keycloak_url}/realms/{realm}/protocol/openid-connect/logout");
    let mut params = HashMap::new();
    params.insert("client_id", client_id);
    params.insert("refresh_token", refresh_token);

    let Ok(http) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    else {
        return;
    };
    let _ = http.post(&logout_url).form(&params).send();
}

/// Full logout: revoke refresh token at Keycloak (if present) and delete saved tokens.
pub fn logout(keycloak_url: &str, realm: &str, client_id: &str) {
    if let Some(tokens) = load_tokens() {
        if let Some(rt) = tokens.refresh_token.as_deref() {
            revoke_refresh_token(keycloak_url, realm, client_id, rt);
        }
    }
    clear_tokens();
}

fn tokens_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("todo-tui").join("tokens.json"))
}

/// Try to refresh using the stored refresh_token. Returns `None` if no refresh token available.
pub fn try_refresh(
    keycloak_url: &str,
    realm: &str,
    client_id: &str,
    refresh_token: &str,
) -> ApiResult<OidcTokens> {
    let token_url = format!("{keycloak_url}/realms/{realm}/protocol/openid-connect/token");
    let mut params = HashMap::new();
    params.insert("grant_type", "refresh_token");
    params.insert("client_id", client_id);
    params.insert("refresh_token", refresh_token);

    let http = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(ApiError::from)?;

    let resp = http
        .post(&token_url)
        .form(&params)
        .send()
        .map_err(ApiError::from)?;

    if !resp.status().is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(ApiError::Config(format!("token refresh failed: {body}")));
    }

    parse_token_response(resp.text().unwrap_or_default())
}

/// Open browser and wait for the PKCE authorization code callback, then exchange for tokens.
pub fn login_interactive(
    keycloak_url: &str,
    realm: &str,
    client_id: &str,
) -> ApiResult<OidcTokens> {
    let code_verifier = generate_code_verifier();
    let code_challenge = pkce_challenge(&code_verifier);

    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| ApiError::Config(format!("cannot bind local server: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| ApiError::Config(format!("local_addr: {e}")))?
        .port();
    let redirect_uri = format!("http://localhost:{port}/callback");

    let auth_url = format!(
        "{keycloak_url}/realms/{realm}/protocol/openid-connect/auth\
        ?response_type=code\
        &client_id={client_id}\
        &redirect_uri={encoded_redirect}\
        &code_challenge={code_challenge}\
        &code_challenge_method=S256\
        &scope=openid+email+profile",
        encoded_redirect = url_encode(&redirect_uri),
    );

    eprintln!("Opening browser for Keycloak login...");
    if open::that(&auth_url).is_err() {
        eprintln!("Could not open browser. Visit manually:\n{auth_url}");
    }

    eprintln!("Waiting for login callback (timeout {CALLBACK_TIMEOUT_SECS}s)...");
    let code = wait_for_callback(listener)?;
    let token_url = format!("{keycloak_url}/realms/{realm}/protocol/openid-connect/token");
    exchange_code(&token_url, client_id, &code, &redirect_uri, &code_verifier)
}

fn generate_code_verifier() -> String {
    // Use two UUIDs (v4 uses OS CSPRNG) for 32 bytes of randomness.
    let b1 = *Uuid::new_v4().as_bytes();
    let b2 = *Uuid::new_v4().as_bytes();
    let bytes: Vec<u8> = b1.iter().chain(b2.iter()).copied().collect();
    URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

fn wait_for_callback(listener: TcpListener) -> ApiResult<String> {
    listener
        .set_nonblocking(false)
        .map_err(|e| ApiError::Config(format!("set_nonblocking: {e}")))?;

    // Accept connections until we get the code or time out.
    // A simple approach: set SO_RCVTIMEO via a raw socket option is complex; instead we
    // spawn a timer thread that connects to force accept() to unblock on timeout.
    let addr = listener.local_addr().ok();
    let handle = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(CALLBACK_TIMEOUT_SECS));
        if let Some(a) = addr {
            let _ = std::net::TcpStream::connect(a);
        }
    });

    if let Some(stream) = listener.incoming().next() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => {
                return Err(ApiError::Config(
                    "login timed out waiting for browser callback".into(),
                ));
            }
        };
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut first_line = String::new();
        let _ = reader.read_line(&mut first_line);

        if let Some(code) = extract_code_from_request_line(&first_line) {
            let body = "<html><body><h2>Login successful.</h2><p>You can close this tab.</p></body></html>";
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            drop(handle);
            return Ok(code);
        }
        // Timer thread connected without a code — timeout.
    }

    Err(ApiError::Config(
        "login timed out waiting for browser callback".into(),
    ))
}

fn extract_code_from_request_line(line: &str) -> Option<String> {
    // "GET /callback?code=abc&session_state=xyz HTTP/1.1"
    let path = line.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("code=") {
            return Some(url_decode(value));
        }
    }
    None
}

fn exchange_code(
    token_url: &str,
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> ApiResult<OidcTokens> {
    let mut params = HashMap::new();
    params.insert("grant_type", "authorization_code");
    params.insert("client_id", client_id);
    params.insert("code", code);
    params.insert("redirect_uri", redirect_uri);
    params.insert("code_verifier", code_verifier);

    let http = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(ApiError::from)?;

    let resp = http
        .post(token_url)
        .form(&params)
        .send()
        .map_err(ApiError::from)?;

    if !resp.status().is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(ApiError::Config(format!("token exchange failed: {body}")));
    }

    parse_token_response(resp.text().unwrap_or_default())
}

#[derive(Deserialize)]
struct RawTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

fn parse_token_response(body: String) -> ApiResult<OidcTokens> {
    let raw: RawTokenResponse = serde_json::from_str(&body)
        .map_err(|e| ApiError::Config(format!("parsing token response: {e}")))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Ok(OidcTokens {
        access_token: raw.access_token,
        refresh_token: raw.refresh_token,
        expires_at: now + raw.expires_in,
    })
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
                out.push(char::from_digit((b & 0xf) as u32, 16).unwrap_or('0'));
            }
        }
    }
    out
}

fn url_decode(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push(((hi << 4) | lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
