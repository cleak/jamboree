"""Routing manifest lookup for Maestro tool-call subjects."""

from __future__ import annotations

import base64
import json
from datetime import datetime  # noqa: TC003
from typing import Protocol, Self, cast

from pydantic import Field

from jam_maestro.models import StrictBaseModel
from jam_maestro.nats_rpc import NatsJsonMessage, request_json

ROUTING_MANIFEST_BUCKET = "routing-manifest"
ROUTING_MANIFEST_KEY = "current"
ROUTING_MANIFEST_UPDATED_SUBJECT = "routing-manifest.updated"
ROUTING_MANIFEST_SCHEMA_VERSION = 1
JETSTREAM_NOT_FOUND_CODE = 404


class RoutingService(StrictBaseModel):
    """One service entry in the routing manifest."""

    current_version: str = Field(min_length=1)
    subject_prefix: str = Field(min_length=1)
    binary_path: str = Field(min_length=1)
    binary_sha256: str = Field(min_length=1)
    started_at: datetime
    expected_health: str = Field(min_length=1)


class RoutingManifest(StrictBaseModel):
    """Routing manifest JSON stored in the NATS KV bucket."""

    schema_version: int = Field(ge=1)
    updated_at: datetime
    updated_by: str = Field(min_length=1)
    trace_id: str = Field(min_length=1)
    services: dict[str, RoutingService] = Field(default_factory=dict)
    previous_manifest_id: str | None = None

    def subject_for(self, service: str, method: str) -> str | None:
        """Return the routed NATS subject for `service.method`."""
        route = self.services.get(service)
        method = method.strip()
        if route is None or not method:
            return None
        return f"{route.subject_prefix.rstrip('.')}.{method}"


class RoutingManifestSource(Protocol):
    """Source that can load the current routing manifest."""

    async def load(self, trace_id: str) -> RoutingManifest | None:
        """Load the current routing manifest."""
        ...


class StaticRoutingManifestSource:
    """In-memory manifest source for tests and probes."""

    def __init__(self, manifest: RoutingManifest | None = None) -> None:
        self.manifest = manifest

    async def load(self, trace_id: str) -> RoutingManifest | None:
        """Return the configured manifest."""
        _ = trace_id
        return self.manifest


class NatsRoutingManifestSource:
    """NATS JetStream KV-backed routing manifest source."""

    def __init__(self, nats_url: str = "nats://127.0.0.1:4222") -> None:
        self._nats_url = nats_url

    async def load(self, trace_id: str) -> RoutingManifest | None:
        """Load `routing-manifest/current` from JetStream KV."""
        raw = await read_kv_json(
            nats_url=self._nats_url,
            bucket=ROUTING_MANIFEST_BUCKET,
            key=ROUTING_MANIFEST_KEY,
            trace_id=trace_id,
        )
        if raw is None:
            return None
        return RoutingManifest.model_validate(raw)


class RoutingManifestRouter:
    """Cached route resolver that falls back to conventional subjects."""

    def __init__(self, source: RoutingManifestSource | None = None) -> None:
        self._source = source
        self._manifest: RoutingManifest | None = None
        self._stale = source is not None

    async def subject_for(self, service: str, method: str, trace_id: str) -> str:
        """Resolve the current subject for a service method."""
        if self._source is not None and self._stale:
            await self.refresh(trace_id)
        routed = self._manifest.subject_for(service, method) if self._manifest else None
        return routed or f"tool.{service}.{method}"

    async def refresh(self, trace_id: str) -> None:
        """Reload the manifest from the configured source."""
        if self._source is None:
            return
        self._manifest = await self._source.load(trace_id)
        self._stale = False

    async def reload_on_update(self, message: NatsJsonMessage) -> bool:
        """Reload after a `routing-manifest.updated` event."""
        if message.subject != ROUTING_MANIFEST_UPDATED_SUBJECT:
            return False
        trace_id = message.headers.get("Trace-Id") or _trace_from_update_payload(message.payload)
        await self.refresh(trace_id)
        return True

    @classmethod
    def with_manifest(cls, manifest: RoutingManifest | None) -> Self:
        """Create a router backed by a static in-memory manifest."""
        return cls(StaticRoutingManifestSource(manifest))


async def read_kv_json(
    *,
    nats_url: str,
    bucket: str,
    key: str,
    trace_id: str,
) -> dict[str, object] | None:
    """Read a JSON object from a NATS JetStream KV bucket through the API."""
    response = await request_json(
        nats_url=nats_url,
        subject=f"$JS.API.STREAM.MSG.GET.KV_{bucket}",
        payload={"last_by_subj": f"$KV.{bucket}.{key}"},
        trace_id=trace_id,
    )
    if _is_not_found_response(response):
        return None
    message_obj = response.get("message")
    if not isinstance(message_obj, dict):
        error = f"JetStream KV response missing message for {bucket}/{key}"
        raise TypeError(error)
    message = cast("dict[str, object]", message_obj)
    data = message.get("data")
    if not isinstance(data, str):
        error = f"JetStream KV response missing base64 data for {bucket}/{key}"
        raise TypeError(error)
    parsed = json.loads(base64.b64decode(data).decode())
    if not isinstance(parsed, dict):
        error = f"JetStream KV value is not a JSON object for {bucket}/{key}"
        raise TypeError(error)
    return cast("dict[str, object]", parsed)


def _is_not_found_response(response: dict[str, object]) -> bool:
    error_obj = response.get("error")
    if not isinstance(error_obj, dict):
        return False
    error = cast("dict[str, object]", error_obj)
    return error.get("code") == JETSTREAM_NOT_FOUND_CODE


def _trace_from_update_payload(payload: dict[str, object]) -> str:
    trace = payload.get("trace_id")
    if isinstance(trace, str):
        return trace
    return "01HXKJ00000000000000000000"
