from __future__ import annotations

import contextlib
import os
import tempfile
from collections.abc import AsyncIterator
from pathlib import Path

import pytest
import pytest_asyncio
from httpx import ASGITransport, AsyncClient

os.environ.setdefault("LOG_JSON", "false")
os.environ.setdefault("LOG_LEVEL", "WARNING")


@pytest_asyncio.fixture
async def api(monkeypatch: pytest.MonkeyPatch) -> AsyncIterator[tuple[AsyncClient, str]]:
    from app.db import Database, set_database
    from app.models import Base
    from app.services.bootstrap import ensure_bootstrap
    from app.settings import get_settings

    tmpdir = Path(tempfile.mkdtemp())
    db_path = tmpdir / "test.db"
    raw_key = "todo_test_key_abcdefghijklmnop"

    monkeypatch.setenv("DATABASE_URL", f"sqlite+aiosqlite:///{db_path}")
    monkeypatch.setenv("BOOTSTRAP_USER_EMAIL", "test@example.com")
    monkeypatch.setenv("BOOTSTRAP_API_KEY", raw_key)
    monkeypatch.setenv("LOG_JSON", "false")
    get_settings.cache_clear()

    database = Database(f"sqlite+aiosqlite:///{db_path}")
    await set_database(database)

    async with database.engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)
    async with database.session() as session:
        await ensure_bootstrap(session)

    from app.main import create_app

    app = create_app()
    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        client.headers.update({"X-API-Key": raw_key})
        yield client, raw_key

    await database.dispose()
    db_path.unlink(missing_ok=True)
    with contextlib.suppress(OSError):
        tmpdir.rmdir()
    get_settings.cache_clear()
