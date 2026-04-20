# Tickets

# T0001: postgres backup and persistence 

Goal: Make sure we have versioned backups of our posgres data, index by 
date. Each morning, a new backup should be created and stored on disc in a dedicated folder (e.g. `/srv/todo/pg_backups`).

# T0002: Keycloak OIDC authentication [DONE]

Replace the API key by OIDC using keycloak. The tui should implement `:auth login` and redirect to the keycloak login page. The tui should show the currently authenticated user.

# T0003: python quality checks [DONE]

Similar as for the rust part, we use ruff and ty for quality checks. Implement a `just qc` command which runs the ruff linting and formatting with auto fixes. Then run `ty` as typecheker. 

# T0004: Revise README and solve TODO's [DONE]

Solve all TODO's in the README, revise it to match current repo structure. Also add a section on the remote deployment.

# T0005: Add the server setup to README and justfile

Make sure the setup-server.sh is idempotent and has strict firewall rules enabled. Revise the already existing script. Add a task to justfile and document in the README.

# T0006: Review current API structure.

Check if on slug deletion all corresponding notes and todos are deleted.

# T0007: enhance the daily status email to also contain statistics 

Add statistics like items (todos, notes) per slug and check if there are any orphaned todos or notes (i.e. not being attributed to any slug).

# T0008: Harden Keycloak deployment security

Several security shortcuts were taken to get OIDC working over plain HTTP. Fix before any production use:

1. `sslRequired: none` in `deploy/keycloak/realm-todo.json` — disables HTTPS enforcement for the todo realm. Should be `"external"` once TLS is in place.
2. Keycloak port 8080 exposed publicly — `docker-compose.yml` binds `8080:8080` so Keycloak admin UI and token endpoints are reachable from the internet. Should be restricted to `127.0.0.1` or routed through Caddy with TLS.
3. Keycloak running in `start-dev` mode — not suitable for production. Switch to `start` with a proper PostgreSQL backend once the above are resolved.
4. `KC_HOSTNAME_STRICT: false` and `KC_HTTP_ENABLED: true` added to allow external HTTP access. Remove once TLS is configured.

Resolution path: configure a domain + TLS via Caddy (proxy `keycloak.{$DOMAIN}` → `keycloak:8080`), restore `sslRequired: external`, bind Keycloak back to `127.0.0.1`, and switch to `start` mode.