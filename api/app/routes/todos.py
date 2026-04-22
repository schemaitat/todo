from __future__ import annotations

from datetime import UTC, datetime
from email.utils import format_datetime, parsedate_to_datetime
from uuid import UUID

from fastapi import APIRouter, Depends, Header, HTTPException, Query, Response, status
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from ..auth import get_current_user
from ..db import session_dependency
from ..models import Context, EventKind, Todo, User
from ..schemas import TodoCreate, TodoOut, TodoUpdate
from ..services.events import record_event

router = APIRouter(tags=["todos"])


async def _load_context(session: AsyncSession, user: User, slug: str) -> Context:
    result = await session.execute(
        select(Context).where(Context.user_id == user.id, Context.slug == slug)
    )
    ctx = result.scalar_one_or_none()
    if ctx is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="context not found")
    return ctx


async def _load_todo(session: AsyncSession, user: User, todo_id: UUID) -> Todo:
    stmt = (
        select(Todo)
        .join(Context, Todo.context_id == Context.id)
        .where(Todo.id == todo_id, Context.user_id == user.id)
    )
    todo = (await session.execute(stmt)).scalar_one_or_none()
    if todo is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="todo not found")
    return todo


def _to_http_date(dt: datetime) -> str:
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=UTC)
    return format_datetime(dt, usegmt=True)


@router.get("/contexts/{slug}/todos", response_model=list[TodoOut])
async def list_todos(
    slug: str,
    include_done: bool = Query(default=True),
    include_deleted: bool = Query(default=False),
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> list[Todo]:
    ctx = await _load_context(session, user, slug)
    stmt = select(Todo).where(Todo.context_id == ctx.id).order_by(Todo.created_at)
    if not include_done:
        stmt = stmt.where(Todo.done.is_(False))
    if not include_deleted:
        stmt = stmt.where(Todo.deleted_at.is_(None))
    return list((await session.execute(stmt)).scalars())


@router.post("/contexts/{slug}/todos", response_model=TodoOut, status_code=status.HTTP_201_CREATED)
async def create_todo(
    slug: str,
    body: TodoCreate,
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> Todo:
    ctx = await _load_context(session, user, slug)
    todo = Todo(context_id=ctx.id, title=body.title, description=body.description)
    session.add(todo)
    await session.flush()
    await record_event(
        session,
        user_id=user.id,
        context_id=ctx.id,
        entity_type="todo",
        entity_id=todo.id,
        kind=EventKind.TODO_CREATED,
        payload={"title": todo.title},
    )
    await session.commit()
    await session.refresh(todo)
    return todo


@router.get("/todos/{todo_id}", response_model=TodoOut)
async def get_todo(
    todo_id: UUID,
    response: Response,
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> Todo:
    todo = await _load_todo(session, user, todo_id)
    response.headers["Last-Modified"] = _to_http_date(todo.updated_at)
    return todo


@router.patch("/todos/{todo_id}", response_model=TodoOut)
async def update_todo(
    todo_id: UUID,
    body: TodoUpdate,
    response: Response,
    if_match: str | None = Header(default=None, alias="If-Match"),
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> Todo:
    todo = await _load_todo(session, user, todo_id)

    if if_match is not None:
        try:
            expected = parsedate_to_datetime(if_match)
        except (TypeError, ValueError) as e:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST, detail="bad If-Match header"
            ) from e
        current = todo.updated_at
        if current.tzinfo is None:
            current = current.replace(tzinfo=UTC)
        if abs((current - expected).total_seconds()) > 1:
            raise HTTPException(
                status_code=status.HTTP_412_PRECONDITION_FAILED,
                detail="todo changed elsewhere",
            )

    now = datetime.now(UTC)
    events = []
    if body.context_slug is not None:
        new_ctx = await _load_context(session, user, body.context_slug)
        if new_ctx.id != todo.context_id:
            todo.context_id = new_ctx.id
            todo.updated_at = now
            events.append((todo.context_id, EventKind.TODO_MOVED, {"to_slug": body.context_slug}))
    if body.title is not None and body.title != todo.title:
        todo.title = body.title
        todo.updated_at = now
        events.append((todo.context_id, EventKind.TODO_RENAMED, {"title": todo.title}))
    if body.done is not None and body.done != todo.done:
        todo.done = body.done
        todo.updated_at = now
        events.append((todo.context_id, EventKind.TODO_TOGGLED, {"done": todo.done}))
    if body.description is not None and body.description != todo.description:
        todo.description = body.description
        todo.updated_at = now
        events.append(
            (
                todo.context_id,
                EventKind.TODO_DESCRIPTION_EDITED,
                {"length": len(todo.description)},
            )
        )

    for context_id, kind, payload in events:
        await record_event(
            session,
            user_id=user.id,
            context_id=context_id,
            entity_type="todo",
            entity_id=todo.id,
            kind=kind,
            payload=payload,
        )
    await session.commit()
    await session.refresh(todo)
    response.headers["Last-Modified"] = _to_http_date(todo.updated_at)
    return todo


@router.delete("/todos/{todo_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_todo(
    todo_id: UUID,
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> None:
    todo = await _load_todo(session, user, todo_id)
    if todo.deleted_at is None:
        todo.deleted_at = datetime.now(UTC)
        todo.updated_at = todo.deleted_at  # ty: ignore[invalid-assignment]
        await record_event(
            session,
            user_id=user.id,
            context_id=todo.context_id,
            entity_type="todo",
            entity_id=todo.id,
            kind=EventKind.TODO_DELETED,
            payload={},
        )
    await session.commit()
