from __future__ import annotations

import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
async def test_inbox_exists_by_default(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.get("/contexts")
    assert r.status_code == 200
    slugs = [c["slug"] for c in r.json()]
    assert slugs == ["inbox"]


@pytest.mark.asyncio
async def test_create_context(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.post("/contexts", json={"slug": "work", "name": "Work", "color": "#ff00aa"})
    assert r.status_code == 201
    data = r.json()
    assert data["slug"] == "work"
    assert data["name"] == "Work"
    assert data["color"] == "#ff00aa"


@pytest.mark.asyncio
async def test_create_duplicate_slug_is_409(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    await client.post("/contexts", json={"slug": "work", "name": "Work"})
    r = await client.post("/contexts", json={"slug": "work", "name": "Other"})
    assert r.status_code == 409


@pytest.mark.asyncio
async def test_rename_context(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    await client.post("/contexts", json={"slug": "proj", "name": "Proj"})
    r = await client.patch("/contexts/proj", json={"name": "Projects"})
    assert r.status_code == 200
    assert r.json()["name"] == "Projects"


@pytest.mark.asyncio
async def test_archive_context(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    await client.post("/contexts", json={"slug": "tmp", "name": "Tmp"})
    r = await client.delete("/contexts/tmp")
    assert r.status_code == 204
    r = await client.get("/contexts", params={"include_archived": "false"})
    slugs = [c["slug"] for c in r.json()]
    assert "tmp" not in slugs
    assert "inbox" in slugs


@pytest.mark.asyncio
async def test_cannot_archive_inbox(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.delete("/contexts/inbox")
    assert r.status_code == 400


@pytest.mark.asyncio
async def test_slug_validation(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.post("/contexts", json={"slug": "Bad Slug", "name": "x"})
    assert r.status_code == 422
