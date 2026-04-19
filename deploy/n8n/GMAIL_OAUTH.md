# Gmail OAuth2 setup for n8n workflows

Required once to enable the snapshot email and email-ingest workflows.

## 1. Create a Google Cloud project

1. Go to [console.cloud.google.com](https://console.cloud.google.com)
2. Create a new project (e.g. `todo-n8n`)
3. Enable the **Gmail API**: APIs & Services → Enable APIs → search Gmail API → Enable

## 2. Configure the OAuth consent screen

APIs & Services → OAuth consent screen:

- User type: **External**
- App name: `todo` (or anything)
- Support email: your Gmail address
- Scopes: add `https://www.googleapis.com/auth/gmail.modify`
- Test users: add your Gmail address (`a.schemaitat@gmail.com`)

Save and continue through all steps.

## 3. Create OAuth credentials

APIs & Services → Credentials → **Create credentials** → OAuth client ID:

- Application type: **Web application**
- Authorized redirect URI: `http://localhost:5678/rest/oauth2-credential/callback`

Copy the **Client ID** and **Client Secret**.

## 4. Create the credential in n8n

1. Open n8n at http://localhost:5678
2. **Credentials** → **Add credential** → search **Gmail OAuth2**
3. Paste Client ID and Client Secret
4. Click **Sign in with Google** → complete the consent screen
5. Save — note the credential name

## 5. Add credentials to .env

```
N8N_GOOGLE_CLIENT_ID=<your client id>
N8N_GOOGLE_CLIENT_SECRET=<your client secret>
```

These are for reference only; n8n stores the live tokens internally.

## Notes

- The app stays in "testing" mode for personal use — no Google verification needed
- Test users can be added/removed in the OAuth consent screen at any time
- If the refresh token expires, re-open the credential in n8n and click Sign in with Google again
