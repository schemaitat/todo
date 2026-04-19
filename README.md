# todo

Personal todo/notes system with a terminal UI, centralized API, and email automation.

## Components

```
todo-tui  ──HTTP──►  FastAPI + Postgres
                          ▲
n8n workflows  ───────────┘
  • snapshot email (cron)
  • email ingest (Gmail trigger)
```

| Component | Location | Role |
|---|---|---|
| `todo-tui` | `crates/todo-tui` | Ratatui terminal UI |
| `todo-api-client` | `crates/todo-api-client` | Blocking HTTP client used by the TUI |
| `todo-store` | `crates/todo-store` | Shared DTO types (Todo, Note, Event, Context) |
| FastAPI service | `api/` | REST backend, Postgres via SQLAlchemy 2.x async |
| n8n | `deploy/n8n/` | Scheduled snapshot emails + Gmail ingest |
| Postgres | Docker volume `pgdata` | Single source of truth |

## How they connect

- The TUI reads config from `~/.config/todo-tui/config.toml` or env vars and talks to the API over HTTP.
- All mutations go through the API; the TUI caches the last fetched lists locally in memory.
- n8n runs two workflows: one calls `GET /snapshot` on a cron and emails the result; the other watches a Gmail label and calls `POST /contexts/{ctx}/todos` or `/notes`.
- Events (create/rename/toggle/delete) are appended to an `events` table in the same transaction as the mutation — the TUI's `:history` view reads them via `GET /events`.

## Authentication

API requests are authenticated with a static `X-API-Key` header. The key is hashed (SHA-256 lookup + argon2 verify) and stored in the `api_keys` table — the plaintext is never persisted.

The bootstrap key is generated once on first startup and printed to the API log. Set `BOOTSTRAP_API_KEY` in `.env` before first start to pin a known value.

n8n reads the key from the `TODO_API_KEY` environment variable passed in via `docker-compose.yml`.

For Gmail access, n8n uses OAuth2 — see [`deploy/n8n/GMAIL_OAUTH.md`](deploy/n8n/GMAIL_OAUTH.md).

## Setup

### Prerequisites

- Docker + Docker Compose
- Rust toolchain (`rustup`)
- Python 3.12 + `uv`

### First run

```bash
# 1. copy and edit secrets
cp .env.example .env   # or edit .env directly

# 2. start Postgres + API  (creates schema and seeds bootstrap user on first boot)
just stack-up

# 3. install Python deps (needed for local dev / migration script only)
just api-sync

# 4. build and install the TUI
just build

# 5. load env and launch
source .env && todo-tui
```

### Migrating legacy data

If you have an existing `~/Library/Application Support/todo-tui/data.json`:

```bash
just import-legacy          # dry run first: just import-legacy --dry-run
```

### n8n email workflows

```bash
just n8n-up        # start n8n at http://localhost:5678
just n8n-import    # import snapshot + ingest workflow JSONs
```

Then set up a Gmail OAuth2 credential in the n8n UI — see [`deploy/n8n/GMAIL_OAUTH.md`](deploy/n8n/GMAIL_OAUTH.md) — and toggle both workflows active.

### Email ingest rules

Emails moved to the Gmail label **`todo-ingest`** become todos or notes:

| Subject | Result |
|---|---|
| `Buy milk` | todo in `inbox` |
| `[work] Buy milk` | todo in context `work` |
| `note: Meeting recap` | note in `inbox` |
| `[work] note: Stand-up` | note in context `work` |

Add `ctx:slug` anywhere in the body to override the context.

## Common commands

```bash
just stack-up      # start Postgres + API
just stack-down    # stop everything
just test-rust     # cargo test --workspace
just test-api      # pytest
just check         # rustfmt + clippy
just api-dev       # local uvicorn with SQLite (no Docker needed)
```
