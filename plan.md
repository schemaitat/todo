# Plan: todo app to centralized, scalable, multi-context architecture

## Status summary (updated 2026-04-19)

| Phase | Description | Status |
|---|---|---|
| 0 | Scaffolding (api/, deploy/, justfile) | ✓ Done |
| 1 | Postgres schema + SQLAlchemy models + bootstrap | ✓ Done |
| 2 | FastAPI skeleton (auth, settings, db, logging) | ✓ Done |
| 3 | Domain endpoints (contexts, todos, notes, events, snapshot) + tests | ✓ Done |
| 4 | Rust API client crate (`todo-api-client`) + wiremock tests | ✓ Done |
| 5 | TUI refactor to use API client | ✓ Done |
| 6 | Scheduled email snapshot: `/snapshot` endpoint + n8n workflow JSON | ✓ Done |
| 7 | Email ingest via n8n workflow | ✓ Done |
| 8 | Legacy JSON data migration script | ✓ Done |
| 9 | Auth evolution to OIDC/Keycloak | ○ Todo |
| 10 | Deployment polish (observability, backups, prod compose): basic `docker-compose.yml` done | ~ Partial |

**Notes on partial phases:**
- **Phase 6**: `api/app/routes/snapshot.py` implements `GET /snapshot?format=plain|html|json`; the n8n workflow JSON (`deploy/n8n/workflows/snapshot.json`) has not been created yet.
- **Phase 10**: `deploy/docker-compose.yml` covers Postgres + API + n8n profile; Caddy/TLS, Prometheus metrics, and `pg_dump` backup sidecar are not yet wired up.

---

## 1. Vision

Today the app is a single-binary Rust TUI (`crates/todo-tui`) with file-backed
state (`crates/todo-store` writing JSON to `dirs::data_local_dir()`) and a CLI
mailer (`crates/todo-mailer`) that reads the same file. Everything lives on one
laptop.

Target state:

- One source of truth: a Python FastAPI service backed by Postgres.
- Any number of clients (the TUI today, a web UI or mobile app later, n8n
  automations, cron jobs) talk to the API over HTTP.
- Data is partitioned into user-defined **contexts** (private, work, side
  projects, ...) so the same account can separate todos/notes without running
  multiple instances.
- Scheduled outbound (email snapshots) and inbound (emails becoming todos) are
  handled by n8n workflows hitting the API. The Rust `todo-mailer` crate
  becomes optional/legacy.
- Auth starts with a static API key header for simplicity; later swapped for
  OIDC (Keycloak) without breaking the API contract.

## 2. Target architecture

```
            +-----------------+        +--------------------+
            |   todo-tui      |        |   n8n workflows    |
            |   (Rust client) |        |  (cron + email)    |
            +--------+--------+        +----------+---------+
                     |                            |
                     | HTTPS + X-API-Key (later: Bearer JWT)
                     v                            v
                  +-------------------------------------+
                  |   FastAPI service (todo-api)        |
                  |   - auth dependency                 |
                  |   - REST endpoints                  |
                  |   - SQLAlchemy 2.x async + Alembic  |
                  +-----------------+-------------------+
                                    |
                                    v
                            +---------------+
                            |  PostgreSQL   |
                            +---------------+

Optional later: Keycloak sitting in front for OIDC.
```

Repo layout after the migration:

```
todo/
  crates/
    todo-store/        # keep as shared types crate (DTOs), drop file I/O
    todo-api-client/   # new: HTTP client used by todo-tui and todo-mailer
    todo-tui/          # now an API client instead of direct file access
    todo-mailer/       # reduced to a thin CLI that calls the API (or deleted)
  api/                 # new: FastAPI service
    app/
      main.py
      settings.py
      db.py
      auth.py
      models.py
      schemas.py
      routes/
        contexts.py
        todos.py
        notes.py
        events.py
        snapshot.py
      alembic/
      tests/
    pyproject.toml
    Dockerfile
  deploy/
    docker-compose.yml
    docker-compose.prod.yml
    n8n/workflows/     # JSON exports, version-controlled
    keycloak/          # realm JSON for later
  plan.md
```

