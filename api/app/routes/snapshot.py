from __future__ import annotations

from datetime import datetime, timezone

from fastapi import APIRouter, Depends, HTTPException, Query, status
from fastapi.responses import HTMLResponse, PlainTextResponse, Response
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from ..auth import get_current_user
from ..db import session_dependency
from ..models import Context, Note, Todo, User
from ..schemas import SnapshotFormat, SnapshotJsonOut, SnapshotNoteOut, SnapshotTodoOut
from ..services.snapshot import format_html, format_plain

router = APIRouter(prefix="/snapshot", tags=["snapshot"])


async def _load_snapshot_data(
    session: AsyncSession, user: User, slug: str
) -> tuple[Context, list[Todo], list[Note]]:
    ctx = (
        await session.execute(
            select(Context).where(Context.user_id == user.id, Context.slug == slug)
        )
    ).scalar_one_or_none()
    if ctx is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="context not found")

    open_todos = list(
        (
            await session.execute(
                select(Todo)
                .where(
                    Todo.context_id == ctx.id,
                    Todo.deleted_at.is_(None),
                    Todo.done.is_(False),
                )
                .order_by(Todo.created_at)
            )
        ).scalars()
    )
    live_notes = list(
        (
            await session.execute(
                select(Note)
                .where(Note.context_id == ctx.id, Note.deleted_at.is_(None))
                .order_by(Note.created_at)
            )
        ).scalars()
    )
    return ctx, open_todos, live_notes


@router.get("")
async def get_snapshot(
    context: str = Query(default="inbox"),
    format: SnapshotFormat = Query(default="plain"),
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> Response:
    ctx, open_todos, live_notes = await _load_snapshot_data(session, user, context)
    now = datetime.now(timezone.utc)

    if format == "html":
        return HTMLResponse(format_html(open_todos, live_notes, now=now))
    if format == "plain":
        return PlainTextResponse(format_plain(open_todos, live_notes, now=now))

    payload = SnapshotJsonOut(
        context=ctx,  # type: ignore[arg-type]
        generated_at=now,
        open_todos=[SnapshotTodoOut(id=t.id, title=t.title, done=t.done) for t in open_todos],
        notes=[SnapshotNoteOut(id=n.id, title=n.title, body=n.body) for n in live_notes],
    )
    return Response(content=payload.model_dump_json(), media_type="application/json")
