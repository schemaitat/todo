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

# --- One-shots -------------------------------------------------------------

import-legacy *args:
    cd api && .venv/bin/python scripts/import_legacy.py {{args}}
