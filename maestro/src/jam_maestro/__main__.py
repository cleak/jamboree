"""Minimal live probe for the Maestro backend skeleton."""

from __future__ import annotations

import argparse
import asyncio
import os
import sys
from dataclasses import dataclass
from pathlib import Path

from jam_maestro.backend import LiteLLMBackend
from jam_maestro.input_budget import load_input_budget_config
from jam_maestro.models import MaestroRequest, Message
from jam_maestro.routing_manifest import NatsRoutingManifestSource, RoutingManifestRouter
from jam_maestro.session import (
    MaestroSessionLoop,
    NatsObserveClient,
    NatsSessionClient,
    NatsTaskEventPublisher,
    SessionDecision,
)
from jam_maestro.skills import FileSkillLoader
from jam_maestro.tempyr_journal import CliTempyrJournal
from jam_maestro.trace import new_trace_id
from jam_maestro.wake import TaskWake, next_task_wake, subscribe_task_wakes


@dataclass(frozen=True)
class _RunTaskOptions:
    task_id: str
    description: str
    project: str
    task_class: str | None
    priority: str
    requested_by: str
    nats_url: str
    trace_id: str | None
    tempyr_worktree: str | None
    skills_root: str | None


def main() -> None:
    parser = argparse.ArgumentParser(description="Run Maestro development probes.")
    subcommands = parser.add_subparsers(dest="command")

    prompt = subcommands.add_parser("prompt", help="Run a single backend prompt.")
    prompt.add_argument("prompt", nargs="?", default="Reply with: pong")
    prompt.add_argument("--model", default=os.environ.get("JAM_MAESTRO_MODEL", "gpt-5.5"))
    prompt.add_argument(
        "--trace-id",
        default=os.environ.get("JAM_TRACE_ID", "01HXKJ00000000000000000000"),
    )

    snapshot = subcommands.add_parser("world-snapshot", help="Call jam-svc-observe.")
    snapshot.add_argument("task_id")
    snapshot.add_argument("--nats-url", default=os.environ.get("NATS_URL", "nats://127.0.0.1:4222"))
    snapshot.add_argument("--trace-id", default=os.environ.get("JAM_TRACE_ID"))

    run_task = subcommands.add_parser("run-task", help="Run one Maestro task wake directly.")
    run_task.add_argument("task_id")
    run_task.add_argument("--description", default="manual Maestro task wake")
    run_task.add_argument("--project", default="blueberry")
    run_task.add_argument("--task-class")
    run_task.add_argument("--priority", default="normal")
    run_task.add_argument("--requested-by", default="human:caleb")
    run_task.add_argument("--nats-url", default=os.environ.get("NATS_URL", "nats://127.0.0.1:4222"))
    run_task.add_argument("--trace-id", default=os.environ.get("JAM_TRACE_ID"))
    run_task.add_argument("--tempyr-worktree", default=os.environ.get("JAM_TEMPYR_WORKTREE"))
    run_task.add_argument("--skills-root", default=os.environ.get("JAM_SKILLS_ROOT"))

    wake_once = subcommands.add_parser(
        "wake-once",
        help="Wait for one journal.task.requested event and handle it.",
    )
    wake_once.add_argument(
        "--nats-url", default=os.environ.get("NATS_URL", "nats://127.0.0.1:4222")
    )
    wake_once.add_argument("--timeout-secs", type=float, default=30.0)
    wake_once.add_argument("--tempyr-worktree", default=os.environ.get("JAM_TEMPYR_WORKTREE"))
    wake_once.add_argument("--skills-root", default=os.environ.get("JAM_SKILLS_ROOT"))

    listen = subcommands.add_parser("listen", help="Run the Maestro wake loop.")
    listen.add_argument("--nats-url", default=os.environ.get("NATS_URL", "nats://127.0.0.1:4222"))
    listen.add_argument("--timeout-secs", type=float, default=30.0)
    listen.add_argument("--tempyr-worktree", default=os.environ.get("JAM_TEMPYR_WORKTREE"))
    listen.add_argument("--skills-root", default=os.environ.get("JAM_SKILLS_ROOT"))

    args = parser.parse_args()
    if args.command == "world-snapshot":
        response = asyncio.run(_world_snapshot(args.task_id, args.nats_url, args.trace_id))
        sys.stdout.write(f"{response.model_dump_json(indent=2)}\n")
        return
    if args.command == "run-task":
        response = asyncio.run(
            _run_task(
                _RunTaskOptions(
                    task_id=args.task_id,
                    description=args.description,
                    project=args.project,
                    task_class=args.task_class,
                    priority=args.priority,
                    requested_by=args.requested_by,
                    nats_url=args.nats_url,
                    trace_id=args.trace_id,
                    tempyr_worktree=args.tempyr_worktree,
                    skills_root=args.skills_root,
                )
            )
        )
        sys.stdout.write(f"{response.model_dump_json(indent=2)}\n")
        return
    if args.command == "wake-once":
        response = asyncio.run(
            _wake_once(
                nats_url=args.nats_url,
                timeout_secs=args.timeout_secs,
                tempyr_worktree=args.tempyr_worktree,
                skills_root=args.skills_root,
            )
        )
        sys.stdout.write(f"{response.model_dump_json(indent=2)}\n")
        return
    if args.command == "listen":
        asyncio.run(
            _listen(
                nats_url=args.nats_url,
                timeout_secs=args.timeout_secs,
                tempyr_worktree=args.tempyr_worktree,
                skills_root=args.skills_root,
            )
        )
        return

    prompt_text = getattr(args, "prompt", "Reply with: pong")
    model = getattr(args, "model", os.environ.get("JAM_MAESTRO_MODEL", "gpt-5.5"))
    trace_id = getattr(args, "trace_id", "01HXKJ00000000000000000000")
    backend = LiteLLMBackend(model=model)
    response = backend.respond(
        MaestroRequest(
            messages=[Message(role="user", content=prompt_text)],
            reasoning_effort="medium",
            budget_usd=0.25,
            trace_id=trace_id,
        )
    )
    sys.stdout.write(f"{response.model_dump_json(indent=2)}\n")


