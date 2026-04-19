#!/usr/bin/env bash
set -euo pipefail

prefect server start --host 0.0.0.0 --port 4200 &

# Wait for server to be ready
until python -c "
import urllib.request, sys
try:
    urllib.request.urlopen('http://localhost:4200/api/health')
except Exception:
    sys.exit(1)
" 2>/dev/null; do sleep 1; done

export PREFECT_API_URL=http://localhost:4200/api
exec python flows/snapshot_email.py
