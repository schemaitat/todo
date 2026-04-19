from __future__ import annotations

from datetime import UTC, datetime

from fastapi import APIRouter, Depends, HTTPException, Query, status
from fastapi.responses import HTMLResponse, PlainTextResponse, Response
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from ..auth import get_current_user
from ..db import session_dependency
from ..models import Context, Note, Todo, User
from ..schemas import (
    SnapshotContextJsonOut,
    SnapshotFormat,
    SnapshotJsonOut,
    SnapshotNoteOut,
    SnapshotTodoOut,
)
from ..services.snapshot import format_html, format_plain

router = APIRouter(prefix="/snapshot", tags=["snapshot"])

ContextsData = list[tuple[Context, list[Todo], list[Note]]]


async def _load_data(session: AsyncSession, user: User, slug: str | None) -> ContextsData:
    if slug is not None:
        ctx = (
            await session.execute(
                select(Context).where(Context.user_id == user.id, Context.slug == slug)
            )
        ).scalar_one_or_none()
        if ctx is None:
            raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="context not found")
        contexts = [ctx]
    else:
        contexts = list(
            (
                await session.execute(
                    select(Context)
                    .where(Context.user_id == user.id, Context.archived_at.is_(None))
                    .order_by(Context.position)
                )
            ).scalars()
        )

    result: ContextsData = []
    for ctx in contexts:
        todos = list(
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
        notes = list(
            (
                await session.execute(
                    select(Note)
                    .where(Note.context_id == ctx.id, Note.deleted_at.is_(None))
                    .order_by(Note.created_at)
                )
            ).scalars()
        )
        result.append((ctx, todos, notes))

    return result


@router.get("")
async def get_snapshot(
    context: str | None = Query(default=None),
    format: SnapshotFormat = Query(default="plain"),
    include_notes: bool = Query(default=True),
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> Response:
    data = await _load_data(session, user, context)
    if not include_notes:
        data = [(ctx, todos, []) for ctx, todos, _ in data]
    now = datetime.now(UTC)

    if format == "html":
        return HTMLResponse(format_html(data, now=now))
    if format == "plain":
        return PlainTextResponse(format_plain(data, now=now))

    payload = SnapshotJsonOut(
        generated_at=now,
        contexts=[
            SnapshotContextJsonOut(
                slug=ctx.slug,
                name=ctx.name,
                open_todos=[SnapshotTodoOut(id=t.id, title=t.title, done=t.done) for t in todos],
                notes=[SnapshotNoteOut(id=n.id, title=n.title, body=n.body) for n in notes],
            )
            for ctx, todos, notes in data
        ],
    )
    return Response(content=payload.model_dump_json(), media_type="application/json")
