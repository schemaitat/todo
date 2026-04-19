from __future__ import annotations

from datetime import datetime, timezone
from email.utils import format_datetime, parsedate_to_datetime
from uuid import UUID

from fastapi import APIRouter, Depends, Header, HTTPException, Query, Response, status
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from ..auth import get_current_user
from ..db import session_dependency
from ..models import Context, EventKind, Note, User
from ..schemas import NoteCreate, NoteOut, NoteUpdate
from ..services.events import record_event

router = APIRouter(tags=["notes"])


async def _load_context(session: AsyncSession, user: User, slug: str) -> Context:
    result = await session.execute(
        select(Context).where(Context.user_id == user.id, Context.slug == slug)
    )
    ctx = result.scalar_one_or_none()
    if ctx is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="context not found")
    return ctx


async def _load_note(session: AsyncSession, user: User, note_id: UUID) -> Note:
    stmt = (
        select(Note)
        .join(Context, Note.context_id == Context.id)
        .where(Note.id == note_id, Context.user_id == user.id)
    )
    note = (await session.execute(stmt)).scalar_one_or_none()
    if note is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="note not found")
    return note


def _to_http_date(dt: datetime) -> str:
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return format_datetime(dt, usegmt=True)


@router.get("/contexts/{slug}/notes", response_model=list[NoteOut])
async def list_notes(
    slug: str,
    include_deleted: bool = Query(default=False),
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> list[Note]:
    ctx = await _load_context(session, user, slug)
    stmt = select(Note).where(Note.context_id == ctx.id).order_by(Note.created_at)
    if not include_deleted:
        stmt = stmt.where(Note.deleted_at.is_(None))
    return list((await session.execute(stmt)).scalars())


@router.post("/contexts/{slug}/notes", response_model=NoteOut, status_code=status.HTTP_201_CREATED)
async def create_note(
    slug: str,
    body: NoteCreate,
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> Note:
    ctx = await _load_context(session, user, slug)
    note = Note(context_id=ctx.id, title=body.title, body=body.body)
    session.add(note)
    await session.flush()
    await record_event(
        session,
        user_id=user.id,
        context_id=ctx.id,
        entity_type="note",
        entity_id=note.id,
        kind=EventKind.NOTE_CREATED,
        payload={"title": note.title},
    )
    await session.commit()
    await session.refresh(note)
    return note


@router.get("/notes/{note_id}", response_model=NoteOut)
async def get_note(
    note_id: UUID,
    response: Response,
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> Note:
    note = await _load_note(session, user, note_id)
    response.headers["Last-Modified"] = _to_http_date(note.updated_at)
    return note


@router.patch("/notes/{note_id}", response_model=NoteOut)
async def update_note(
    note_id: UUID,
    body: NoteUpdate,
    response: Response,
    if_match: str | None = Header(default=None, alias="If-Match"),
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> Note:
    note = await _load_note(session, user, note_id)

    if if_match is not None:
        try:
            expected = parsedate_to_datetime(if_match)
        except (TypeError, ValueError) as e:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST, detail="bad If-Match header"
            ) from e
        current = note.updated_at
        if current.tzinfo is None:
            current = current.replace(tzinfo=timezone.utc)
        if abs((current - expected).total_seconds()) > 1:
            raise HTTPException(
                status_code=status.HTTP_412_PRECONDITION_FAILED,
                detail="note changed elsewhere",
            )

    now = datetime.now(timezone.utc)
    events: list[tuple[EventKind, dict]] = []
    if body.title is not None and body.title != note.title:
        note.title = body.title
        events.append((EventKind.NOTE_RENAMED, {"title": note.title}))
    if body.body is not None and body.body != note.body:
        note.body = body.body
        events.append((EventKind.NOTE_EDITED, {"length": len(note.body)}))
    if events:
        note.updated_at = now

    for kind, payload in events:
        await record_event(
            session,
            user_id=user.id,
            context_id=note.context_id,
            entity_type="note",
            entity_id=note.id,
            kind=kind,
            payload=payload,
        )
    await session.commit()
    await session.refresh(note)
    response.headers["Last-Modified"] = _to_http_date(note.updated_at)
    return note


@router.delete("/notes/{note_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_note(
    note_id: UUID,
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> None:
    note = await _load_note(session, user, note_id)
    if note.deleted_at is None:
        note.deleted_at = datetime.now(timezone.utc)
        note.updated_at = note.deleted_at
        await record_event(
            session,
            user_id=user.id,
            context_id=note.context_id,
            entity_type="note",
            entity_id=note.id,
            kind=EventKind.NOTE_DELETED,
            payload={},
        )
    await session.commit()
