from __future__ import annotations

import sqlite3
from typing import TYPE_CHECKING

from jam_maestro.session_store import QuerySessionStoreRequest, query_session_store

if TYPE_CHECKING:
    from pathlib import Path


def test_query_session_store_returns_fts_hits(tmp_path: Path) -> None:
    db = tmp_path / "session-store.db"
    _create_session_store(db)

    result = query_session_store(QuerySessionStoreRequest(query="CodeRabbit"), db_path=db)

    assert len(result.hits) == 1
    assert result.hits[0].session_id == "session-1"
    assert "CodeRabbit" in result.hits[0].content


def test_query_session_store_returns_empty_for_missing_db(tmp_path: Path) -> None:
    result = query_session_store(
        QuerySessionStoreRequest(query="anything"),
        db_path=tmp_path / "missing.db",
    )

    assert result.hits == []


def _create_session_store(path: Path) -> None:
    with sqlite3.connect(path) as conn:
        conn.executescript(
            """
            CREATE TABLE messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata_json TEXT
            );
            CREATE VIRTUAL TABLE messages_fts
            USING fts5(content, content='messages', content_rowid='id');
            CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
                INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
            END;
            """
        )
        conn.execute(
            """
            INSERT INTO messages(session_id, timestamp, role, content, metadata_json)
            VALUES (?, ?, ?, ?, ?)
            """,
            (
                "session-1",
                "2026-05-06T01:00:00Z",
                "assistant",
                "Handled a CodeRabbit comment about ECS extraction.",
                "{}",
            ),
        )
