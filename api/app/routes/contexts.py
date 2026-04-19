from __future__ import annotations

from datetime import datetime, timezone

from fastapi import APIRouter, Depends, HTTPException, Query, status
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from ..auth import get_current_user
from ..db import session_dependency
from ..models import Context, EventKind, User
from ..schemas import ContextCreate, ContextOut, ContextUpdate
from ..services.events import record_event

router = APIRouter(prefix="/contexts", tags=["contexts"])


async def _load_context(session: AsyncSession, user: User, slug: str) -> Context:
    result = await session.execute(
        select(Context).where(Context.user_id == user.id, Context.slug == slug)
    )
    ctx = result.scalar_one_or_none()
    if ctx is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="context not found")
    return ctx


@router.get("", response_model=list[ContextOut])
async def list_contexts(
    include_archived: bool = Query(default=True),
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> list[Context]:
    stmt = select(Context).where(Context.user_id == user.id).order_by(Context.position, Context.created_at)
    if not include_archived:
        stmt = stmt.where(Context.archived_at.is_(None))
    return list((await session.execute(stmt)).scalars())


@router.post("", response_model=ContextOut, status_code=status.HTTP_201_CREATED)
async def create_context(
    body: ContextCreate,
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> Context:
    existing_count = (
        await session.execute(
            select(Context).where(Context.user_id == user.id).order_by(Context.position.desc())
        )
    ).scalars().all()
    next_position = (existing_count[0].position + 1) if existing_count else 0

    ctx = Context(
        user_id=user.id,
        slug=body.slug,
        name=body.name,
        color=body.color or "#8888ff",
        position=next_position,
    )
    session.add(ctx)
    try:
        await session.flush()
    except IntegrityError as e:
        await session.rollback()
        raise HTTPException(status_code=status.HTTP_409_CONFLICT, detail="slug already exists") from e
    await record_event(
        session,
        user_id=user.id,
        context_id=ctx.id,
        entity_type="context",
        entity_id=ctx.id,
        kind=EventKind.CONTEXT_CREATED,
        payload={"slug": ctx.slug, "name": ctx.name},
    )
    await session.commit()
    await session.refresh(ctx)
    return ctx


@router.patch("/{slug}", response_model=ContextOut)
async def update_context(
    slug: str,
    body: ContextUpdate,
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> Context:
    ctx = await _load_context(session, user, slug)
    renamed = False
    if body.name is not None and body.name != ctx.name:
        ctx.name = body.name
        renamed = True
    if body.color is not None:
        ctx.color = body.color
    if body.position is not None:
        ctx.position = body.position
    if body.slug is not None and body.slug != ctx.slug:
        ctx.slug = body.slug
        renamed = True

    if renamed:
        await record_event(
            session,
            user_id=user.id,
            context_id=ctx.id,
            entity_type="context",
            entity_id=ctx.id,
            kind=EventKind.CONTEXT_RENAMED,
            payload={"slug": ctx.slug, "name": ctx.name},
        )

    try:
        await session.commit()
    except IntegrityError as e:
        await session.rollback()
        raise HTTPException(status_code=status.HTTP_409_CONFLICT, detail="slug already exists") from e
    await session.refresh(ctx)
    return ctx


@router.delete("/{slug}", status_code=status.HTTP_204_NO_CONTENT)
async def archive_context(
    slug: str,
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> None:
    if slug == "inbox":
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST, detail="inbox context cannot be archived"
        )
    ctx = await _load_context(session, user, slug)
    if ctx.archived_at is None:
        ctx.archived_at = datetime.now(timezone.utc)
        await record_event(
            session,
            user_id=user.id,
            context_id=ctx.id,
            entity_type="context",
            entity_id=ctx.id,
            kind=EventKind.CONTEXT_ARCHIVED,
            payload={"slug": ctx.slug},
        )
    await session.commit()
