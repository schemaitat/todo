# Tickets

## T0001 — Gmail OAuth2 blocked on raw IP for remote n8n

**Status:** open  
**Branch:** feat/remote-deploy

### Problem

Google OAuth2 does not allow raw IP addresses as redirect URIs. When trying
to authorize the Gmail credential in the remote n8n instance at
`http://128.140.46.93:5678`, Google rejects the callback URL with:

> Ungültige Weiterleitung: Muss mit einer öffentlichen Top-Level-Domain enden

### Context

- Local n8n (localhost) works fine — Google allows `localhost` as a redirect URI
- Remote n8n needs `http://<host>:5678/rest/oauth2-credential/callback` as an
  authorized redirect URI in the Google Cloud Console OAuth client
- This URI must use a registered domain, not a bare IP

### Resolution

Set up a real domain (or free DuckDNS subdomain, e.g. `todo-andre.duckdns.org`)
pointing to `128.140.46.93`, then:

1. Add `http://<domain>:5678/rest/oauth2-credential/callback` to the OAuth
   client's authorized redirect URIs in Google Cloud Console
2. Update `.env`:
   ```
   N8N_HOST=<domain>
   N8N_WEBHOOK_URL=http://<domain>:5678/
   ```
3. Redeploy n8n: `just deploy && just n8n-up-prod` (or restart the n8n container)
4. Re-create the Gmail OAuth2 credential in the remote n8n UI

Long-term: adding Caddy + TLS (Phase 10) gives a proper HTTPS domain and makes
this a non-issue.
