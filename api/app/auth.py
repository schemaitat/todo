from __future__ import annotations

import hashlib
import secrets
from datetime import UTC, datetime
from functools import lru_cache
from typing import Any

import jwt
from argon2 import PasswordHasher
from argon2.exceptions import VerifyMismatchError
from fastapi import Depends, Header, HTTPException, status
from jwt import PyJWKClient, PyJWKClientError
from sqlalchemy import select, update
from sqlalchemy.ext.asyncio import AsyncSession

from .db import session_dependency
from .logging_ import bind_user_id
from .models import ApiKey, Context, User
from .settings import get_settings

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
        update(ApiKey).where(ApiKey.id == api_key.id).values(last_used_at=datetime.now(UTC))
    )
    await session.commit()
    return user


@lru_cache(maxsize=1)
def _jwks_client(issuer: str, jwks_base: str | None) -> PyJWKClient:
    base = jwks_base or issuer
    jwks_url = f"{base}/protocol/openid-connect/certs"
    return PyJWKClient(jwks_url, cache_jwk_set=True, lifespan=300)


async def resolve_bearer_token(session: AsyncSession, token: str) -> User | None:
    settings = get_settings()
    if not settings.oidc_issuer or not settings.oidc_client_id:
        return None

    try:
        client = _jwks_client(settings.oidc_issuer, settings.oidc_jwks_url)
        signing_key = client.get_signing_key_from_jwt(token)
        claims: dict[str, Any] = jwt.decode(
            token,
            signing_key.key,
            algorithms=["RS256"],
            issuer=settings.oidc_issuer,
            options={"verify_aud": False},
        )
    except (PyJWKClientError, jwt.PyJWTError) as exc:
        import logging

        logging.getLogger(__name__).warning("JWT validation failed: %s", exc)
        return None

    # Use sub if present, fall back to email (Keycloak 26 omits sub from access tokens by default).
    sub: str = claims.get("sub") or claims.get("email", "")
    if not sub:
        return None

    email: str = claims.get("email", f"{sub}@oidc.local")
    display_name: str = claims.get("name") or claims.get("preferred_username") or email

    user = (
        await session.execute(select(User).where(User.external_sub == sub))
    ).scalar_one_or_none()

    if user is None:
        # Link existing user by email (e.g. bootstrap user logging in via OIDC for first time).
        user = (await session.execute(select(User).where(User.email == email))).scalar_one_or_none()
        if user is not None:
            await session.execute(update(User).where(User.id == user.id).values(external_sub=sub))
            await session.commit()

    if user is None:
        user = User(email=email, display_name=display_name, external_sub=sub)
        session.add(user)
        await session.flush()
        session.add(Context(user_id=user.id, slug="inbox", name="inbox"))
        await session.commit()

    if user.disabled_at is not None:
        return None
    return user


async def get_current_user(
    x_api_key: str | None = Header(default=None, alias="X-API-Key"),
    authorization: str | None = Header(default=None),
    session: AsyncSession = Depends(session_dependency),
) -> User:
    if authorization and authorization.startswith("Bearer "):
        token = authorization.removeprefix("Bearer ").strip()
        user = await resolve_bearer_token(session, token)
        if user is not None:
            bind_user_id(str(user.id))
            return user

    if x_api_key:
        user = await resolve_api_key(session, x_api_key)
        if user is not None:
            bind_user_id(str(user.id))
            return user

    raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="unauthorized")
