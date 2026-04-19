from __future__ import annotations

import hashlib
import secrets
from datetime import datetime, timezone

from argon2 import PasswordHasher
from argon2.exceptions import VerifyMismatchError
from fastapi import Depends, Header, HTTPException, status
from sqlalchemy import select, update
from sqlalchemy.ext.asyncio import AsyncSession

from .db import session_dependency
from .logging_ import bind_user_id
from .models import ApiKey, User

_ph = PasswordHasher()

API_KEY_PREFIX = "todo_"


def generate_api_key() -> str:
    return API_KEY_PREFIX + secrets.token_urlsafe(32)


def hash_api_key(raw: str) -> str:
    return _ph.hash(raw)


def key_lookup_digest(raw: str) -> str:
    """Fast, deterministic digest for index lookup. Collisions are astronomically unlikely for
    random 32-byte keys; the argon2 hash below is the real check."""
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


def _verify_hash(stored_hash: str, raw: str) -> bool:
    if stored_hash.startswith("$argon2"):
        try:
            _ph.verify(stored_hash, raw)
            return True
        except VerifyMismatchError:
            return False
    return secrets.compare_digest(stored_hash, key_lookup_digest(raw))


async def resolve_api_key(session: AsyncSession, raw_key: str) -> User | None:
    digest = key_lookup_digest(raw_key)

    # Primary path: O(1) lookup on sha256 digest.
    row = (
        await session.execute(
            select(ApiKey, User)
            .join(User, User.id == ApiKey.user_id)
            .where(ApiKey.key_hash == digest, ApiKey.revoked_at.is_(None))
        )
    ).first()

    # Fallback: tokens stored as argon2 hashes must be scanned (small N for a personal tool).
    if row is None:
        rows = (
            await session.execute(
                select(ApiKey, User)
                .join(User, User.id == ApiKey.user_id)
                .where(ApiKey.revoked_at.is_(None), ApiKey.key_hash.like("$argon2%"))
            )
        ).all()
        for ak, user in rows:
            if _verify_hash(ak.key_hash, raw_key):
                row = (ak, user)
                break

    if row is None:
        return None

    api_key, user = row
    if user.disabled_at is not None:
        return None

    await session.execute(
        update(ApiKey)
        .where(ApiKey.id == api_key.id)
        .values(last_used_at=datetime.now(timezone.utc))
    )
    await session.commit()
    return user


async def get_current_user(
    x_api_key: str | None = Header(default=None, alias="X-API-Key"),
    session: AsyncSession = Depends(session_dependency),
) -> User:
    if not x_api_key:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="missing api key")
    user = await resolve_api_key(session, x_api_key)
    if user is None:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="invalid api key")
    bind_user_id(str(user.id))
    return user
