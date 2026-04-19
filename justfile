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

api-dev:
    cd api && DATABASE_URL="${DATABASE_URL:-sqlite+aiosqlite:///./todo.db}" .venv/bin/uvicorn app.main:app --host 0.0.0.0 --port 8000 --reload

test-api:
    cd api && .venv/bin/pytest -q

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

automation-up:
    just _compose up -d automation

automation-logs:
    just _compose logs -f automation


# --- Remote deploy -----------------------------------------------------------
# Usage: just deploy SERVER=user@host
#        just stack-up-prod SERVER=user@host
#        just ping-remote

prefect-ui:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a && source .env && set +a
    host="root@${SERVER##*@}"
    echo "Prefect UI → http://localhost:4200  (Ctrl-C to close tunnel)"
    open http://localhost:4200
    ssh -N -L 4200:localhost:4200 "$host"

automation-run:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a && source .env && set +a
    host="root@${SERVER##*@}"
    ssh "$host" "docker exec -e PREFECT_API_URL=http://localhost:4200/api deploy-automation-1 prefect deployment run snapshot-email/snapshot-email"

_server:
    #!/usr/bin/env bash
    set -a && source .env && set +a
    echo "root@${SERVER##*@}"  # strip any existing user prefix, force root

# deploy code
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
        --exclude='automation/.venv' \
        --exclude='api/todo.db' \
        --exclude='.env' \
        . "$host":/srv/todo
    echo "Done."

# deploy .env separately (don't want to risk overwriting any existing secrets on the server)
deploy-env:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a && source .env && set +a
    host="root@${SERVER##*@}"
    scp .env "$host":/srv/todo/.env
    echo "Deployed .env to $host"

# deploy and start stack on prod server
stack-up-prod:
    #!/usr/bin/env bash
    set -euo pipefail
    set -a && source .env && set +a
    host="root@${SERVER##*@}"
    ssh "$host" "cd /srv/todo/deploy && docker compose -f docker-compose.yml -f docker-compose.prod.yml --env-file .env up -d --build postgres api caddy automation"

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