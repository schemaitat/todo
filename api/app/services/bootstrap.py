from __future__ import annotations

from sqlalchemy import func, select
from sqlalchemy.ext.asyncio import AsyncSession

from ..auth import generate_api_key, key_lookup_digest
from ..logging_ import log
from ..models import ApiKey, Context, User
from ..settings import get_settings


async def ensure_bootstrap(session: AsyncSession) -> None:
    settings = get_settings()
    existing_users = (await session.execute(select(func.count()).select_from(User))).scalar_one()
    if existing_users > 0:
        return

    user = User(
        email=settings.bootstrap_user_email, display_name=settings.bootstrap_user_display_name
    )
    session.add(user)
    await session.flush()

    inbox = Context(user_id=user.id, slug="inbox", name="inbox", color="#8888ff", position=0)
    session.add(inbox)

    raw_key = settings.bootstrap_api_key or generate_api_key()
    api_key = ApiKey(
        user_id=user.id,
        key_hash=key_lookup_digest(raw_key),
        label=settings.bootstrap_api_key_label,
    )
    session.add(api_key)
    await session.commit()

    if settings.bootstrap_api_key is None:
        log.warning(
            "bootstrap.api_key_generated",
            message="=== todo-api bootstrap key (store it, it is not shown again) ===",
            api_key=raw_key,
            user_id=str(user.id),
            email=user.email,
        )
    else:
        log.info("bootstrap.ready", user_id=str(user.id), email=user.email)
