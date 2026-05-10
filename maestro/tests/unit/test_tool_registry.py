from __future__ import annotations

from datetime import UTC, datetime

import pytest
from pydantic import ValidationError

from jam_maestro.candidates import (
    ProposeToolChangeRequest,
    RecordImprovementCandidateRequest,
    RecordTempyrUpdateCandidateRequest,
)
from jam_maestro.journal_reader import ReadJournalRequest
from jam_maestro.session_store import QuerySessionStoreRequest
from jam_maestro.skills import ReadSkillsRequest
from jam_maestro.tempyr_journal_query import (
    TempyrJournalBlameRequest,
    TempyrJournalRangeRequest,
    TempyrJournalSearchRequest,
)
from jam_maestro.tempyr_query import QueryTempyrRequest
from jam_maestro.tool_registry import (
    DEFAULT_TOOL_ROUTES,
    DELIBERATELY_ABSENT_TOOL_NAMES,
    MaestroToolRegistry,
    NoSuchToolError,
    ToolRoute,
)
from jam_maestro.tools import (
    EvolveRequestSkillEvolutionRequest,
    MessageEnqueueMessageRequest,
    MessageFullStopRequest,
    MessageInterruptWithMessageRequest,
    ObserveClassifyReviewArtifactsRequest,
    ObserveComputeReadinessRequest,
    ObserveListBlockersRequest,
    ObserveListReviewArtifactsRequest,
    ObserveQueryQuotaRequest,
    ObserveRefreshWorldSnapshotRequest,
    ObserveWorldSnapshotDeltaRequest,
    ObserveWorldSnapshotRequest,
    RepoMarkReviewArtifactHandledRequest,
    RepoPrepareMergeRequest,
    RepoReadPrCommentsRequest,
    RepoReplyToCommentRequest,
    RepoRequestHumanMergeRequest,
    RepoRequestReviewRequest,
    ResearchRequestResearchRequest,
    SearchWebCrawlRequest,
    SearchWebExtractRequest,
    SearchWebSearchRequest,
    SessionArchiveSessionRequest,
    SessionListActiveRequest,
    SessionPurgeSessionRequest,
    SuperviseNotifyHumanRequest,
    SupervisePauseDispatchRequest,
    SuperviseResumeDispatchRequest,
    WorktreeFindConflictsRequest,
    WorktreeWorktreeDiffRequest,
)


def test_deliberately_absent_tools_are_not_registered() -> None:
    assert DELIBERATELY_ABSENT_TOOL_NAMES.isdisjoint(DEFAULT_TOOL_ROUTES)


@pytest.mark.parametrize("tool_name", sorted(DELIBERATELY_ABSENT_TOOL_NAMES))
def test_calling_deliberately_absent_tools_returns_no_such_tool(tool_name: str) -> None:
    registry = MaestroToolRegistry()

    with pytest.raises(NoSuchToolError) as exc:
        registry.prepare_request(tool_name, {"task_id": "task-1"})

    assert str(exc.value) == f"no such tool: {tool_name}"


def test_known_tool_prepares_validated_request() -> None:
    registry = MaestroToolRegistry()

    prepared = registry.prepare_request("world-snapshot", {"task_id": "task-1"})

    assert prepared.route.subject == "tool.observe.world-snapshot"
    assert prepared.payload == ObserveWorldSnapshotRequest(task_id="task-1")


def test_observe_snapshot_derivative_tools_prepare_validated_requests() -> None:
    registry = MaestroToolRegistry()

    readiness = registry.prepare_request("compute-readiness", {"task_id": "task-1"})
    blockers = registry.prepare_request("list-blockers", {"task_id": "task-1"})
    refresh = registry.prepare_request(
        "refresh-world-snapshot",
        {"task_id": "task-1", "max_staleness_secs": 0},
    )

    assert readiness.route.subject == "tool.observe.compute-readiness"
    assert readiness.payload == ObserveComputeReadinessRequest(task_id="task-1")
    assert blockers.route.subject == "tool.observe.list-blockers"
    assert blockers.payload == ObserveListBlockersRequest(task_id="task-1")
    assert refresh.route.subject == "tool.observe.refresh-world-snapshot"
    assert refresh.payload == ObserveRefreshWorldSnapshotRequest(
        task_id="task-1",
        max_staleness_secs=0,
    )


