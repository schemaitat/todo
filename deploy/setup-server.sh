#!/usr/bin/env bash
# One-shot bootstrap for a fresh Debian/Ubuntu VPS.
# Run as root or a user with sudo.
set -euo pipefail

REPO_DIR="${REPO_DIR:-/srv/todo}"

echo "=== 1. System update ==="
apt-get update -qq && apt-get upgrade -y -qq

echo "=== 2. Install Docker ==="
if ! command -v docker &>/dev/null; then
  curl -fsSL https://get.docker.com | sh
  systemctl enable --now docker
fi

echo "=== 3. Firewall (ufw) ==="
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw allow 443/udp   # HTTP/3 QUIC
ufw --force enable

echo "=== 4. Create app directory ==="
mkdir -p "$REPO_DIR"
chown -R "${SUDO_USER:-$USER}":"${SUDO_USER:-$USER}" "$REPO_DIR"

echo ""
echo "Done. Next steps:"
echo "  1. rsync the repo to $REPO_DIR  (just remote-deploy SERVER=user@host)"
echo "  2. cp $REPO_DIR/deploy/.env.prod.example $REPO_DIR/deploy/.env"
echo "  3. Edit $REPO_DIR/deploy/.env - set DOMAIN, passwords, API key"
echo "  4. just remote-stack-up SERVER=user@host"
