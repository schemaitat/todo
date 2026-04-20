# todo

Personal todo/notes system with a terminal UI, centralized API, and email automation.

## Components

| Component | Location | Role |
|---|---|---|
| `todo-tui` | `crates/todo-tui` | Ratatui terminal UI |
| `todo-api-client` | `crates/todo-api-client` | Blocking HTTP client used by the TUI |
| `todo-store` | `crates/todo-store` | Shared DTO types (Todo, Note, Event, Context) |
| `todo-mailer` | `crates/todo-mailer` | Rust email helper library |
| FastAPI service | `api/` | REST backend, Postgres via SQLAlchemy 2.x async |
| Prefect automation | `automation/` | Scheduled daily snapshot email via SMTP |
| Keycloak | `deploy/keycloak/` | OIDC provider (realm `todo`) for Bearer-token auth |
| Postgres | Docker volume `pgdata` | Single source of truth |

## Architecture

```mermaid
graph LR
    TUI["todo-tui<br/>(Ratatui)"] -->|"HTTPS · Bearer / X-API-Key"| Caddy
    Browser["Browser<br/>(OIDC login)"] -->|HTTPS| Caddy

    subgraph Edge["VPS edge"]
        Caddy["Caddy<br/>(TLS · HTTP/3)"]
    end

    subgraph Internal["Docker network"]
        API["FastAPI<br/>(api/)"]
        KC["Keycloak<br/>(realm: todo)"]
        PG[("Postgres")]
        Automation["Prefect<br/>(automation/)"]
    end

    Caddy -->|"api.&lt;domain&gt; &rarr; api:8000"| API
    Caddy -->|"auth.&lt;domain&gt; &rarr; keycloak:8080"| KC
    API <-->|"SQLAlchemy async"| PG
    KC <-->|JDBC| PG
    API -->|"JWKS (RS256)"| KC
    Automation -->|"HTTP · X-API-Key"| API
    Automation -->|SMTP| Mail["Email"]
```

Caddy terminates TLS on the VPS (automatic certs via Let's Encrypt) and routes by host: `api.<domain>` &rarr; `api:8000`, `auth.<domain>` &rarr; `keycloak:8080`. The API and Keycloak containers never bind to public ports in production — only Caddy is exposed on 80/443 (plus UDP 443 for HTTP/3). See `deploy/Caddyfile` and `deploy/docker-compose.prod.yml`.

## How they connect

- The TUI reads config from `~/.config/todo-tui/config.toml` or env vars and talks to the API over HTTP.
- All mutations go through the API; the TUI caches the last fetched lists locally in memory.
- The Prefect automation runs on a cron schedule (weekdays 08:00 UTC) and sends an HTML snapshot email grouped by context.
- Events (create/rename/toggle/delete) are appended to an `events` table in the same transaction as the mutation — the TUI's `:history` view reads them via `GET /events`.

## Authentication

The API accepts two credential types, checked in this order:

1. **`Authorization: Bearer <token>`** — OIDC access token from Keycloak (realm `todo`, RS256). Tokens are verified against the realm's JWKS endpoint and users are auto-provisioned on first sign-in (linked to an existing user by email when possible). Configure via `OIDC_ISSUER`, `OIDC_CLIENT_ID`, and optionally `OIDC_JWKS_URL` (internal URL for JWKS fetch, defaults to the issuer).
2. **`X-API-Key: <key>`** — static API key for headless clients (Prefect automation, scripts). Keys are hashed (SHA-256 lookup + argon2 verify) and stored in the `api_keys` table; the plaintext is never persisted. The bootstrap key is generated once on first startup and printed to the API log — set `BOOTSTRAP_API_KEY` in `.env` before first start to pin a known value.

For outbound email (snapshot), configure SMTP credentials in `.env` (`TODO_SMTP_USER`, `TODO_SMTP_PASS`). Gmail app passwords work out of the box with the default `smtp.gmail.com:587` settings.

## Setup

### Prerequisites

- Docker + Docker Compose
- Rust toolchain (`rustup`)
- Python 3.12 + `uv`
- `just` (install via [uvx](https://uvx.sh/): `uv tool install rust-just`)

### First run (local development)

```bash
# 1. copy and edit secrets
cp deploy/.env.example .env   # fill in at minimum BOOTSTRAP_API_KEY and SMTP credentials
# for OIDC sign-in also set OIDC_ISSUER, OIDC_CLIENT_ID, and (for prod) DOMAIN +
# KEYCLOAK_ADMIN / KEYCLOAK_ADMIN_PASSWORD — see deploy/docker-compose.yml

# 2. start Postgres + API (creates schema and seeds bootstrap user on first boot)
just stack-up

# 3. build and install the TUI
just build-tui

# 4. load env and launch (the binary is installed as `todo`)
source .env && todo
```

### Remote deployment

The production stack runs on a VPS behind Caddy (automatic TLS). Docker Compose merges `docker-compose.yml` with `docker-compose.prod.yml` — the prod override removes host-exposed ports, enables `restart: unless-stopped`, and adds the Caddy service.

**One-time server preparation** (run as root on the VPS):

```bash
bash deploy/setup-server.sh
```

This installs Docker, enables UFW with rules for SSH / 80 / 443 (including HTTP/3 QUIC), and creates `/srv/todo`.

**Deploy and start**:

```bash
# 1. push secrets to the server (do this before first deploy)
just remote-deploy-env SERVER=user@host

# 2. rsync code to the server
just remote-deploy SERVER=user@host

# 3. build images and start the full stack on the server
just remote-stack-up SERVER=user@host

# 4. verify the remote API
just remote-ping SERVER=user@host

# 5. open the Prefect UI via SSH tunnel (optional)
just remote-prefect-ui SERVER=user@host
```

`SERVER` can also be set in `.env` so you can omit it from every command.

## Common commands

```bash
# Docker / stack
just stack-up          # start Postgres + API
just stack-down        # stop everything
just stack-logs        # tail all stack logs
just db-up             # start only Postgres
just automation-up     # start the Prefect automation container
just automation-down   # stop the automation container
just automation-logs   # tail automation logs

# Development
just dev-api                # local uvicorn with SQLite (no Docker needed)
just dev-tui                # run TUI from source (loads .env automatically)

# Quality checks
just qc                     # rs-qc + py-qc
just rs-qc                  # rustfmt + clippy (Rust)
just py-qc                  # ruff format/lint + ty typecheck (Python)

# Tests
just test                   # test-rs + test-py
just test-rs                # cargo test --workspace
just test-py                # pytest

# Remote
just remote-deploy          # rsync code to server
just remote-deploy-env      # push .env to server
just remote-stack-up        # build + start full prod stack on server
just remote-stack-down      # stop the prod stack on server
just remote-ping            # health-check the remote API
just remote-ssh             # open SSH shell on server
just remote-automation-run  # trigger snapshot email manually on server
just remote-prefect-ui      # open Prefect UI via SSH tunnel
```