def test_world_snapshot_delta_prepares_validated_request() -> None:
    registry = MaestroToolRegistry()

    prepared = registry.prepare_request(
        "world-snapshot-delta",
        {
            "task_id": "task-1",
            "since": "2026-05-06T21:00:00Z",
        },
    )

    assert prepared.route.subject == "tool.observe.world-snapshot-delta"
    assert isinstance(prepared.payload, ObserveWorldSnapshotDeltaRequest)
    assert prepared.payload == ObserveWorldSnapshotDeltaRequest(
        task_id="task-1",
        since=datetime(2026, 5, 6, 21, 0, tzinfo=UTC),
    )


def test_request_skill_evolution_prepares_validated_request() -> None:
    registry = MaestroToolRegistry()

    prepared = registry.prepare_request(
        "request-skill-evolution",
        {
            "skill_name": "blueberry/ecs",
            "eval_source": "/home/maestro/.jam/evals/ecs.jsonl",
            "reason": "skill under suspicion",
        },
    )

    assert prepared.route.subject == "tool.evolve.request-skill-evolution"
    assert prepared.payload == EvolveRequestSkillEvolutionRequest(
        skill_name="blueberry/ecs",
        eval_source="/home/maestro/.jam/evals/ecs.jsonl",
        reason="skill under suspicion",
    )


def test_read_skills_prepares_validated_request() -> None:
    registry = MaestroToolRegistry()

    prepared = registry.prepare_request(
        "read-skills",
        {"scope": "blueberry/task-types/light-edit"},
    )

    assert prepared.route.subject == "meta.read-skills"
    assert prepared.payload == ReadSkillsRequest(scope="blueberry/task-types/light-edit")


def test_read_journal_prepares_validated_request() -> None:
    registry = MaestroToolRegistry()

    prepared = registry.prepare_request(
        "read-journal",
        {"trace_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV", "task_id": "task-1", "limit": 10},
    )

    assert prepared.route.subject == "meta.read-journal"
    assert prepared.payload == ReadJournalRequest(
        trace_id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
        task_id="task-1",
        limit=10,
    )


def test_query_session_store_prepares_validated_request() -> None:
    registry = MaestroToolRegistry()

    prepared = registry.prepare_request(
        "query-session-store",
        {"query": "CodeRabbit ECS", "limit": 5},
    )

    assert prepared.route.subject == "meta.query-session-store"
    assert prepared.payload == QuerySessionStoreRequest(query="CodeRabbit ECS", limit=5)


def test_query_tempyr_prepares_validated_request() -> None:
    registry = MaestroToolRegistry()

    prepared = registry.prepare_request(
        "query-tempyr",
        {"query": "Tempyr graph", "scope": "blueberry", "max_results": 3},
    )

    assert prepared.route.subject == "meta.query-tempyr"
    assert prepared.payload == QueryTempyrRequest(
        query="Tempyr graph",
        scope="blueberry",
        max_results=3,
    )


def test_tempyr_journal_query_tools_prepare_validated_requests() -> None:
    registry = MaestroToolRegistry()

    search = registry.prepare_request(
        "tempyr-journal-search",
        {
            "query": "trace failure",
            "kind": ["dead_end"],
            "agent": "codex",
            "since_days": 7,
            "limit": 5,
        },
    )
    range_query = registry.prepare_request(
        "tempyr-journal-range",
        {"rev_range": "HEAD~3..HEAD", "kind": ["decision"], "limit": 3},
    )
    blame = registry.prepare_request(
        "tempyr-journal-blame",
        {"file_path": "crates/jam-cli/src/main.rs", "limit": 4},
    )

    assert search.route.subject == "meta.tempyr-journal-search"
    assert search.payload == TempyrJournalSearchRequest(
        query="trace failure",
        kind=["dead_end"],
        agent="codex",
        since_days=7,
        limit=5,
    )
    assert range_query.route.subject == "meta.tempyr-journal-range"
    assert range_query.payload == TempyrJournalRangeRequest(
        rev_range="HEAD~3..HEAD",
        kind=["decision"],
        limit=3,
    )
    assert blame.route.subject == "meta.tempyr-journal-blame"
    assert blame.payload == TempyrJournalBlameRequest(
        file_path="crates/jam-cli/src/main.rs",
        limit=4,
    )


