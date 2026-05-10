"""Wake-event parsing and NATS subscription for the Maestro."""

from __future__ import annotations

from collections.abc import AsyncIterator, Mapping
from typing import cast

from pydantic import Field

from jam_maestro.models import StrictBaseModel
from jam_maestro.nats_rpc import NatsJsonMessage, next_json_message, subscribe_json
from jam_maestro.trace import is_trace_id

TASK_REQUESTED_SUBJECT = "journal.task.requested"
TRACE_ID_HEADER = "Trace-Id"
PARENT_TRACE_ID_HEADER = "Parent-Trace-Id"


class WakeEventError(ValueError):
    """Raised when a wake event is malformed."""


class TaskWake(StrictBaseModel):
    """A parsed `journal.task.requested` wake event."""

    subject: str = TASK_REQUESTED_SUBJECT
    trace_id: str = Field(min_length=26, max_length=26)
    parent_trace_id: str | None = Field(default=None, min_length=26, max_length=26)
    task_id: str = Field(min_length=1)
    description: str = Field(min_length=1)
    project: str = Field(default="blueberry", min_length=1)
    task_class: str | None = Field(default=None, min_length=1)
    priority: str = Field(default="normal", min_length=1)
    requested_by: str = Field(default="unknown", min_length=1)


async def next_task_wake(
    *,
    nats_url: str,
    timeout_secs: float = 30.0,
) -> TaskWake:
    """Wait for the next task-requested wake event."""
    message = await next_json_message(
        nats_url=nats_url,
        subject=TASK_REQUESTED_SUBJECT,
        timeout_secs=timeout_secs,
    )
    return task_wake_from_message(message)


async def subscribe_task_wakes(
    *,
    nats_url: str,
    timeout_secs: float = 30.0,
) -> AsyncIterator[TaskWake]:
    """Yield task wake events from the Jamboree bus."""
    async for message in subscribe_json(
        nats_url=nats_url,
        subject=TASK_REQUESTED_SUBJECT,
        timeout_secs=timeout_secs,
    ):
        yield task_wake_from_message(message)


def task_wake_from_message(message: NatsJsonMessage) -> TaskWake:
    """Parse a NATS JSON message into a `TaskWake`."""
    envelope = message.payload
    payload = _payload_from_envelope(envelope)
    trace_id = _trace_from_message(message, envelope)
    parent_trace_id = _optional_trace(
        message.headers.get(PARENT_TRACE_ID_HEADER)
        or _optional_str(envelope.get("parent_trace_id"))
    )

    task_id = _required_str(payload, "task_id")
    description = _required_str(payload, "description")
    project = _optional_str(payload.get("project")) or "blueberry"
    task_class = _optional_str(payload.get("task_class"))
    priority = _optional_str(payload.get("priority")) or "normal"
    requested_by = _optional_str(payload.get("requested_by")) or "unknown"

    return TaskWake(
        subject=message.subject,
        trace_id=trace_id,
        parent_trace_id=parent_trace_id,
        task_id=task_id,
        description=description,
        project=project,
        task_class=task_class,
        priority=priority,
        requested_by=requested_by,
    )


def _payload_from_envelope(envelope: dict[str, object]) -> Mapping[str, object]:
    payload = envelope.get("payload")
    if isinstance(payload, Mapping):
        return cast("Mapping[str, object]", payload)
    return envelope


def _trace_from_message(message: NatsJsonMessage, envelope: dict[str, object]) -> str:
    trace = message.headers.get(TRACE_ID_HEADER) or _optional_str(envelope.get("trace_id"))
    if trace and is_trace_id(trace):
        return trace
    error = "journal.task.requested wake is missing a valid trace_id"
    raise WakeEventError(error)


def _optional_trace(value: str | None) -> str | None:
    if value is None:
        return None
    if is_trace_id(value):
        return value
    return None


def _required_str(payload: Mapping[str, object], key: str) -> str:
    value = payload.get(key)
    if isinstance(value, str) and value:
        return value
    error = f"journal.task.requested payload is missing `{key}`"
    raise WakeEventError(error)


def _optional_str(value: object) -> str | None:
    return value if isinstance(value, str) and value else None
