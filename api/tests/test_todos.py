from __future__ import annotations

import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
async def test_create_and_list_todo(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.post("/contexts/inbox/todos", json={"title": "write tests"})
    assert r.status_code == 201
    r = await client.get("/contexts/inbox/todos")
    assert r.status_code == 200
    titles = [t["title"] for t in r.json()]
    assert titles == ["write tests"]


@pytest.mark.asyncio
async def test_toggle_todo(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.post("/contexts/inbox/todos", json={"title": "x"})
    tid = r.json()["id"]
    r = await client.patch(f"/todos/{tid}", json={"done": True})
    assert r.status_code == 200
    assert r.json()["done"] is True


@pytest.mark.asyncio
async def test_rename_todo(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.post("/contexts/inbox/todos", json={"title": "old"})
    tid = r.json()["id"]
    r = await client.patch(f"/todos/{tid}", json={"title": "new"})
    assert r.status_code == 200
    assert r.json()["title"] == "new"


@pytest.mark.asyncio
async def test_soft_delete_todo(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.post("/contexts/inbox/todos", json={"title": "delete me"})
    tid = r.json()["id"]
    r = await client.delete(f"/todos/{tid}")
    assert r.status_code == 204
    r = await client.get("/contexts/inbox/todos")
    assert r.json() == []
    r = await client.get("/contexts/inbox/todos", params={"include_deleted": "true"})
    assert len(r.json()) == 1
    assert r.json()[0]["deleted_at"] is not None


@pytest.mark.asyncio
async def test_missing_context_is_404(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.post("/contexts/nope/todos", json={"title": "x"})
    assert r.status_code == 404


@pytest.mark.asyncio
async def test_missing_todo_is_404(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.patch("/todos/00000000-0000-0000-0000-000000000000", json={"done": True})
    assert r.status_code == 404


@pytest.mark.asyncio
async def test_include_done_filter(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.post("/contexts/inbox/todos", json={"title": "a"})
    r = await client.post("/contexts/inbox/todos", json={"title": "b"})
    tid = r.json()["id"]
    await client.patch(f"/todos/{tid}", json={"done": True})
    r = await client.get("/contexts/inbox/todos", params={"include_done": "false"})
    titles = [t["title"] for t in r.json()]
    assert titles == ["a"]
