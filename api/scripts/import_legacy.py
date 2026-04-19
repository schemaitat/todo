#!/usr/bin/env python3
"""One-shot migration of legacy todo-tui JSON data into the API database.

Legacy layout (~/Library/Application Support/todo-tui/):
  data.json   - {"todos": [...], "notes": [...]}
  events.jsonl - one JSON object per line, {"ts": "...", "kind": {"type": "...", "id": "...", ...}}

Usage:
  python scripts/import_legacy.py [--dry-run] [--data-json PATH] [--events-jsonl PATH]
                                   [--email EMAIL] [--context inbox]
"""

from __future__ import annotations

import argparse
import asyncio
import json
import sys
from datetime import UTC, datetime
from pathlib import Path
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker, create_async_engine

# Resolve default legacy data path (macOS: ~/Library/Application Support/todo-tui)
_DEFAULT_DATA_DIR = Path.home() / "Library" / "Application Support" / "todo-tui"


def _parse_dt(s: str) -> datetime:
    dt = datetime.fromisoformat(s.replace("Z", "+00:00"))
    if dt.tzinfo is None:
        return dt.replace(tzinfo=UTC)
    return dt


def _load_data(path: Path) -> dict:
    raw = path.read_text()
    return json.loads(raw)


def _load_events(path: Path) -> list[dict]:
    events = []
    for line in path.read_text().splitlines():
        line = line.strip()
        if line:
            events.append(json.loads(line))
    return events


def _entity_type(kind_name: str) -> str:
    if kind_name.startswith("Todo"):
        return "todo"
    if kind_name.startswith("Note"):
        return "note"
    return "unknown"


def _map_event(raw: dict) -> dict:
    """Convert a legacy event dict to (ts, entity_type, entity_id, kind, payload)."""
    ts = _parse_dt(raw["ts"])
    kind_obj: dict = raw["kind"]
    kind_name: str = kind_obj["type"]
    entity_id = UUID(kind_obj["id"])
    entity_type = _entity_type(kind_name)

    payload = {k: v for k, v in kind_obj.items() if k not in ("type", "id")}

    # NoteEdited: legacy stored full body; new schema stores length only
    if kind_name == "NoteEdited" and "body" in payload:
        payload = {"length": len(payload["body"])}

    return {
        "ts": ts,
        "entity_type": entity_type,
        "entity_id": entity_id,
        "kind": kind_name,
        "payload": payload,
    }


async def _run(
    database_url: str,
    data_path: Path,
    events_path: Path | None,
    user_email: str,
    context_slug: str,
    dry_run: bool,
) -> None:
    from app.models import Context, Event, Note, Todo, User

    data = _load_data(data_path)
    raw_events = _load_events(events_path) if events_path and events_path.exists() else []

    todos_in = data.get("todos", [])
    notes_in = data.get("notes", [])

    print(f"Found {len(todos_in)} todos, {len(notes_in)} notes, {len(raw_events)} events")

    if dry_run:
        print("[dry-run] no changes written")
        return

    engine = create_async_engine(database_url, future=True)
    factory = async_sessionmaker(engine, expire_on_commit=False)

    async with factory() as session:
        session: AsyncSession

        # Resolve user
        user = (
            await session.execute(select(User).where(User.email == user_email))
        ).scalar_one_or_none()
        if user is None:
            print(
                f"ERROR: no user with email {user_email!r} found — run the API first to bootstrap",
                file=sys.stderr,
            )
            sys.exit(1)

        # Resolve context
        ctx = (
            await session.execute(
                select(Context).where(Context.user_id == user.id, Context.slug == context_slug)
            )
        ).scalar_one_or_none()
        if ctx is None:
            print(
                f"ERROR: context {context_slug!r} not found for user {user_email!r}",
                file=sys.stderr,
            )
            sys.exit(1)

        print(f"Importing into context '{ctx.slug}' (id={ctx.id}) for user {user.email}")

        # Insert todos
        todo_count = 0
        for t in todos_in:
            created = _parse_dt(t["created_at"])
            todo = Todo(
                id=UUID(t["id"]),
                context_id=ctx.id,
                title=t["title"],
                done=t["done"],
                created_at=created,
                updated_at=created,  # legacy had no updated_at
                deleted_at=_parse_dt(t["deleted_at"]) if t.get("deleted_at") else None,
            )
            session.add(todo)
            todo_count += 1

        # Insert notes
        note_count = 0
        for n in notes_in:
            note = Note(
                id=UUID(n["id"]),
                context_id=ctx.id,
                title=n["title"],
                body=n.get("body", ""),
                created_at=_parse_dt(n["created_at"]),
                updated_at=_parse_dt(n["updated_at"]),
                deleted_at=_parse_dt(n["deleted_at"]) if n.get("deleted_at") else None,
            )
            session.add(note)
            note_count += 1

        # Insert events
        event_count = 0
        for raw in raw_events:
            try:
                ev = _map_event(raw)
            except Exception as exc:
                print(f"  skipping malformed event {raw}: {exc}")
                continue
            session.add(
                Event(
                    user_id=user.id,
                    context_id=ctx.id,
                    entity_type=ev["entity_type"],
                    entity_id=ev["entity_id"],
                    kind=ev["kind"],
                    payload=ev["payload"],
                    ts=ev["ts"],
                )
            )
            event_count += 1

        await session.commit()

    await engine.dispose()

    print(f"Imported: {todo_count} todos, {note_count} notes, {event_count} events")

    # Archive source files
    today = datetime.now().strftime("%Y-%m-%d")
    archived = data_path.with_suffix(f".json.imported-{today}")
    data_path.rename(archived)
    print(f"Archived data file -> {archived.name}")

    if events_path and events_path.exists():
        archived_ev = events_path.with_suffix(f".jsonl.imported-{today}")
        events_path.rename(archived_ev)
        print(f"Archived events file -> {archived_ev.name}")


def main() -> None:
    parser = argparse.ArgumentParser(description="Import legacy todo-tui JSON data into the API DB")
    parser.add_argument("--data-json", type=Path, default=_DEFAULT_DATA_DIR / "data.json")
    parser.add_argument("--events-jsonl", type=Path, default=_DEFAULT_DATA_DIR / "events.jsonl")
    parser.add_argument("--database-url", default=None, help="Overrides DATABASE_URL env var")
    parser.add_argument(
        "--email", default="a.schemaitat@gmail.com", help="User email to import into"
    )
    parser.add_argument("--context", default="inbox", help="Target context slug")
    parser.add_argument("--dry-run", action="store_true", help="Print counts without writing")
    args = parser.parse_args()

    if not args.data_json.exists():
        print(f"ERROR: data file not found: {args.data_json}", file=sys.stderr)
        sys.exit(1)

    database_url = args.database_url
    if database_url is None:
        import os

        database_url = os.environ.get(
            "DATABASE_URL", "postgresql+asyncpg://todo:todo@localhost:5432/todo"
        )

    asyncio.run(
        _run(
            database_url=database_url,
            data_path=args.data_json,
            events_path=args.events_jsonl,
            user_email=args.email,
            context_slug=args.context,
            dry_run=args.dry_run,
        )
    )


if __name__ == "__main__":
    main()
