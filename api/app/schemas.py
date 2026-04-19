from __future__ import annotations

from datetime import datetime
from typing import Any, Literal
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field


class ORMBase(BaseModel):
    model_config = ConfigDict(from_attributes=True)


class UserOut(ORMBase):
    id: UUID
    email: str
    display_name: str
    created_at: datetime


class ContextOut(ORMBase):
    id: UUID
    slug: str
    name: str
    color: str
    position: int
    created_at: datetime
    archived_at: datetime | None


class ContextCreate(BaseModel):
    slug: str = Field(min_length=1, max_length=64, pattern=r"^[a-z0-9][a-z0-9_-]*$")
    name: str = Field(min_length=1, max_length=255)
    color: str | None = Field(default=None, max_length=16)


class ContextUpdate(BaseModel):
    name: str | None = Field(default=None, min_length=1, max_length=255)
    color: str | None = Field(default=None, max_length=16)
    position: int | None = None
    slug: str | None = Field(default=None, min_length=1, max_length=64, pattern=r"^[a-z0-9][a-z0-9_-]*$")


class TodoOut(ORMBase):
    id: UUID
    context_id: UUID
    title: str
    done: bool
    created_at: datetime
    updated_at: datetime
    deleted_at: datetime | None


class TodoCreate(BaseModel):
    title: str = Field(min_length=1, max_length=1024)


class TodoUpdate(BaseModel):
    title: str | None = Field(default=None, min_length=1, max_length=1024)
    done: bool | None = None
    context_slug: str | None = None


class NoteOut(ORMBase):
    id: UUID
    context_id: UUID
    title: str
    body: str
    created_at: datetime
    updated_at: datetime
    deleted_at: datetime | None


class NoteCreate(BaseModel):
    title: str = Field(min_length=1, max_length=1024)
    body: str = Field(default="")


class NoteUpdate(BaseModel):
    title: str | None = Field(default=None, min_length=1, max_length=1024)
    body: str | None = None
    context_slug: str | None = None


class EventOut(ORMBase):
    id: int
    context_id: UUID | None
    entity_type: str
    entity_id: UUID | None
    kind: str
    payload: dict[str, Any]
    ts: datetime


class SnapshotTodoOut(BaseModel):
    id: UUID
    title: str
    done: bool


class SnapshotNoteOut(BaseModel):
    id: UUID
    title: str
    body: str


class SnapshotJsonOut(BaseModel):
    context: ContextOut
    generated_at: datetime
    open_todos: list[SnapshotTodoOut]
    notes: list[SnapshotNoteOut]


SnapshotFormat = Literal["plain", "html", "json"]


class HealthOut(BaseModel):
    status: str = "ok"


class VersionOut(BaseModel):
    version: str
