from __future__ import annotations

import asyncio
import base64
import json
from typing import TYPE_CHECKING

from jam_maestro.nats_rpc import NatsJsonMessage
from jam_maestro.routing_manifest import (
    ROUTING_MANIFEST_UPDATED_SUBJECT,
    RoutingManifest,
    RoutingManifestRouter,
    RoutingService,
    read_kv_json,
)

if TYPE_CHECKING:
    from collections.abc import Sequence

    import pytest

TRACE_ID = "01HXKJ00000000000000000000"


def _manifest(prefix: str) -> RoutingManifest:
    return RoutingManifest.model_validate(
        {
            "schema_version": 1,
            "updated_at": "2026-05-06T12:00:00Z",
            "updated_by": "human:caleb",
            "trace_id": TRACE_ID,
            "services": {
                "observe": RoutingService.model_validate(
                    {
                        "current_version": "0.4.7",
                        "subject_prefix": prefix,
                        "binary_path": "/home/maestro/.jam/bin/jam-svc-observe-0.4.7",
                        "binary_sha256": "abc123",
                        "started_at": "2026-05-06T12:00:00Z",
                        "expected_health": "ok",
                    }
                )
            },
        }
    )


class SequenceSource:
    def __init__(self, manifests: Sequence[RoutingManifest | None]) -> None:
        self._manifests = list(manifests)
        self.loads: list[str] = []

    async def load(self, trace_id: str) -> RoutingManifest | None:
        self.loads.append(trace_id)
        return self._manifests.pop(0)


def test_manifest_resolves_versioned_subject() -> None:
    manifest = _manifest("tool.observe.v047")

    assert manifest.subject_for("observe", "world-snapshot") == ("tool.observe.v047.world-snapshot")
    assert manifest.subject_for("repo", "open-pr") is None


def test_router_falls_back_without_manifest() -> None:
    router = RoutingManifestRouter.with_manifest(None)

    subject = asyncio.run(router.subject_for("observe", "world-snapshot", TRACE_ID))

    assert subject == "tool.observe.world-snapshot"


def test_router_reloads_after_update_event() -> None:
    source = SequenceSource(
        [
            _manifest("tool.observe.v047"),
            _manifest("tool.observe.v048"),
        ]
    )
    router = RoutingManifestRouter(source)

    first = asyncio.run(router.subject_for("observe", "world-snapshot", TRACE_ID))
    reloaded = asyncio.run(
        router.reload_on_update(
            NatsJsonMessage(
                subject=ROUTING_MANIFEST_UPDATED_SUBJECT,
                headers={"Trace-Id": TRACE_ID},
                payload={},
            )
        )
    )
    second = asyncio.run(router.subject_for("observe", "world-snapshot", TRACE_ID))

    assert reloaded
    assert first == "tool.observe.v047.world-snapshot"
    assert second == "tool.observe.v048.world-snapshot"
    assert source.loads == [TRACE_ID, TRACE_ID]


def test_read_kv_json_decodes_jetstream_response(monkeypatch: pytest.MonkeyPatch) -> None:
    async def fake_request_json(**kwargs: object) -> dict[str, object]:
        assert kwargs["subject"] == "$JS.API.STREAM.MSG.GET.KV_routing-manifest"
        body = json.dumps({"schema_version": 1}).encode()
        return {"message": {"data": base64.b64encode(body).decode()}}

    monkeypatch.setattr("jam_maestro.routing_manifest.request_json", fake_request_json)

    parsed = asyncio.run(
        read_kv_json(
            nats_url="nats://127.0.0.1:4222",
            bucket="routing-manifest",
            key="current",
            trace_id=TRACE_ID,
        )
    )

    assert parsed == {"schema_version": 1}


def test_read_kv_json_returns_none_for_not_found(monkeypatch: pytest.MonkeyPatch) -> None:
    async def fake_request_json(**kwargs: object) -> dict[str, object]:
        _ = kwargs
        return {"error": {"code": 404}}

    monkeypatch.setattr("jam_maestro.routing_manifest.request_json", fake_request_json)

    parsed = asyncio.run(
        read_kv_json(
            nats_url="nats://127.0.0.1:4222",
            bucket="routing-manifest",
            key="current",
            trace_id=TRACE_ID,
        )
    )

    assert parsed is None
