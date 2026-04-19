from __future__ import annotations

from datetime import datetime, timezone
from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException, Query, status
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
    todo = Todo(context_id=ctx.id, title=body.title)
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


@router.patch("/todos/{todo_id}", response_model=TodoOut)
async def update_todo(
    todo_id: UUID,
    body: TodoUpdate,
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> Todo:
    todo = await _load_todo(session, user, todo_id)
    now = datetime.now(timezone.utc)
    events = []
    if body.title is not None and body.title != todo.title:
        todo.title = body.title
        todo.updated_at = now
        events.append((EventKind.TODO_RENAMED, {"title": todo.title}))
    if body.done is not None and body.done != todo.done:
        todo.done = body.done
        todo.updated_at = now
        events.append((EventKind.TODO_TOGGLED, {"done": todo.done}))

    for kind, payload in events:
        await record_event(
            session,
            user_id=user.id,
            context_id=todo.context_id,
            entity_type="todo",
            entity_id=todo.id,
            kind=kind,
            payload=payload,
        )
    await session.commit()
    await session.refresh(todo)
    return todo


@router.delete("/todos/{todo_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_todo(
    todo_id: UUID,
    user: User = Depends(get_current_user),
    session: AsyncSession = Depends(session_dependency),
) -> None:
    todo = await _load_todo(session, user, todo_id)
    if todo.deleted_at is None:
        todo.deleted_at = datetime.now(timezone.utc)
        todo.updated_at = todo.deleted_at
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
