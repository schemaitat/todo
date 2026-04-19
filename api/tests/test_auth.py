from __future__ import annotations

import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
async def test_health_is_unauthenticated(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.get("/health", headers={"X-API-Key": ""})
    assert r.status_code == 200
    assert r.json() == {"status": "ok"}


@pytest.mark.asyncio
async def test_me_requires_api_key(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.get("/me", headers={"X-API-Key": ""})
    assert r.status_code == 401
    assert r.json()["detail"] == "missing api key"


@pytest.mark.asyncio
async def test_me_rejects_wrong_key(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.get("/me", headers={"X-API-Key": "todo_wrong"})
    assert r.status_code == 401
    assert r.json()["detail"] == "invalid api key"


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
    from datetime import datetime, timezone

    from sqlalchemy import update

    from app.auth import key_lookup_digest
    from app.db import get_database
    from app.models import ApiKey

    db = get_database()
    digest = key_lookup_digest(raw_key)
    async with db.session() as session:
        await session.execute(
            update(ApiKey).where(ApiKey.key_hash == digest).values(revoked_at=datetime.now(timezone.utc))
        )
        await session.commit()

    r = await client.get("/me")
    assert r.status_code == 401


@pytest.mark.asyncio
async def test_last_used_at_updates(api: tuple[AsyncClient, str]) -> None:
    client, raw_key = api

    from sqlalchemy import select

    from app.auth import key_lookup_digest
    from app.db import get_database
    from app.models import ApiKey

    db = get_database()
    digest = key_lookup_digest(raw_key)
    r = await client.get("/me")
    assert r.status_code == 200
    async with db.session() as session:
        ak = (
            await session.execute(select(ApiKey).where(ApiKey.key_hash == digest))
        ).scalar_one()
        assert ak.last_used_at is not None
