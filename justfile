default:
    @just --list

# --- Rust ------------------------------------------------------------------

fmt:
    cargo fmt --all

lint:
    cargo clippy --all-targets --all-features --fix --allow-dirty

check: fmt lint

build:
    cargo build --release
    mkdir -p ~/.local/bin
    install -m 755 target/release/todo-tui ~/.local/bin/todo

tui:
    #!/usr/bin/env bash
    set -a && source .env && set +a
    cargo run -p todo-tui

test-rust:
    cargo test --workspace

# --- Python API ------------------------------------------------------------

api-sync:
    cd api && uv venv --allow-existing --python 3.12 && uv pip install -e '.[dev]'

api-dev:
    cd api && DATABASE_URL="${DATABASE_URL:-sqlite+aiosqlite:///./todo.db}" .venv/bin/uvicorn app.main:app --host 0.0.0.0 --port 8000 --reload

migrate *args:
    cd api && .venv/bin/alembic {{args}}

api-upgrade:
    cd api && .venv/bin/alembic upgrade head

test-api:
    cd api && .venv/bin/pytest -q

lint-api:
    cd api && .venv/bin/ruff check app tests scripts && .venv/bin/ruff format --check app tests scripts

fmt-api:
    cd api && .venv/bin/ruff format app tests scripts && .venv/bin/ruff check --fix app tests scripts

# --- Deploy (compose) ------------------------------------------------------

_compose *args:
    cd deploy && docker compose --env-file ../.env {{args}}

db-up:
    just _compose up -d postgres

db-down:
    just _compose down

stack-up:
    just _compose up -d postgres api

stack-down:
    just _compose down

n8n-up:
    just _compose --profile n8n up -d n8n

n8n-open:
    #!/usr/bin/env bash
    set -a && source .env && set +a
    open "http://${SERVER##*@}:5678"

n8n-import:
    just _compose --profile n8n exec n8n n8n import:workflow --input=/workflows/snapshot.json
    just _compose --profile n8n exec n8n n8n import:workflow --input=/workflows/email-ingest.json

n8n-import-remote:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a && source .env && set +a
    host="root@${SERVER##*@}"
    ssh "$host" "cd /srv/todo/deploy && docker compose --env-file .env --profile n8n exec n8n n8n import:workflow --input=/workflows/snapshot.json"
    ssh "$host" "cd /srv/todo/deploy && docker compose --env-file .env --profile n8n exec n8n n8n import:workflow --input=/workflows/email-ingest.json"

# --- Remote deploy -----------------------------------------------------------
# Usage: just deploy SERVER=user@host
#        just stack-up-prod SERVER=user@host
#        just ping-remote

_server:
    #!/usr/bin/env bash
    set -a && source .env && set +a
    echo "root@${SERVER##*@}"  # strip any existing user prefix, force root

deploy:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a && source .env && set +a
    host="root@${SERVER##*@}"
    echo "Syncing to $host:/srv/todo ..."
    rsync -az --delete \
        --exclude='.git' \
        --exclude='target/' \
        --exclude='api/.venv' \
        --exclude='api/__pycache__' \
        --exclude='api/todo.db' \
        --exclude='.env' \
        . "$host":/srv/todo
    echo "Done."

stack-up-prod:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a && source .env && set +a
    host="root@${SERVER##*@}"
    ssh "$host" "cd /srv/todo/deploy && docker compose -f docker-compose.yml -f docker-compose.prod.yml --env-file .env up -d --build postgres api caddy"

n8n-up-prod:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a && source .env && set +a
    host="root@${SERVER##*@}"
    ssh "$host" "cd /srv/todo/deploy && docker compose -f docker-compose.yml -f docker-compose.prod.yml --env-file .env --profile n8n up -d n8n"

ssh-remote:
    #!/usr/bin/env bash
    set -a && source .env && set +a
    ssh "root@${SERVER##*@}"

ping-remote:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a && source .env && set +a
    echo "Pinging $TODO_API_URL/health ..."
    curl -sf "$TODO_API_URL/health" | python3 -m json.tool
    echo ""
    echo "Listing contexts ..."
    curl -sf -H "X-API-Key: $TODO_API_KEY" "$TODO_API_URL/contexts" | python3 -m json.tool

# --- One-shots -------------------------------------------------------------

import-legacy *args:
    cd api && .venv/bin/python scripts/import_legacy.py {{args}}