def test_worktree_diff_and_conflict_tools_prepare_validated_requests() -> None:
    registry = MaestroToolRegistry()

    diff = registry.prepare_request(
        "worktree-diff",
        {"worktree_path": "/home/picker/workers/task-1", "base_ref": "origin/main"},
    )
    conflicts = registry.prepare_request(
        "find-conflicts",
        {"worktree_path": "/home/picker/workers/task-1", "target_ref": "origin/main"},
    )

    assert diff.route.subject == "tool.worktree.worktree-diff"
    assert diff.payload == WorktreeWorktreeDiffRequest(
        worktree_path="/home/picker/workers/task-1",
        base_ref="origin/main",
    )
    assert conflicts.route.subject == "tool.worktree.find-conflicts"
    assert conflicts.payload == WorktreeFindConflictsRequest(
        worktree_path="/home/picker/workers/task-1",
        target_ref="origin/main",
    )


def test_candidate_queue_meta_tools_prepare_validated_requests() -> None:
    registry = MaestroToolRegistry()

    improvement = registry.prepare_request(
        "record-improvement-candidate",
        {
            "category": "tooling",
            "description": "Add a trace summary view.",
            "motivation": "Large trace chains need compression.",
        },
    )
    tool_change = registry.prepare_request(
        "propose-tool-change",
        {
            "spec": {"name": "trace-summarize"},
            "rationale": "Summaries would reduce review time.",
        },
    )
    tempyr_update = registry.prepare_request(
        "record-tempyr-update-candidate",
        {
            "candidate": {"node_id": "api-trace-replay", "status": "stable"},
            "reason": "Implementation is present.",
        },
    )

    assert improvement.route.subject == "meta.record-improvement-candidate"
    assert improvement.payload == RecordImprovementCandidateRequest(
        category="tooling",
        description="Add a trace summary view.",
        motivation="Large trace chains need compression.",
    )
    assert tool_change.route.subject == "meta.propose-tool-change"
    assert tool_change.payload == ProposeToolChangeRequest(
        spec={"name": "trace-summarize"},
        rationale="Summaries would reduce review time.",
    )
    assert tempyr_update.route.subject == "meta.record-tempyr-update-candidate"
    assert tempyr_update.payload == RecordTempyrUpdateCandidateRequest(
        candidate={"node_id": "api-trace-replay", "status": "stable"},
        reason="Implementation is present.",
    )


def test_request_research_prepares_validated_request() -> None:
    registry = MaestroToolRegistry()

    prepared = registry.prepare_request(
        "request-research",
        {
            "question": "What changed in Bevy terrain streaming APIs?",
            "tier": "deep",
            "scope": "blueberry/terrain",
            "deadline": "2026-05-06T23:59:00Z",
        },
    )

    assert prepared.route.subject == "tool.research.request-research"
    assert prepared.payload == ResearchRequestResearchRequest(
        question="What changed in Bevy terrain streaming APIs?",
        tier="deep",
        scope="blueberry/terrain",
        deadline="2026-05-06T23:59:00Z",
    )