async def _world_snapshot(task_id: str, nats_url: str, trace_id: str | None) -> SessionDecision:
    observe_routing = RoutingManifestRouter(NatsRoutingManifestSource(nats_url))
    loop = MaestroSessionLoop(NatsObserveClient(nats_url, routing=observe_routing))
    return await loop.run_task_wake(task_id, trace_id)


async def _run_task(options: _RunTaskOptions) -> SessionDecision:
    wake = TaskWake(
        trace_id=options.trace_id or os.environ.get("JAM_TRACE_ID") or new_trace_id(),
        task_id=options.task_id,
        description=options.description,
        project=options.project,
        task_class=options.task_class,
        priority=options.priority,
        requested_by=options.requested_by,
    )
    loop = _runtime_loop(options.nats_url, options.tempyr_worktree, options.skills_root)
    return await loop.run_task_wake(wake)


async def _wake_once(
    *,
    nats_url: str,
    timeout_secs: float,
    tempyr_worktree: str | None,
    skills_root: str | None,
) -> SessionDecision:
    wake = await next_task_wake(nats_url=nats_url, timeout_secs=timeout_secs)
    loop = _runtime_loop(nats_url, tempyr_worktree, skills_root)
    return await loop.run_task_wake(wake)


async def _listen(
    *,
    nats_url: str,
    timeout_secs: float,
    tempyr_worktree: str | None,
    skills_root: str | None,
) -> None:
    # Build the routers up-front so we can also pass them to a background
    # routing-manifest watcher. Sharing one router per client keeps caches in
    # sync across all tasks.
    observe_routing = RoutingManifestRouter(NatsRoutingManifestSource(nats_url))
    session_routing = RoutingManifestRouter(NatsRoutingManifestSource(nats_url))
    loop = _runtime_loop(
        nats_url, tempyr_worktree, skills_root, observe_routing, session_routing
    )
    refresher = asyncio.create_task(
        _refresh_routing_on_manifest_updates(
            nats_url=nats_url,
            routers=(observe_routing, session_routing),
        )
    )
    try:
        async for wake in subscribe_task_wakes(nats_url=nats_url, timeout_secs=timeout_secs):
            try:
                decision = await loop.run_task_wake(wake)
                sys.stdout.write(f"{decision.model_dump_json()}\n")
                sys.stdout.flush()
            except Exception as err:  # pragma: no cover - resilience guard
                # A single task wake failing must not kill the whole listener.
                # The original implementation let the exception propagate,
                # which crashed `process-compose process maestro` whenever a
                # downstream RPC timed out (e.g. stale routing cache after a
                # session redeploy). Logging + continuing keeps the orchestrator
                # responsive; the task itself surfaces the error via
                # task.failed in the journal.
                sys.stderr.write(
                    f"task wake handler failed: {type(err).__name__}: {err}\n"
                )
                sys.stderr.flush()
    finally:
        refresher.cancel()


async def _refresh_routing_on_manifest_updates(
    *,
    nats_url: str,
    routers: tuple[RoutingManifestRouter, ...],
) -> None:
    """Subscribe to `routing-manifest.updated` and refresh every router on
    each update. Without this, the maestro loop keeps publishing to the
    previous version's subject after `jam deploy` swaps the manifest, and
    every downstream call gets `no responders`."""
    from jam_maestro.nats_rpc import subscribe_json

    async for _msg in subscribe_json(
        nats_url=nats_url,
        subject="routing-manifest.updated",
    ):
        for router in routers:
            try:
                await router.refresh("00000000000000000000000000")
            except Exception as err:  # pragma: no cover - resilience guard
                sys.stderr.write(
                    f"routing manifest refresh failed: {type(err).__name__}: {err}\n"
                )
                sys.stderr.flush()


def _runtime_loop(
    nats_url: str,
    tempyr_worktree: str | None,
    skills_root: str | None,
    observe_routing: RoutingManifestRouter | None = None,
    session_routing: RoutingManifestRouter | None = None,
) -> MaestroSessionLoop:
    if observe_routing is None:
        observe_routing = RoutingManifestRouter(NatsRoutingManifestSource(nats_url))
    if session_routing is None:
        session_routing = RoutingManifestRouter(NatsRoutingManifestSource(nats_url))
    worktree = Path(tempyr_worktree) if tempyr_worktree else None
    root = Path(skills_root) if skills_root else None
    return MaestroSessionLoop(
        NatsObserveClient(nats_url, routing=observe_routing),
        skills=FileSkillLoader(default_root=root),
        session=NatsSessionClient(nats_url, routing=session_routing),
        task_events=NatsTaskEventPublisher(nats_url),
        journal=CliTempyrJournal(worktree=worktree),
        input_budget=load_input_budget_config(),
    )


if __name__ == "__main__":
    main()
