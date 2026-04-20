"""baseline schema

Revision ID: 0001
Revises:
Create Date: 2026-04-18 00:00:00.000000

"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op

revision = "0001"
down_revision = None
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "users",
        sa.Column("id", sa.Uuid(), primary_key=True),
        sa.Column("email", sa.String(320), nullable=False, unique=True),
        sa.Column("display_name", sa.String(255), nullable=False),
        sa.Column("external_sub", sa.String(255), nullable=True, unique=True),
        sa.Column(
            "created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()
        ),
        sa.Column("disabled_at", sa.DateTime(timezone=True), nullable=True),
    )

    op.create_table(
        "api_keys",
        sa.Column("id", sa.Uuid(), primary_key=True),
        sa.Column(
            "user_id", sa.Uuid(), sa.ForeignKey("users.id", ondelete="CASCADE"), nullable=False
        ),
        sa.Column("key_hash", sa.String(255), nullable=False, unique=True),
        sa.Column("label", sa.String(255), nullable=False),
        sa.Column(
            "created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()
        ),
        sa.Column("last_used_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("revoked_at", sa.DateTime(timezone=True), nullable=True),
    )
    op.create_index("ix_api_keys_key_hash", "api_keys", ["key_hash"], unique=True)

    op.create_table(
        "contexts",
        sa.Column("id", sa.Uuid(), primary_key=True),
        sa.Column(
            "user_id", sa.Uuid(), sa.ForeignKey("users.id", ondelete="CASCADE"), nullable=False
        ),
        sa.Column("slug", sa.String(64), nullable=False),
        sa.Column("name", sa.String(255), nullable=False),
        sa.Column("color", sa.String(16), nullable=False, server_default="#8888ff"),
        sa.Column("position", sa.Integer(), nullable=False, server_default="0"),
        sa.Column(
            "created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()
        ),
        sa.Column("archived_at", sa.DateTime(timezone=True), nullable=True),
    )
    op.create_index("ix_contexts_user_slug", "contexts", ["user_id", "slug"], unique=True)

    op.create_table(
        "todos",
        sa.Column("id", sa.Uuid(), primary_key=True),
        sa.Column(
            "context_id",
            sa.Uuid(),
            sa.ForeignKey("contexts.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column("title", sa.String(1024), nullable=False),
        sa.Column("done", sa.Boolean(), nullable=False, server_default=sa.false()),
        sa.Column(
            "created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()
        ),
        sa.Column(
            "updated_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()
        ),
        sa.Column("deleted_at", sa.DateTime(timezone=True), nullable=True),
    )
    op.create_index("ix_todos_context_deleted_done", "todos", ["context_id", "deleted_at", "done"])

    op.create_table(
        "notes",
        sa.Column("id", sa.Uuid(), primary_key=True),
        sa.Column(
            "context_id",
            sa.Uuid(),
            sa.ForeignKey("contexts.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column("title", sa.String(1024), nullable=False),
        sa.Column("body", sa.Text(), nullable=False, server_default=""),
        sa.Column(
            "created_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()
        ),
        sa.Column(
            "updated_at", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()
        ),
        sa.Column("deleted_at", sa.DateTime(timezone=True), nullable=True),
    )
    op.create_index("ix_notes_context_deleted", "notes", ["context_id", "deleted_at"])

    op.create_table(
        "events",
        sa.Column(
            "id",
            sa.BigInteger().with_variant(sa.Integer(), "sqlite"),
            primary_key=True,
            autoincrement=True,
        ),
        sa.Column(
            "user_id", sa.Uuid(), sa.ForeignKey("users.id", ondelete="CASCADE"), nullable=False
        ),
        sa.Column(
            "context_id",
            sa.Uuid(),
            sa.ForeignKey("contexts.id", ondelete="SET NULL"),
            nullable=True,
        ),
        sa.Column("entity_type", sa.String(32), nullable=False),
        sa.Column("entity_id", sa.Uuid(), nullable=True),
        sa.Column("kind", sa.String(64), nullable=False),
        sa.Column("payload", sa.JSON(), nullable=False),
        sa.Column("ts", sa.DateTime(timezone=True), nullable=False, server_default=sa.func.now()),
    )
    op.create_index("ix_events_user_ts", "events", ["user_id", "ts"])
    op.create_index("ix_events_context_ts", "events", ["context_id", "ts"])


def downgrade() -> None:
    op.drop_index("ix_events_context_ts", table_name="events")
    op.drop_index("ix_events_user_ts", table_name="events")
    op.drop_table("events")
    op.drop_index("ix_notes_context_deleted", table_name="notes")
    op.drop_table("notes")
    op.drop_index("ix_todos_context_deleted_done", table_name="todos")
    op.drop_table("todos")
    op.drop_index("ix_contexts_user_slug", table_name="contexts")
    op.drop_table("contexts")
    op.drop_index("ix_api_keys_key_hash", table_name="api_keys")
    op.drop_table("api_keys")
    op.drop_table("users")