def test_pr_comment_tool_surface_prepares_validated_requests() -> None:
    registry = MaestroToolRegistry()

    read_comments = registry.prepare_request(
        "read-pr-comments",
        {"pr_ref": "cleak/blueberry#42"},
    )
    classify = registry.prepare_request(
        "classify-review-artifacts",
        {
            "pr_ref": "cleak/blueberry#42",
            "artifacts": [
                {
                    "id": "coderabbit:42:1",
                    "body": "Please extract this system into a component.",
                }
            ],
        },
    )
    list_artifacts = registry.prepare_request(
        "list-review-artifacts",
        {"pr_ref": "cleak/blueberry#42", "status_filter": "Open"},
    )
    reply = registry.prepare_request(
        "reply-to-comment",
        {"artifact_id": "coderabbit:42:1", "text": "Addressed in the latest push."},
    )
    handled = registry.prepare_request(
        "mark-review-artifact-handled",
        {
            "artifact_id": "coderabbit:42:1",
            "status": "Addressed",
            "reasoning": "The requested component extraction landed.",
        },
    )
    request_review = registry.prepare_request(
        "request-review",
        {
            "pr_ref": "cleak/blueberry#42",
            "reviewer_id": "codex-review",
            "worktree_path": "/home/picker/workers/task-1",
            "task_id": "task-1",
            "base": "main",
        },
    )
    prepare_merge = registry.prepare_request(
        "prepare-merge",
        {"pr_ref": "cleak/blueberry#42"},
    )
    request_merge = registry.prepare_request(
        "request-human-merge",
        {"pr_ref": "cleak/blueberry#42", "summary": "Ready for Manager merge."},
    )

    assert read_comments.route.subject == "tool.repo.read-pr-comments"
    assert read_comments.payload == RepoReadPrCommentsRequest(pr_ref="cleak/blueberry#42")
    assert classify.route.subject == "tool.observe.classify-review-artifacts"
    assert classify.payload == ObserveClassifyReviewArtifactsRequest(
        pr_ref="cleak/blueberry#42",
        artifacts=[
            {
                "id": "coderabbit:42:1",
                "body": "Please extract this system into a component.",
            }
        ],
    )
    assert list_artifacts.route.subject == "tool.observe.list-review-artifacts"
    assert list_artifacts.payload == ObserveListReviewArtifactsRequest(
        pr_ref="cleak/blueberry#42",
        status_filter="Open",
    )
    assert reply.route.subject == "tool.repo.reply-to-comment"
    assert reply.payload == RepoReplyToCommentRequest(
        artifact_id="coderabbit:42:1",
        text="Addressed in the latest push.",
    )
    assert handled.route.subject == "tool.repo.mark-review-artifact-handled"
    assert handled.payload == RepoMarkReviewArtifactHandledRequest(
        artifact_id="coderabbit:42:1",
        status="Addressed",
        reasoning="The requested component extraction landed.",
    )
    assert request_review.route.subject == "tool.repo.request-review"
    assert request_review.payload == RepoRequestReviewRequest(
        pr_ref="cleak/blueberry#42",
        reviewer_id="codex-review",
        worktree_path="/home/picker/workers/task-1",
        task_id="task-1",
        base="main",
    )
    assert prepare_merge.route.subject == "tool.repo.prepare-merge"
    assert prepare_merge.payload == RepoPrepareMergeRequest(pr_ref="cleak/blueberry#42")
    assert request_merge.route.subject == "tool.repo.request-human-merge"
    assert request_merge.payload == RepoRequestHumanMergeRequest(
        pr_ref="cleak/blueberry#42",
        summary="Ready for Manager merge.",
    )


def test_web_search_prepares_validated_request() -> None:
    registry = MaestroToolRegistry()

    search = registry.prepare_request(
        "web-search",
        {
            "query": "bevy ecs 0.16 resources",
            "intent": "fast factual lookup",
            "time_range": "week",
            "domains": ["docs.rs", "bevyengine.org"],
        },
    )
    extract = registry.prepare_request(
        "web-extract",
        {
            "urls": ["https://bevyengine.org/learn"],
            "include_images": True,
        },
    )
    crawl = registry.prepare_request(
        "web-crawl",
        {
            "root_url": "https://bevyengine.org/learn",
            "max_depth": 1,
            "max_pages": 3,
        },
    )

    assert search.route.subject == "tool.search.web-search"
    assert search.payload == SearchWebSearchRequest(
        query="bevy ecs 0.16 resources",
        intent="fast factual lookup",
        time_range="week",
        domains=["docs.rs", "bevyengine.org"],
    )
    assert extract.route.subject == "tool.search.web-extract"
    assert extract.payload == SearchWebExtractRequest(
        urls=["https://bevyengine.org/learn"],
        include_images=True,
    )
    assert crawl.route.subject == "tool.search.web-crawl"
    assert crawl.payload == SearchWebCrawlRequest(
        root_url="https://bevyengine.org/learn",
        max_depth=1,
        max_pages=3,
    )


