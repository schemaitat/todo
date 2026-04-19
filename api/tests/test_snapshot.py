from __future__ import annotations

import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
async def test_snapshot_plain_sections(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    await client.post("/contexts/inbox/todos", json={"title": "write tests"})
    r = await client.post("/contexts/inbox/todos", json={"title": "ship"})
    tid = r.json()["id"]
    await client.patch(f"/todos/{tid}", json={"done": True})
    await client.post("/contexts/inbox/notes", json={"title": "plan", "body": "p1\np2"})

    r = await client.get("/snapshot", params={"context": "inbox", "format": "plain"})
    assert r.status_code == 200
    body = r.text
    assert "## Open todos (1)" in body
    assert "- [ ] write tests" in body
    assert "ship" not in body
    assert "### plan" in body
    assert "p1\np2" in body


@pytest.mark.asyncio
async def test_snapshot_html_escapes(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    await client.post("/contexts/inbox/todos", json={"title": "ship <feature>"})
    await client.post("/contexts/inbox/notes", json={"title": "title & co", "body": "<b>x</b>"})
    r = await client.get("/snapshot", params={"context": "inbox", "format": "html"})
    assert r.status_code == 200
    html = r.text
    assert "ship &lt;feature&gt;" in html
    assert "title &amp; co" in html
    assert "&lt;b&gt;x&lt;/b&gt;" in html
    assert "<b>x</b>" not in html


@pytest.mark.asyncio
async def test_snapshot_json(api: tuple[AsyncClient, str]) -> None:
    client, _ = api
    await client.post("/contexts/inbox/todos", json={"title": "alpha"})
    r = await client.get("/snapshot", params={"context": "inbox", "format": "json"})
    assert r.status_code == 200
    data = r.json()
    assert data["context"]["slug"] == "inbox"
    assert [t["title"] for t in data["open_todos"]] == ["alpha"]