## 3. Key decisions and rationale

| Area | Choice | Why |
|---|---|---|
| API framework | FastAPI | User-requested; great OpenAPI, Pydantic v2 types match the Rust DTOs well. |
| DB | PostgreSQL 16 | Standard, JSONB for event payloads, managed hosting options. |
| ORM | SQLAlchemy 2.x async + Alembic | Stable, async-native, rich migration tooling. SQLModel is an alternative but Alembic support is thinner. |
| Validation | Pydantic v2 | Built into FastAPI. |
| Auth v1 | `X-API-Key` header, compared against hashed key in DB | Trivial to ship; multi-user-ready when we add the user table. |
| Auth v2 | OIDC via Keycloak, RS256 JWTs | Stated future goal; JWT swap is a single dependency change if v1 is structured right. |
| Rust HTTP | `reqwest` blocking + `serde` | TUI is single-threaded/sync; blocking client avoids dragging tokio into the UI loop. |
| Cron | n8n schedule trigger | User asked for n8n; keeps scheduling out of the API and makes workflows editable without redeploys. |
| Email ingest | n8n Gmail trigger to `/todos` | Same tool, one place. |
| Deployment | `docker-compose` on a small VPS, Caddy/Traefik for TLS | Minimal ops, enough for single-tenant for a long time. |
| Config | `pydantic-settings`, env-driven; `~/.config/todo-tui/config.toml` for client | Twelve-factor on the server side; file on the client since it's interactive. |

Non-goals (explicitly out of scope for this plan): real-time sync between
clients, offline write queues, attachments/binary blobs, mobile app, CRDT.

## 4. Phase 0 - prerequisites and repo layout ✓ DONE

Goal: land scaffolding without changing behaviour.

Steps:

1. Add an `api/` directory with `pyproject.toml` (Poetry or uv; uv is faster
   and newer, pick one). Dependencies: `fastapi`, `uvicorn[standard]`,
   `sqlalchemy[asyncio]`, `asyncpg`, `alembic`, `pydantic-settings`,
   `httpx` (for tests), `pytest`, `pytest-asyncio`, `ruff`, `mypy`.
2. Add `deploy/docker-compose.yml` with two services to start: `postgres:16`
   and `api` (build from `api/Dockerfile`). Map a volume for pg data.
3. Extend the `justfile`:
   - `just api-dev` to run uvicorn with autoreload.
   - `just db-up` / `just db-down` to manage the compose stack.
   - `just migrate` to run Alembic.
4. Wire `api/Dockerfile` (python:3.12-slim base, non-root user, copy app,
   `uvicorn app.main:app --host 0.0.0.0 --port 8000`).
5. Keep the existing Rust code untouched this phase. Goal is reversible ground
   state.

Deliverables: `docker compose up` brings up Postgres and an empty FastAPI
returning `{"status":"ok"}` on `/health`.

## 5. Phase 1 - data model and the context concept ✓ DONE

Design the Postgres schema up front because migrations later are painful.

Entities:

- `users` - placeholder today, required for OIDC later. Columns: `id` (uuid
  pk), `email` (unique), `display_name`, `external_sub` (nullable, used once
  OIDC lands), `created_at`, `disabled_at` (soft-delete).
- `api_keys` - `id`, `user_id` fk, `key_hash` (argon2 or bcrypt), `label`,
  `created_at`, `last_used_at`, `revoked_at`. Never store the plaintext.
- `contexts` - `id`, `user_id` fk, `slug` (unique per user, e.g. `work`),
  `name`, `color` (hex), `position` (int, for ordering), `archived_at`.
  Seed one `inbox` context per user at creation.
- `todos` - `id`, `context_id` fk, `title`, `done` (bool), `created_at`,
  `updated_at`, `deleted_at`. Indexes: `(context_id, deleted_at, done)`.
- `notes` - `id`, `context_id` fk, `title`, `body` (text), `created_at`,
  `updated_at`, `deleted_at`. Index on `(context_id, deleted_at)`.
