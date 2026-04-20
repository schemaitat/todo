from __future__ import annotations

from datetime import UTC

import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
async def test_health_is_unauthenticated(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.get("/health", headers={"X-API-Key": ""})
    assert r.status_code == 200
    assert r.json() == {"status": "ok"}


@pytest.mark.asyncio
async def test_me_requires_auth(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.get("/me", headers={"X-API-Key": ""})
    assert r.status_code == 401
    assert r.json()["detail"] == "unauthorized"


@pytest.mark.asyncio
async def test_me_rejects_wrong_key(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.get("/me", headers={"X-API-Key": "todo_wrong"})
    assert r.status_code == 401
    assert r.json()["detail"] == "unauthorized"


@pytest.mark.asyncio
async def test_me_returns_user(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.get("/me")
    assert r.status_code == 200
    body = r.json()
    assert body["email"] == "test@example.com"
    assert "id" in body


@pytest.mark.asyncio
async def test_revoked_key_is_rejected(api: tuple[AsyncClient, str]) -> None:
    client, raw_key = api
    from datetime import datetime

    from app.auth import key_lookup_digest
    from app.db import get_database
    from app.models import ApiKey
    from sqlalchemy import update

    db = get_database()
    digest = key_lookup_digest(raw_key)
    async with db.session() as session:
        await session.execute(
            update(ApiKey).where(ApiKey.key_hash == digest).values(revoked_at=datetime.now(UTC))
        )
        await session.commit()

    r = await client.get("/me")
    assert r.status_code == 401


@pytest.mark.asyncio
async def test_last_used_at_updates(api: tuple[AsyncClient, str]) -> None:
    client, raw_key = api

    from app.auth import key_lookup_digest
    from app.db import get_database
    from app.models import ApiKey
    from sqlalchemy import select

    db = get_database()
    digest = key_lookup_digest(raw_key)
    r = await client.get("/me")
    assert r.status_code == 200
    async with db.session() as session:
        ak = (await session.execute(select(ApiKey).where(ApiKey.key_hash == digest))).scalar_one()
        assert ak.last_used_at is not None


@pytest.mark.asyncio
async def test_bearer_invalid_token_rejected(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.get("/me", headers={"Authorization": "Bearer not.a.jwt", "X-API-Key": ""})
    assert r.status_code == 401


@pytest.mark.asyncio
async def test_bearer_requires_oidc_configured(api: tuple[AsyncClient, str]) -> None:
    """When OIDC is not configured, Bearer tokens are rejected (no issuer to validate against)."""
    client, _ = api
    r = await client.get(
        "/me",
        headers={"Authorization": "Bearer eyJhbGciOiJSUzI1NiJ9.e30.sig", "X-API-Key": ""},
    )
    assert r.status_code == 401


@pytest.mark.asyncio
async def test_oidc_auto_provisions_user(api: tuple[AsyncClient, str]) -> None:
    """A valid JWT auto-creates the user and inbox context on first login."""
    import time
    from unittest.mock import MagicMock, patch

    import app.auth as auth_module
    import jwt as pyjwt
    from app.db import get_database
    from app.models import User
    from app.settings import get_settings
    from cryptography.hazmat.primitives.asymmetric import rsa
    from sqlalchemy import select

    http_client, _ = api

    # Generate throwaway RSA key pair for the test.
    private_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    public_key = private_key.public_key()

    now = int(time.time())
    claims = {
        "sub": "oidc-user-42",
        "iss": "http://keycloak-test/realms/todo",
        "aud": "todo-tui",
        "iat": now,
        "exp": now + 3600,
        "email": "oidc@example.com",
        "name": "OIDC User",
        "preferred_username": "oidcuser",
    }
    token = pyjwt.encode(claims, private_key, algorithm="RS256")

    mock_signing_key = MagicMock()
    mock_signing_key.key = public_key
    mock_jwks_client = MagicMock()
    mock_jwks_client.get_signing_key_from_jwt.return_value = mock_signing_key

    settings = get_settings()

    with (
        patch.object(settings, "oidc_issuer", "http://keycloak-test/realms/todo"),
        patch.object(settings, "oidc_client_id", "todo-tui"),
        patch.object(auth_module, "_jwks_client", return_value=mock_jwks_client),
    ):
        r = await http_client.get(
            "/me",
            headers={"Authorization": f"Bearer {token}", "X-API-Key": ""},
        )

    assert r.status_code == 200
    body = r.json()
    assert body["email"] == "oidc@example.com"
    assert body["display_name"] == "OIDC User"

    db = get_database()
    async with db.session() as session:
        user = (
            await session.execute(select(User).where(User.external_sub == "oidc-user-42"))
        ).scalar_one_or_none()
        assert user is not None
        assert user.email == "oidc@example.com"
