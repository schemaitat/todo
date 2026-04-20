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

# T0008: Harden Keycloak deployment security [IN REVIEW]

Several security shortcuts were taken to get OIDC working over plain HTTP. Fix before any production use:

1. `sslRequired: none` in `deploy/keycloak/realm-todo.json` — disables HTTPS enforcement for the todo realm. Should be `"external"` once TLS is in place.
2. Keycloak port 8080 exposed publicly — `docker-compose.yml` binds `8080:8080` so Keycloak admin UI and token endpoints are reachable from the internet. Should be restricted to `127.0.0.1` or routed through Caddy with TLS.
3. Keycloak running in `start-dev` mode — not suitable for production. Switch to `start` with a proper PostgreSQL backend once the above are resolved.
4. `KC_HOSTNAME_STRICT: false` and `KC_HTTP_ENABLED: true` added to allow external HTTP access. Remove once TLS is configured.

Resolution path: configure a domain + TLS via Caddy (proxy `keycloak.{$DOMAIN}` → `keycloak:8080`), restore `sslRequired: external`, bind Keycloak back to `127.0.0.1`, and switch to `start` mode.

# T0009: Host security hardening (firewall, SSL, access)

Harden the production host before going live. Cover the following areas:

1. **Firewall (ufw)** — ensure only ports 22 (SSH), 80 (HTTP), and 443 (HTTPS) are open to the internet. All other ports (5432 Postgres, 8080 Keycloak, 8000 API) must be blocked externally and only reachable from localhost or the internal Docker network.
2. **SSL / TLS** — configure Caddy to obtain and auto-renew a Let's Encrypt certificate for the domain. Ensure HTTP traffic is redirected to HTTPS. Verify the Keycloak reverse-proxy route (`keycloak.{$DOMAIN}`) is also TLS-terminated by Caddy.
3. **SSH hardening** — disable password authentication, enforce key-based login only. Consider changing the default SSH port or enabling fail2ban to limit brute-force attempts.
4. **Secrets management** — confirm that `.env` files on the host are not world-readable (`chmod 600`). Rotate any credentials that were committed or shared in plain text during development.
5. **Docker socket exposure** — ensure the Docker socket is not exposed to any container that does not strictly need it.
6. **Caddy security headers** — add recommended HTTP security headers (HSTS, X-Frame-Options, X-Content-Type-Options, Referrer-Policy) to the Caddy config.
7. **Automatic security updates** — install and enable `unattended-upgrades` in `deploy/setup-server.sh` so OS security patches apply without manual intervention. Configure a safe auto-reboot window for kernel updates.
8. **Docker daemon log rotation** — configure `/etc/docker/daemon.json` with `log-driver: json-file` and `log-opts: {max-size: "10m", max-file: "3"}` so container logs cannot fill the disk.
9. **fail2ban coverage beyond SSH** — extend item 3 with jails for Caddy (`auth.{$DOMAIN}` login brute-force, 4xx spikes on `api.{$DOMAIN}`) in addition to the SSH jail.
10. **Docker install hardening** — replace `curl -fsSL https://get.docker.com | sh` in `deploy/setup-server.sh:13` with the official Debian/Ubuntu APT repository using GPG key verification. Piping an unverified script to shell is an unnecessary supply-chain risk.
11. **Backup encryption** — coordinate with T0001: backups written to `/srv/todo/pg_backups` must be encrypted at rest (e.g. `age` or `gpg`) with the key stored outside the backup directory.

Acceptance criteria: a fresh `nmap` scan against the host shows only ports 22, 80, and 443 open; the site passes an SSL Labs grade of A or higher; SSH login with a password is rejected; `unattended-upgrades --dry-run` succeeds; `docker info` shows the configured log driver and rotation options.

# T0010: Deployment security hardening (containers and compose)

Harden the Docker-based deployment against container escape, credential exposure, and overly permissive defaults. Covers issues visible in `docker-compose.yml` and `docker-compose.prod.yml`.

