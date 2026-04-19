from __future__ import annotations

import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
async def test_mutations_produce_events(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    r = await client.post("/contexts/inbox/todos", json={"title": "a"})
    tid = r.json()["id"]
    await client.patch(f"/todos/{tid}", json={"done": True})
    await client.patch(f"/todos/{tid}", json={"title": "b"})
    await client.delete(f"/todos/{tid}")
    r = await client.get("/events", params={"context": "inbox"})
    kinds = [e["kind"] for e in r.json()]
    # newest first — recorded order: deleted, renamed, toggled, created
    assert kinds == ["TodoDeleted", "TodoRenamed", "TodoToggled", "TodoCreated"]


@pytest.mark.asyncio
async def test_event_filtering_by_context(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    await client.post("/contexts", json={"slug": "work", "name": "Work"})
    await client.post("/contexts/inbox/todos", json={"title": "inbox-1"})
    await client.post("/contexts/work/todos", json={"title": "work-1"})
    r = await client.get("/events", params={"context": "work"})
    kinds = [e["kind"] for e in r.json()]
    assert kinds == ["TodoCreated", "ContextCreated"]


@pytest.mark.asyncio
async def test_event_limit(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    for i in range(5):
        await client.post("/contexts/inbox/todos", json={"title": str(i)})
    r = await client.get("/events", params={"context": "inbox", "limit": 2})
    assert len(r.json()) == 2
