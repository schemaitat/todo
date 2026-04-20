from __future__ import annotations

import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
async def test_each_mutation_produces_one_event(api: tuple[AsyncClient, str]) -> None:
    client, _ = api

    from app.db import get_database
    from app.models import Event
    from sqlalchemy import func, select

    db = get_database()

    async def count_events() -> int:
        async with db.session() as session:
            return (await session.execute(select(func.count()).select_from(Event))).scalar_one()

    # create context => +1 event
    before = await count_events()
    await client.post("/contexts", json={"slug": "work", "name": "Work"})
    assert await count_events() == before + 1

    # create todo => +1
    r = await client.post("/contexts/work/todos", json={"title": "x"})
    tid = r.json()["id"]
    assert await count_events() == before + 2

    # rename todo => +1
    await client.patch(f"/todos/{tid}", json={"title": "x2"})
    assert await count_events() == before + 3

    # toggle => +1
    await client.patch(f"/todos/{tid}", json={"done": True})
    assert await count_events() == before + 4

    # delete => +1
    await client.delete(f"/todos/{tid}")
    assert await count_events() == before + 5

    # no-op patch => +0
    r = await client.post("/contexts/work/todos", json={"title": "y"})
    yid = r.json()["id"]
    base = await count_events()
    await client.patch(f"/todos/{yid}", json={"title": "y"})
    assert await count_events() == base  # nothing changed


@pytest.mark.asyncio
async def test_cross_user_access_blocked(api: tuple[AsyncClient, str]) -> None:
    client, _ = api

    # Make a second user + key manually.
    from app.auth import key_lookup_digest
    from app.db import get_database
    from app.models import ApiKey, Context, User

    db = get_database()
    other_key = "todo_other_user_key_abcdefghijkl"
    async with db.session() as session:
        u = User(email="other@example.com", display_name="other")
        session.add(u)
        await session.flush()
        session.add(Context(user_id=u.id, slug="inbox", name="inbox"))
        session.add(ApiKey(user_id=u.id, key_hash=key_lookup_digest(other_key), label="other"))
        await session.commit()

    # bootstrap user creates a todo
    r = await client.post("/contexts/inbox/todos", json={"title": "private"})
    tid = r.json()["id"]

    # other user cannot patch it
    r = await client.patch(
        f"/todos/{tid}",
        json={"title": "hax"},
        headers={"X-API-Key": other_key},
    )
    assert r.status_code == 404
