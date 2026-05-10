"""Read-only queries over the derived SQLite session store."""

from __future__ import annotations

import os
import sqlite3
from pathlib import Path

from pydantic import Field

from jam_maestro.models import StrictBaseModel
from jam_maestro.paths import jam_home


class QuerySessionStoreRequest(StrictBaseModel):
    """Inputs for `query-session-store`."""

    query: str = Field(min_length=1, max_length=500)
    limit: int = Field(default=10, ge=1, le=50)


class SessionStoreHit(StrictBaseModel):
    """One FTS-backed session-store hit."""

    session_id: str
    timestamp: str
    role: str
    content: str
    rank: float


class QuerySessionStoreResult(StrictBaseModel):
    """Session-store query result."""

    hits: list[SessionStoreHit]


def query_session_store(
    request: QuerySessionStoreRequest,
    *,
    db_path: Path | None = None,
) -> QuerySessionStoreResult:
    """Query the FTS5 session-store view."""
    path = db_path or _default_db_path()
    if not path.exists():
        return QuerySessionStoreResult(hits=[])
    with sqlite3.connect(path) as conn:
        conn.row_factory = sqlite3.Row
        rows = conn.execute(
            """
            SELECT
                messages.session_id,
                messages.timestamp,
                messages.role,
                messages.content,
                bm25(messages_fts) AS rank
            FROM messages_fts
            JOIN messages ON messages_fts.rowid = messages.id
            WHERE messages_fts MATCH ?
            ORDER BY rank
            LIMIT ?
            """,
            (request.query, request.limit),
        ).fetchall()
    return QuerySessionStoreResult(
        hits=[
            SessionStoreHit(
                session_id=str(row["session_id"]),
                timestamp=str(row["timestamp"]),
                role=str(row["role"]),
                content=str(row["content"]),
                rank=float(row["rank"]),
            )
            for row in rows
        ]
    )


def _default_db_path() -> Path:
    if raw := os.environ.get("JAM_SESSION_STORE_DB"):
        return Path(raw)
    return jam_home() / "session-store.db"