- `events` - `id` (bigserial), `user_id`, `context_id`, `entity_type`
  (`todo`/`note`), `entity_id`, `kind` (enum matching `EventKind` in
  `crates/todo-store/src/lib.rs:71`), `payload` (jsonb), `ts` (timestamptz,
  index). Append-only; drives the history view.

Invariants:

- Every todo/note belongs to exactly one context; a context belongs to exactly
  one user. No global todos.
- Deletion is soft (`deleted_at`) to preserve the current TUI behaviour and
  keep the event log sensible.
- `slug` is the stable identifier used in URLs (`/contexts/work/todos`);
  renaming a context changes `name` but not `slug`. Allow slug edit as a
  separate admin operation.

Steps:

1. Create the Alembic baseline migration with all tables above.
2. Write SQLAlchemy models in `api/app/models.py` matching the schema.
3. Add a startup bootstrap that creates a default user + api key + `inbox`
   context if the DB is empty, printing the API key once. This replaces the
   "single laptop, no auth" setup until we build a real signup flow.

## 6. Phase 2 - FastAPI backend skeleton ✓ DONE

Goal: all plumbing in place, no domain endpoints yet.

Steps:

1. `app/settings.py` - `Settings(BaseSettings)` reads `DATABASE_URL`,
   `BOOTSTRAP_USER_EMAIL`, `BOOTSTRAP_API_KEY` (optional; generated if unset),
   `ALLOWED_ORIGINS`.
2. `app/db.py` - async engine + session dependency (`AsyncSession`).
3. `app/auth.py` - `get_current_user` dependency: reads `X-API-Key`, looks up
   hash, returns the `User` row or `HTTPException(401)`. Update
   `api_keys.last_used_at` on success. Structure the dependency so that
   swapping to JWT in Phase 9 is a one-file change.
4. `app/main.py` - mount routers, register exception handlers, attach CORS,
   expose `/health` and `/version`.
