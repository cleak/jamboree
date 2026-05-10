from __future__ import annotations

import pytest

from jam_maestro.nats_rpc import NatsJsonMessage
from jam_maestro.wake import WakeEventError, task_wake_from_message


def test_task_wake_parses_journal_envelope() -> None:
    wake = task_wake_from_message(
        NatsJsonMessage(
            subject="journal.task.requested",
            headers={"Trace-Id": "01HXKJ00000000000000000000"},
            payload={
                "event_type": "task.requested",
                "trace_id": "01HXKJ00000000000000000000",
                "payload": {
                    "task_id": "task-1",
                    "description": "Do the thing",
                    "project": "blueberry",
                    "task_class": "light-edit",
                    "priority": "normal",
                    "requested_by": "human:caleb",
                },
            },
        )
    )

    assert wake.task_id == "task-1"
    assert wake.trace_id == "01HXKJ00000000000000000000"
    assert wake.task_class == "light-edit"


def test_task_wake_refuses_missing_trace() -> None:
    with pytest.raises(WakeEventError):
        task_wake_from_message(
            NatsJsonMessage(
                subject="journal.task.requested",
                headers={},
                payload={"task_id": "task-1", "description": "Do the thing"},
            )
        )
