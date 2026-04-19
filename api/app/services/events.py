from __future__ import annotations

from typing import Any
from uuid import UUID

from sqlalchemy.ext.asyncio import AsyncSession

from ..models import Event, EventKind


async def record_event(
    session: AsyncSession,
    *,
    user_id: UUID,
    context_id: UUID | None,
    entity_type: str,
    entity_id: UUID | None,
    kind: EventKind,
    payload: dict[str, Any] | None = None,
) -> Event:
    event = Event(
        user_id=user_id,
        context_id=context_id,
        entity_type=entity_type,
        entity_id=entity_id,
        kind=kind.value,
        payload=payload or {},
    )
    session.add(event)
    return event