def test_session_inspection_tools_prepare_validated_requests() -> None:
    registry = MaestroToolRegistry()

    inspect = registry.prepare_request("inspect-picker", {"session_id": "codex-cli:abc"})
    list_active = registry.prepare_request("list-active", {})
    archive = registry.prepare_request("archive-session", {"session_id": "codex-cli:abc"})
    purge = registry.prepare_request(
        "purge-session",
        {"session_id": "codex-cli:abc", "reason": "obsolete", "preserve_worktree": True},
    )

    assert inspect.route.subject == "tool.session.inspect-picker"
    assert list_active.route.subject == "tool.session.list-active"
    assert list_active.payload == SessionListActiveRequest()
    assert archive.route.subject == "tool.session.archive-session"
    assert archive.payload == SessionArchiveSessionRequest(session_id="codex-cli:abc")
    assert purge.route.subject == "tool.session.purge-session"
    assert purge.payload == SessionPurgeSessionRequest(
        session_id="codex-cli:abc",
        reason="obsolete",
        preserve_worktree=True,
    )


def test_message_mode_tools_prepare_validated_requests() -> None:
    registry = MaestroToolRegistry()

    queue = registry.prepare_request(
        "enqueue-message",
        {"session_id": "codex-cli:abc", "text": "Use rayon here.", "from": "maestro:session-1"},
    )
    interrupt = registry.prepare_request(
        "interrupt-with-message",
        {"session_id": "codex-cli:abc", "text": "Stop and inspect the failing test."},
    )
    full_stop = registry.prepare_request(
        "full-stop",
        {
            "session_id": "codex-cli:abc",
            "reason": "stuck in repeated tool loop",
            "requested_by": "maestro:session-1",
        },
    )

    assert queue.route.subject == "tool.message.enqueue-message"
    assert queue.payload == MessageEnqueueMessageRequest.model_validate(
        {
            "session_id": "codex-cli:abc",
            "text": "Use rayon here.",
            "from": "maestro:session-1",
        }
    )
    assert interrupt.route.subject == "tool.message.interrupt-with-message"
    assert interrupt.payload == MessageInterruptWithMessageRequest(
        session_id="codex-cli:abc",
        text="Stop and inspect the failing test.",
    )
    assert full_stop.route.subject == "tool.message.full-stop"
    assert full_stop.payload == MessageFullStopRequest(
        session_id="codex-cli:abc",
        reason="stuck in repeated tool loop",
        requested_by="maestro:session-1",
    )


def test_notify_human_prepares_validated_request() -> None:
    registry = MaestroToolRegistry()

    prepared = registry.prepare_request(
        "notify-human",
        {
            "urgency": "high",
            "summary": "Review Tempyr write failure",
            "payload": {"write_id": "write-1"},
        },
    )

    assert prepared.route.subject == "tool.supervise.notify-human"
    assert prepared.payload == SuperviseNotifyHumanRequest(
        urgency="high",
        summary="Review Tempyr write failure",
        payload={"write_id": "write-1"},
    )


def test_dispatch_pause_resume_prepare_validated_requests() -> None:
    registry = MaestroToolRegistry()

    pause = registry.prepare_request(
        "pause-dispatch",
        {
            "reason": "all harnesses exhausted",
            "changed_by": "maestro:session-1",
        },
    )
    resume = registry.prepare_request(
        "resume-dispatch",
        {"changed_by": "human:caleb"},
    )

    assert pause.route.subject == "tool.supervise.pause-dispatch"
    assert pause.payload == SupervisePauseDispatchRequest(
        reason="all harnesses exhausted",
        changed_by="maestro:session-1",
    )
    assert resume.route.subject == "tool.supervise.resume-dispatch"
    assert resume.payload == SuperviseResumeDispatchRequest(changed_by="human:caleb")


def test_query_quota_prepares_validated_request() -> None:
    registry = MaestroToolRegistry()

    prepared = registry.prepare_request("query-quota", {"harness_id": "codex-cli"})

    assert prepared.route.subject == "tool.observe.query-quota"
    assert prepared.payload == ObserveQueryQuotaRequest(harness_id="codex-cli")


def test_known_tool_rejects_bad_payload() -> None:
    registry = MaestroToolRegistry()

    with pytest.raises(ValidationError):
        registry.prepare_request("world-snapshot", {"task_id": ""})


def test_registry_refuses_accidental_absent_tool_registration() -> None:
    routes = {
        "merge-pr": ToolRoute(
            name="merge-pr",
            subject="tool.repo.merge-pr",
            request_model=ObserveWorldSnapshotRequest,
        )
    }

    with pytest.raises(ValueError, match="deliberately absent tools registered: merge-pr"):
        MaestroToolRegistry(routes)