1. **Hardcoded credentials** — `docker-compose.yml` contains plain-text defaults (`POSTGRES_PASSWORD: todo`, `KC_DB_PASSWORD: todo`, `KC_BOOTSTRAP_ADMIN_PASSWORD: admin`). Replace all defaults with required env-var references (no fallback) so the stack refuses to start without explicit secrets.
2. **API port binding** — `api` binds `8000:8000` without a `127.0.0.1` prefix, exposing the API directly on all interfaces. Restrict to `127.0.0.1:8000:8000` in base compose; Caddy is the only intended external entry point.
3. **`ALLOWED_ORIGINS: "*"`** — the base compose sets a wildcard CORS origin. Tighten to the actual domain even for local dev, or at minimum ensure the prod override always wins.
4. **Keycloak `start-dev` mode** — already tracked in T0008 but also a deployment concern: `start-dev` disables many security checks and is not suitable for production. Switch to `start` with explicit config.
5. **No resource limits** — containers have no `mem_limit` / `cpus` constraints, making the host vulnerable to a runaway container exhausting resources. Add sensible limits to each service.
6. **Run containers as non-root** — the `automation` image has no `USER` directive and runs as root (`api/Dockerfile:23` already sets `USER app`). Add a `USER` directive to `automation/Dockerfile` and verify Prefect works unprivileged.
7. **Read-only filesystems** — where possible, mount the container filesystem as read-only (`read_only: true`) and use explicit `tmpfs` mounts for writable paths (e.g. `/tmp`).
8. **No `security_opt` / capability dropping** — add `security_opt: [no-new-privileges:true]` and drop unnecessary Linux capabilities (`cap_drop: [ALL]`) for each service.
9. **Prefect admin UI** — `automation` exposes the Prefect server on `0.0.0.0` inside the container (`PREFECT_SERVER_API_HOST: "0.0.0.0"`). Restrict to `127.0.0.1` and verify the Caddy config does not accidentally expose it.
10. **API healthcheck missing** — `api` in `docker-compose.yml` has no `healthcheck` while `postgres` and `keycloak` do. Caddy will keep proxying to a crashed API. Add an HTTP probe against `/health` so `depends_on: condition: service_healthy` works end-to-end.
11. **Per-service log driver limits** — no service configures `logging.driver` / `options`. Set `json-file` with `max-size: "10m"` and `max-file: "3"` on every service so a runaway container cannot fill the host disk.
12. **Image digest pinning** — `postgres:16`, `caddy:2-alpine`, `quay.io/keycloak/keycloak:26.0`, and `ghcr.io/astral-sh/uv:latest` (in `automation/Dockerfile`) all use mutable tags. Pin to `@sha256:...` digests and document how to refresh them.
13. **Postgres role separation** — `docker-compose.yml` reuses the `todo` role for both the app DB and the Keycloak DB (`KC_DB_USERNAME: todo`). Create a dedicated `keycloak` role with privileges on the `keycloak` database only, via `deploy/postgres/init.sql`.
14. **TLS between API and Postgres** — even on the internal Docker network, enable TLS in Postgres and add `sslmode=require` (or `verify-full` with a CA) to `DATABASE_URL` as defense in depth.

Acceptance criteria: `docker compose config` shows no plain-text credential defaults; all custom-image containers run as non-root; port bindings for internal services are `127.0.0.1`-scoped or absent; `docker inspect` confirms `no-new-privileges` is set; every service defines a `healthcheck` and log-driver limits; all images are pinned by digest.

# T0011: API application-layer security hardening

Harden the FastAPI surface against misuse, unauthorized access, and information leakage. These items are separate from transport/container concerns in T0009/T0010.

1. **CORS misconfiguration** — `api/app/main.py:37` combines `allow_origins=["*"]` (from `Settings.cors_origins` at `api/app/settings.py:31`) with `allow_credentials=True`, `allow_methods=["*"]`, and `allow_headers=["*"]`. Browsers silently reject the `*` + credentials combo; remove the wildcard fallback and restrict methods/headers to what the API actually uses.
2. **JWT audience not verified** — `api/app/auth.py:111` sets `options={"verify_aud": False}`. Any RS256 token issued by the Keycloak issuer for any client in the realm passes. Enable `verify_aud` pinned to `todo-tui`, or validate the `azp` claim explicitly.
3. **No rate limiting** — add per-IP and per-key rate limits to authentication-sensitive endpoints (`/me`, future `/auth/*`) plus a global cap, either in-process (`slowapi`) or at Caddy (`rate_limit` handler). API-key brute force and token-replay are currently unthrottled.
4. **API key lifecycle** — keys have no enforced expiry or rotation workflow. Add a configurable TTL, a rotation command, and surface `last_used_at` / `revoked_at` through an authenticated `/keys` endpoint so the owner can audit and revoke keys.
5. **Audit logging** — add a dedicated structured-log stream for security events: failed auth, API key creation/revocation, OIDC user linking (`auth.py:134`), bootstrap user creation. The generic `request` log in `RequestLogMiddleware` is not a substitute.
6. **Request body size limit** — add a middleware that rejects bodies above a sane cap (e.g. 1 MiB) before they reach route handlers. Uvicorn provides no application-level cap by default.
7. **Error handler leakage guard** — `api/app/main.py:48` forwards `exc.detail` verbatim. Add a guard that strips the detail in 5xx responses and never surfaces exception messages from unhandled errors.
8. **Defense-in-depth response headers** — even with Caddy-provided headers, mirror a minimal set (`X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`) at the API so the guarantees hold if Caddy is bypassed (e.g. via an internal tunnel).

