from __future__ import annotations

import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
async def test_create_note(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.post("/contexts/inbox/notes", json={"title": "hi", "body": "hello"})
    assert r.status_code == 201
    data = r.json()
    assert data["title"] == "hi"
    assert data["body"] == "hello"


@pytest.mark.asyncio
async def test_update_note_body_and_title(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.post("/contexts/inbox/notes", json={"title": "t", "body": ""})
    nid = r.json()["id"]
    r = await client.patch(f"/notes/{nid}", json={"title": "t2", "body": "bbb"})
    assert r.status_code == 200
    assert r.json()["title"] == "t2"
    assert r.json()["body"] == "bbb"


@pytest.mark.asyncio
async def test_if_match_matches(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.post("/contexts/inbox/notes", json={"title": "t"})
    nid = r.json()["id"]
    r = await client.get(f"/notes/{nid}")
    last = r.headers["last-modified"]
    r = await client.patch(
        f"/notes/{nid}",
        json={"body": "fresh"},
        headers={"If-Match": last},
    )
    assert r.status_code == 200


@pytest.mark.asyncio
async def test_if_match_mismatch_returns_412(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.post("/contexts/inbox/notes", json={"title": "t"})
    nid = r.json()["id"]
    r = await client.patch(
        f"/notes/{nid}",
        json={"body": "once"},
        headers={"If-Match": "Wed, 01 Jan 2020 00:00:00 GMT"},
    )
    assert r.status_code == 412


@pytest.mark.asyncio
async def test_soft_delete_note(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.post("/contexts/inbox/notes", json={"title": "byebye"})
    nid = r.json()["id"]
    r = await client.delete(f"/notes/{nid}")
    assert r.status_code == 204
    r = await client.get("/contexts/inbox/notes")
    assert r.json() == []
