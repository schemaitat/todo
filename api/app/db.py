from __future__ import annotations

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager

from sqlalchemy.ext.asyncio import (
    AsyncEngine,
    AsyncSession,
    async_sessionmaker,
    create_async_engine,
)

from .settings import get_settings


class Database:
    def __init__(self, url: str) -> None:
        self._engine: AsyncEngine = create_async_engine(url, future=True, pool_pre_ping=True)
        self._session_factory = async_sessionmaker(self._engine, expire_on_commit=False)

    @property
    def engine(self) -> AsyncEngine:
        return self._engine

    @asynccontextmanager
    async def session(self) -> AsyncIterator[AsyncSession]:
        async with self._session_factory() as session:
            yield session

    async def dispose(self) -> None:
        await self._engine.dispose()


_db: Database | None = None


def get_database() -> Database:
    global _db
    if _db is None:
        _db = Database(get_settings().database_url)
    return _db


async def set_database(database: Database) -> None:
    """Override the process-wide database (used by tests)."""
    global _db
    if _db is not None and _db is not database:
        await _db.dispose()
    _db = database


async def session_dependency() -> AsyncIterator[AsyncSession]:
    db = get_database()
    async with db.session() as session:
        yield session
