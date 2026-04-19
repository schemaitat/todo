from __future__ import annotations

from datetime import datetime

from fastapi import APIRouter, Depends, HTTPException, Query, status
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from ..auth import get_current_user
from ..db import session_dependency
from ..models import Context, Event, User
from ..schemas import EventOut

router = APIRouter(prefix="/events", tags=["events"])


@router.get("", response_model=list[EventOut])
async def list_events(
    context: str | None = Query(default=None),
    limit: int = Query(default=200, ge=1, le=1000),
    before: datetime | None = Query(default=None),
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> list[Event]:
    stmt = select(Event).where(Event.user_id == user.id).order_by(Event.ts.desc()).limit(limit)
    if context is not None:
        ctx = (
            await session.execute(
                select(Context).where(Context.user_id == user.id, Context.slug == context)
            )
        ).scalar_one_or_none()
        if ctx is None:
            raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="context not found")
        stmt = stmt.where(Event.context_id == ctx.id)
    if before is not None:
        stmt = stmt.where(Event.ts < before)
    return list((await session.execute(stmt)).scalars())