5. Structured logging via `structlog` or stdlib `logging` with JSON formatter;
   include request id, user id (when auth'd), latency.
6. Tests: `api/tests/test_auth.py` covering missing key, wrong key, revoked
   key, happy path. Use `httpx.AsyncClient` with an in-process ASGI transport.

Deliverables: `GET /health` unauthenticated, `GET /me` (returns current user)
authenticated - end-to-end proof the auth dependency works.

## 7. Phase 3 - domain endpoints ✓ DONE

Design REST resources so the Rust client is mechanical.

Routes (all require auth, all scoped to current user):

- `GET /contexts` -> list including archived flag.
- `POST /contexts` -> `{slug, name, color?}`; returns 409 on duplicate slug.
- `PATCH /contexts/{slug}` -> rename, recolor, reorder.
- `DELETE /contexts/{slug}` -> sets `archived_at`; cannot delete `inbox`.
- `GET /contexts/{slug}/todos?include_done=true&include_deleted=false` ->
  ordered by `created_at`.
- `POST /contexts/{slug}/todos` -> `{title}`.
- `PATCH /todos/{id}` -> `{title?, done?}`; returns 404 if not in a context
  owned by the caller.
- `DELETE /todos/{id}` -> soft delete.
- `GET /contexts/{slug}/notes` and `POST /contexts/{slug}/notes` -> same
  shape as todos.
- `PATCH /notes/{id}` -> `{title?, body?}`; use `If-Match` with `updated_at`
  ISO timestamp for optimistic concurrency, return 412 on mismatch.
- `DELETE /notes/{id}` -> soft delete.
- `GET /events?context=slug&limit=200&before=<ts>` -> paginated newest-first.
- `GET /snapshot?context=slug&format=plain|html` -> returns the current email
  snapshot. Reuses the logic currently in `crates/todo-mailer/src/email.rs:42`
  (`format_body`) and `:98` (`format_html`), rewritten in Python. n8n/cron use
  this endpoint.

Cross-cutting rules:

- All mutations append to `events` in the same transaction that mutated the
  row. Do this in a service-layer helper so routes stay thin.
- Response schemas live in `app/schemas.py` (Pydantic v2). Keep names aligned
  with the Rust types in `crates/todo-store/src/lib.rs:10` (`Todo`, `Note`,
  `EventKind`) so the client crate can deserialize with minimal glue.

Testing:

- Per-route pytest covering happy path, 404 on cross-user access, 409 on slug
  collisions, 412 on stale note edits, and a consistency test asserting that
  every mutation produces exactly one event row.

## 8. Phase 4 - Rust API client crate ✓ DONE

Create `crates/todo-api-client`:

- Library only. Depends on `reqwest` (features `blocking`, `json`,
  `rustls-tls`), `serde`, `thiserror`, `url`, and `todo-store` for the shared
  DTOs (once step below is done).
- `Config` struct: `base_url`, `api_key`, `context_slug` (default active
  context). Loaded from env (`TODO_API_URL`, `TODO_API_KEY`,
  `TODO_CONTEXT`) or from `~/.config/todo-tui/config.toml` via the `config`
  crate. Env wins.
- `Client` struct wrapping `reqwest::blocking::Client`, prebuilt with the API
  key header and a sane timeout (5s).
- Methods mirroring the TUI's needs:
  - `list_contexts`, `create_context`, `set_active_context`
  - `list_todos(ctx)`, `create_todo(ctx, title)`, `toggle_todo(id)`,
    `rename_todo(id, title)`, `delete_todo(id)`
  - `list_notes(ctx)`, `create_note(ctx, title)`, `rename_note(id, title)`,
    `update_note_body(id, body, if_match)`, `delete_note(id)`
  - `list_events(ctx, limit)`
- Error type: `ApiError { Http(StatusCode, body), Network(reqwest::Error),
  Serde(...), Conflict, NotFound, Unauthorized }`. The TUI uses these to
  render useful status messages.

Steps:

1. Refactor `crates/todo-store`: keep `Todo`, `Note`, `Event`, `EventKind`,
   add `Context` and `EventKind::{ContextCreated, ContextRenamed,
   ContextArchived}`. Delete `load`, `save`, `append_event`, `data_path`,
   `events_path`, `load_events` (defined in
   `crates/todo-store/src/lib.rs:100-151`) - or move them behind a
   `local-fs` feature for the one-shot migration script.
2. Build the client crate, pointing its tests at a mocked HTTP server
   (`wiremock` crate) so it can run without the real backend.
3. Add integration tests in `api/tests/` that run a real Postgres via
   `testcontainers-python` and exercise the full stack. Mirror the Rust
   client's method signatures to catch drift.

## 9. Phase 5 - TUI refactor ✓ DONE

This is the biggest client-side change. The TUI currently mutates
`app.store` in place and calls `storage::save` after every change (see
`crates/todo-tui/src/app.rs:612`). That becomes HTTP calls.

Steps:

1. Replace `App.store: Store` with:
   - `App.client: todo_api_client::Client`
   - `App.active_context: Context`
   - `App.todos: Vec<Todo>` (cached view; refreshed after each mutation or on
     demand)
   - `App.notes: Vec<Note>`
   - `App.contexts: Vec<Context>`
2. All mutation handlers (`commit_input`, `delete_current`, `toggle_done`,
   `finish_note_edit` - `crates/todo-tui/src/app.rs:405`, `:483`, `:511`,
   `:313`) call `client.*` and then update the cached `Vec`s from the
   response. On error, set `status` to the error message and do not mutate
   the cache.
3. Introduce a "context switcher" UX:
   - New command `:ctx <slug>` - validates against `App.contexts`, reloads
     todos/notes.
   - New command `:ctx new <slug> [name]` to create one.
   - Status bar shows `[<active context>]` at the left.
   - Optional picker mode (`Mode::ContextPicker`) bound to `:ctx` with no
     arg; arrow keys + enter to pick. Follow the existing mode pattern at
     `crates/todo-tui/src/app.rs:17` (`Mode` enum).
4. Offline behavior: if the client returns `ApiError::Network`, show a red
   status "offline - read-only"; block further writes until a successful
   `client.list_contexts()` ping. Keep the last fetched `Vec`s so the user
   can still read.
5. Filter/selection invariants continue to hold since `visible_todo_indices`
   / `visible_note_indices` (`crates/todo-tui/src/app.rs:566`, `:578`)
   operate on the cached `Vec`; verify `snap_selection` still works after
   every refresh.
6. Event log view (`Mode::History` in `crates/todo-tui/src/app.rs:624`) now
   calls `client.list_events` instead of `storage::load_events`.
7. Delete `crates/todo-store`'s file I/O usage from the TUI's dependencies.

Acceptance: with `TODO_API_URL`, `TODO_API_KEY`, `TODO_CONTEXT` set and the
API running, the TUI behaves exactly as today from the user's point of view,
with the addition of `:ctx` commands.

## 10. Phase 6 - scheduled email snapshots via n8n ✓ DONE

Today `todo-mailer` (`crates/todo-mailer/src/email.rs`) reads the local JSON
and sends one email. Two options:

- **Option A (recommended):** delete or shrink the Rust mailer; let n8n
  handle scheduling and sending. An n8n workflow runs on a cron schedule,
  hits `GET /snapshot?context=work&format=html`, and sends via the Gmail or
  SMTP node using the same credentials currently in `.env`. Owner edits the
  schedule in n8n's UI.
- **Option B:** keep `todo-mailer` as a systemd timer; have it call the API
  `/snapshot` endpoint and send via SMTP as it does today. More moving
  parts, but no extra service.

Prefer A because it unifies scheduled jobs and email ingest (next phase) in
one place and removes a whole Rust crate from the maintenance surface.

Steps for Option A:

1. Move `format_body` / `format_html` logic from
   `crates/todo-mailer/src/email.rs` into the API as the `/snapshot`
   endpoint. Port the tests.
2. Add n8n to `deploy/docker-compose.yml` with a volume for workflow state.
3. Build a workflow: Schedule Trigger -> HTTP Request (call `/snapshot`
   with API key in header) -> Gmail node. Export the workflow JSON into
   `deploy/n8n/workflows/snapshot.json`.
4. Delete `crates/todo-mailer` once the n8n workflow is verified in prod.
   Remove the `email *args` recipe from the `justfile` (lives at
   `justfile:17`).

## 11. Phase 7 - email ingest via n8n ✓ DONE

Goal: emailing `inbox-work@...` (or forwarding to a watched Gmail label)
creates a todo or note in the right context.

Steps:

1. Create a dedicated Gmail account or label for ingest (e.g.
   `todo+ingest@...`).
2. n8n workflow: Gmail Trigger (poll label every minute) -> Function node
   that parses the message:
   - Default context = `inbox`.
   - Override via subject prefix `[work]` or body tag `ctx:work`.
   - Subject -> `title`.
   - Body (plain) -> nothing (todo) or note body if subject starts with
     `note:`.
3. HTTP Request node -> `POST /contexts/{ctx}/todos` or
   `POST /contexts/{ctx}/notes` with the API key.
4. Gmail node: archive the message on success; label `todo-ingest-failed`
   on error for manual review.
5. Export workflow JSON to `deploy/n8n/workflows/email-ingest.json`.

Document the parsing rules in the repo README so the user remembers them
next year.

## 12. Phase 8 - migrate existing JSON data ✓ DONE

The user has live data in
`dirs::data_local_dir()/todo-tui/data.json` and
`events.jsonl`. One-shot script to avoid losing it.

Steps:

1. Write `crates/todo-migrate` (binary) or a Python script in
   `api/scripts/import_legacy.py`. Python is easier because it can import the
   SQLAlchemy models directly and use a transaction.
2. Read the JSON file, assign every todo/note to the bootstrap user's
   `inbox` context, preserve `id`, `created_at`, `deleted_at`, `updated_at`,
   `done`.
3. Replay `events.jsonl` into the `events` table with preserved timestamps
   and a synthetic `context_id = inbox`.
4. Run once against the prod DB, verify counts match, archive the local JSON
   to `data.json.imported-YYYY-MM-DD`.

## 13. Phase 9 - auth evolution to OIDC/Keycloak ○ TODO

Do not block the whole plan on this. Ship v1 with API keys, swap later.

Steps (when ready):

1. Add `keycloak` service to `deploy/docker-compose.yml`, bootstrapping a
   realm from a versioned JSON in `deploy/keycloak/realm.json`. Clients:
   `todo-tui` (public, device code flow) and `todo-api` (confidential, used
   by the API to validate tokens via JWKS).
2. Add `python-jose[cryptography]` and rewrite `app/auth.py` to accept a
   Bearer JWT, validate against Keycloak's JWKS, and resolve the user by
   `external_sub`. On first sight of an `external_sub`, upsert a `users`
   row so the rest of the schema is unchanged.
3. Keep `X-API-Key` accepted in parallel for a deprecation window - machine
   clients (n8n) keep using keys, humans move to OIDC. Log usage of each to
   measure drift.
4. In the TUI client, add a device-code flow: `todo-tui login` opens the
   verification URL, polls for the token, stores it in the OS keyring
   (`keyring` crate), refreshes transparently.
5. Eventually delete the `api_keys` table for human users, keep it for
   service accounts (n8n, cron).

## 14. Phase 10 - deployment, observability, backups ~ PARTIAL

- Prod compose: Postgres (data volume + nightly `pg_dump` to object storage
  via a sidecar), API behind Caddy with automatic TLS, n8n behind basic auth
  on a subpath, Keycloak on a separate subdomain once live.
- `/metrics` Prometheus endpoint via `prometheus-fastapi-instrumentator`.
  Scrape from a Grafana Cloud free account or a local Prometheus.
- Structured logs to stdout, scraped by Loki/Grafana or simply
  `docker logs`.
- DB backups: `pg_dumpall` nightly, retain 7 daily + 4 weekly. Test a
  restore quarterly - schedule a calendar reminder.
- Secrets: `.env` file on the host, mode 600, never committed. Move to a
  proper secret manager only if/when the deployment outgrows a single host.

## 15. Risks and open questions

- **TUI latency on bad networks.** Every keypress that mutates state now
  does an HTTP round trip. At 150ms RTT the UX is still fine for CRUD; at
  500ms+ it feels sluggish. If this shows up, introduce optimistic updates
  in the cache and reconcile asynchronously. Not worth building up front.
- **Conflict resolution on notes.** Optimistic concurrency via `If-Match`
  covers the honest case. Cross-device simultaneous edits are rare for a
  personal tool; we accept the occasional 412 and surface it as "note
  changed elsewhere, reopen".
- **Event log growth.** `events` grows unbounded. Partition by month once it
  crosses ~1M rows. Not a day-one problem.
- **API key rotation.** Build the endpoint `POST /me/api-keys` with
  `revoke/label` support now; avoid painful rollovers later.
- **Multi-user?** The schema supports it; the bootstrap flow assumes a
  single user. Defer any real signup UI until OIDC, where Keycloak handles
  user creation.
- **Do we keep `todo-store` as a crate?** Yes as a shared-types crate;
  probably rename to `todo-types` once the file I/O is gone, since "store"
  will be misleading.

## 16. Rough sequencing and effort

A realistic order, parallelizable where noted. Times are back-of-envelope
for a single developer.

1. Phase 0 scaffolding - 0.5 day.
2. Phase 1 schema + Phase 2 skeleton + Phase 3 endpoints - 2-3 days total.
   Do these as one sprint since they share tests.
3. Phase 4 client crate + wiremock tests - 1 day. Can start once Phase 3
   shapes are stable.
4. Phase 5 TUI refactor - 2 days. The biggest risk of regressions; leave a
   day for polishing the context switcher UX.
5. Phase 6 snapshot endpoint + n8n workflow - 0.5 day.
6. Phase 8 data migration - 0.5 day; run it the same day Phase 5 lands.
7. Phase 7 email ingest - 0.5 day.
8. Phase 10 deployment polish - 0.5 day ongoing.
9. Phase 9 OIDC - 2 days, scheduled separately once the rest is stable.

First shippable milestone: Phases 0-6 + 8, roughly one week of focused
work. That already delivers "centralized API, TUI uses it, contexts,
scheduled email snapshots". Everything after that is incremental hardening.