Acceptance criteria: integration test confirms CORS rejects a non-whitelisted origin; a JWT with wrong `aud` returns 401; `/me` returns 429 under rapid repeated calls; a 5 MiB POST returns 413; every failed API-key lookup produces an `audit` log entry distinct from the `request` log.

# T0012: Keycloak realm hardening

Harden the `todo` realm beyond the HTTPS-enforcement scope of T0008.

1. **Brute-force protection disabled** — `deploy/keycloak/realm-todo.json` does not set `bruteForceProtected: true`. Enable it with tuned `failureFactor`, `maxDeltaTimeSeconds`, and `maxFailureWaitSeconds`.
2. **No password policy** — add `passwordPolicy` (minimum length, character classes, password history, maximum age) to the realm import.
3. **Committed bootstrap password** — `realm-todo.json:54` stores `"value": "changeme"` for `todo-admin`. Even with `temporary: true` this is in git history. Move the initial credential out of the repo: seed from an env-var only on first import, or trigger an email reset flow instead.
4. **`webOrigins: ["+"]`** — narrow to explicit origins required by the TUI redirect flow; the `+` glob is broader than a native CLI client needs.
5. **Session and token lifetimes** — set SSO idle/absolute timeouts, access-token TTL, and `refreshTokenMaxReuse: 0`. Default lifetimes are longer than appropriate for a security-critical realm.
6. **MFA / WebAuthn** — require a second factor (TOTP or WebAuthn) at minimum for `todo-admin`; optional but recommended for regular users.
7. **Admin bootstrap credentials** — `KC_BOOTSTRAP_ADMIN_PASSWORD` from env is only used on first boot but persists in the compose file and shell env. Document a rotation runbook: log in once, create a real admin user with MFA, delete the bootstrap admin.
8. **Event logging** — enable Keycloak's `eventsListeners` with persistence (login success/failure, admin actions) so an audit trail survives restarts and is queryable via the admin API.

Acceptance criteria: realm JSON enables brute-force protection and a password policy; no plaintext password remains in version control; `kcadm.sh` confirms event logging is enabled; the admin user has an MFA credential registered; token lifetimes are explicitly set (not defaults).

# T0013: Supply chain and dependency hygiene

Reduce the risk of compromised dependencies or base images reaching production.

1. **Image vulnerability scanning** — add a CI step (Trivy or Grype) that scans built images (`api`, `automation`) and the pinned base images (`postgres`, `caddy`, `keycloak`). Fail the build on HIGH/CRITICAL findings.
2. **Python and Rust dependency auditing** — add `pip-audit` (or `uv pip audit`) for `api/` and `automation/`, and `cargo audit` for the Rust workspace, to the `just check` pipeline.
3. **SBOM generation** — generate an SBOM (Syft) for each released image and publish it as a release artifact.
4. **Dependabot / Renovate** — configure automated update PRs for Python deps, Rust crates, GitHub Actions, and Docker base images.
5. **Dependency pinning** — `api/pyproject.toml` and `automation/pyproject.toml` specify lower bounds only (`fastapi>=0.115`, etc.). Generate and commit a `uv.lock` (or equivalent) for reproducible, auditable builds.
6. **`uv:latest` in `automation/Dockerfile`** — replace with a pinned version matching `api/Dockerfile` (`uv:0.9.16`).
7. **Signed commits and images** — enable `commit.gpgSign` (already present in the user's workflow per git policy), and sign released images with `cosign` keyless signing tied to the GitHub OIDC token. Verify signatures in the deployment pipeline before `docker compose up`.

Acceptance criteria: CI fails on any HIGH+ CVE in images or code deps; every release tag has a matching signed image and published SBOM; Dependabot PRs appear automatically on base-image or dep updates; `automation/Dockerfile` no longer references `uv:latest`.