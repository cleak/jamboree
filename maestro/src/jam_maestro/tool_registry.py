"""Maestro callable-tool allowlist."""

from __future__ import annotations

from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING, Final

from jam_maestro.candidates import (
    ProposeToolChangeRequest,
    RecordImprovementCandidateRequest,
    RecordTempyrUpdateCandidateRequest,
)
from jam_maestro.journal_reader import ReadJournalRequest
from jam_maestro.mcp_router import McpDiscoverAndLoadRequest
from jam_maestro.record_learning import RecordLearningRequest
from jam_maestro.session_store import QuerySessionStoreRequest
from jam_maestro.skills import ReadSkillsRequest
from jam_maestro.tempyr_journal_query import (
    TempyrJournalBlameRequest,
    TempyrJournalRangeRequest,
    TempyrJournalSearchRequest,
)
from jam_maestro.tempyr_query import QueryTempyrRequest
from jam_maestro.tools import (
    EvolveRequestSkillEvolutionRequest,
    MessageEnqueueMessageRequest,
    MessageFullStopRequest,
    MessageInterruptWithMessageRequest,
    ObserveBranchStalenessRequest,
    ObserveClassifyReviewArtifactsRequest,
    ObserveComputeReadinessRequest,
    ObserveListBlockersRequest,
    ObserveListReviewArtifactsRequest,
    ObserveQueryQuotaRequest,
    ObserveRefreshWorldSnapshotRequest,
    ObserveWorldSnapshotDeltaRequest,
    ObserveWorldSnapshotRequest,
    RepoMarkReviewArtifactHandledRequest,
    RepoOpenPrRequest,
    RepoPrepareMergeRequest,
    RepoPrStatusRequest,
    RepoReadPrCommentsRequest,
    RepoReplyToCommentRequest,
    RepoRequestHumanMergeRequest,
    RepoRequestReviewRequest,
    ResearchRequestResearchRequest,
    SearchWebCrawlRequest,
    SearchWebExtractRequest,
    SearchWebSearchRequest,
    SessionArchiveSessionRequest,
    SessionInspectPickerRequest,
    SessionListActiveRequest,
    SessionPurgeSessionRequest,
    SessionSpawnPickerRequest,
    SuperviseNotifyHumanRequest,
    SupervisePauseDispatchRequest,
    SuperviseResumeDispatchRequest,
    WorktreeFindConflictsRequest,
    WorktreeWorktreeDiffRequest,
)

if TYPE_CHECKING:
    from collections.abc import Mapping

    from pydantic import BaseModel

DELIBERATELY_ABSENT_TOOL_NAMES: Final[frozenset[str]] = frozenset(
    {
        "add-tool",
        "auto-merge",
        "auto-rebase",
        "auto-update-tempyr-node",
        "clone-session",
        "eval",
        "exec",
        "fork-Maestro",
        "merge-pr",
        "python -c",
        "read-file",
        "run-command",
        "set-task-plan-note",
        "write-file",
    }
)


class NoSuchToolError(LookupError):
    """Raised when the Maestro tries to call a tool outside the allowlist."""

    def __init__(self, tool_name: str) -> None:
        self.tool_name = tool_name
        super().__init__(f"no such tool: {tool_name}")


class DeliberatelyAbsentToolRegisteredError(ValueError):
    """Raised when a forbidden tool name appears in the callable registry."""

    def __init__(self, tool_names: str) -> None:
        self.tool_names = tool_names
        super().__init__(f"deliberately absent tools registered: {tool_names}")


@dataclass(frozen=True, slots=True)
class ToolRoute:
    """One callable Maestro tool route."""

    name: str
    subject: str
    request_model: type[BaseModel]


@dataclass(frozen=True, slots=True)
class PreparedToolCall:
    """Validated tool call ready for the NATS request boundary."""

    route: ToolRoute
    payload: BaseModel


def _route(
    name: str,
    subject: str,
    request_model: type[BaseModel],
) -> tuple[str, ToolRoute]:
    return name, ToolRoute(name=name, subject=subject, request_model=request_model)


DEFAULT_TOOL_ROUTES: Final[Mapping[str, ToolRoute]] = MappingProxyType(
    dict(
        [
            _route(
                "branch-staleness",
                "tool.observe.branch-staleness",
                ObserveBranchStalenessRequest,
            ),
            _route(
                "classify-review-artifacts",
                "tool.observe.classify-review-artifacts",
                ObserveClassifyReviewArtifactsRequest,
            ),
            _route(
                "compute-readiness",
                "tool.observe.compute-readiness",
                ObserveComputeReadinessRequest,
            ),
            _route(
                "enqueue-message",
                "tool.message.enqueue-message",
                MessageEnqueueMessageRequest,
            ),
            _route("full-stop", "tool.message.full-stop", MessageFullStopRequest),
            _route("inspect-picker", "tool.session.inspect-picker", SessionInspectPickerRequest),
            _route(
                "list-blockers",
                "tool.observe.list-blockers",
                ObserveListBlockersRequest,
            ),
            _route("list-active", "tool.session.list-active", SessionListActiveRequest),
            _route(
                "list-review-artifacts",
                "tool.observe.list-review-artifacts",
                ObserveListReviewArtifactsRequest,
            ),
            _route(
                "mark-review-artifact-handled",
                "tool.repo.mark-review-artifact-handled",
                RepoMarkReviewArtifactHandledRequest,
            ),
            _route(
                "mcp-discover-and-load",
                "meta.mcp-discover-and-load",
                McpDiscoverAndLoadRequest,
            ),
            _route("open-pr", "tool.repo.open-pr", RepoOpenPrRequest),
            _route("notify-human", "tool.supervise.notify-human", SuperviseNotifyHumanRequest),
            _route(
                "pause-dispatch",
                "tool.supervise.pause-dispatch",
                SupervisePauseDispatchRequest,
            ),
            _route("prepare-merge", "tool.repo.prepare-merge", RepoPrepareMergeRequest),
            _route("pr-status", "tool.repo.pr-status", RepoPrStatusRequest),
            _route(
                "propose-tool-change",
                "meta.propose-tool-change",
                ProposeToolChangeRequest,
            ),
            _route("query-quota", "tool.observe.query-quota", ObserveQueryQuotaRequest),
            _route(
                "query-session-store",
                "meta.query-session-store",
                QuerySessionStoreRequest,
            ),
            _route("query-tempyr", "meta.query-tempyr", QueryTempyrRequest),
            _route(
                "tempyr-journal-search",
                "meta.tempyr-journal-search",
                TempyrJournalSearchRequest,
            ),
            _route(
                "tempyr-journal-range",
                "meta.tempyr-journal-range",
                TempyrJournalRangeRequest,
            ),
            _route(
                "tempyr-journal-blame",
                "meta.tempyr-journal-blame",
                TempyrJournalBlameRequest,
            ),
            _route(
                "refresh-world-snapshot",
                "tool.observe.refresh-world-snapshot",
                ObserveRefreshWorldSnapshotRequest,
            ),
            _route(
                "read-pr-comments",
                "tool.repo.read-pr-comments",
                RepoReadPrCommentsRequest,
            ),
            _route("read-journal", "meta.read-journal", ReadJournalRequest),
            _route("read-skills", "meta.read-skills", ReadSkillsRequest),
            _route(
                "record-improvement-candidate",
                "meta.record-improvement-candidate",
                RecordImprovementCandidateRequest,
            ),
            _route("record-learning", "meta.record-learning", RecordLearningRequest),
            _route(
                "record-tempyr-update-candidate",
                "meta.record-tempyr-update-candidate",
                RecordTempyrUpdateCandidateRequest,
            ),
            _route(
                "interrupt-with-message",
                "tool.message.interrupt-with-message",
                MessageInterruptWithMessageRequest,
            ),
            _route(
                "reply-to-comment",
                "tool.repo.reply-to-comment",
                RepoReplyToCommentRequest,
            ),
            _route(
                "request-human-merge",
                "tool.repo.request-human-merge",
                RepoRequestHumanMergeRequest,
            ),
            _route(
                "request-skill-evolution",
                "tool.evolve.request-skill-evolution",
                EvolveRequestSkillEvolutionRequest,
            ),
            _route(
                "request-research",
                "tool.research.request-research",
                ResearchRequestResearchRequest,
            ),
            _route(
                "request-review",
                "tool.repo.request-review",
                RepoRequestReviewRequest,
            ),
            _route(
                "resume-dispatch",
                "tool.supervise.resume-dispatch",
                SuperviseResumeDispatchRequest,
            ),
            _route("web-crawl", "tool.search.web-crawl", SearchWebCrawlRequest),
            _route("web-extract", "tool.search.web-extract", SearchWebExtractRequest),
            _route("web-search", "tool.search.web-search", SearchWebSearchRequest),
            _route("worktree-diff", "tool.worktree.worktree-diff", WorktreeWorktreeDiffRequest),
            _route("find-conflicts", "tool.worktree.find-conflicts", WorktreeFindConflictsRequest),
            _route(
                "archive-session",
                "tool.session.archive-session",
                SessionArchiveSessionRequest,
            ),
            _route("purge-session", "tool.session.purge-session", SessionPurgeSessionRequest),
            _route("spawn-picker", "tool.session.spawn-picker", SessionSpawnPickerRequest),
            _route("world-snapshot", "tool.observe.world-snapshot", ObserveWorldSnapshotRequest),
            _route(
                "world-snapshot-delta",
                "tool.observe.world-snapshot-delta",
                ObserveWorldSnapshotDeltaRequest,
            ),
        ]
    )
)


class MaestroToolRegistry:
    """Validate and prepare calls for the tools the Maestro is allowed to use."""

    def __init__(self, routes: Mapping[str, ToolRoute] = DEFAULT_TOOL_ROUTES) -> None:
        overlap = DELIBERATELY_ABSENT_TOOL_NAMES.intersection(routes)
        if overlap:
            names = ", ".join(sorted(overlap))
            raise DeliberatelyAbsentToolRegisteredError(names)
        self._routes = routes

    @property
    def names(self) -> frozenset[str]:
        """Return the callable tool names."""
        return frozenset(self._routes)

    def route_for(self, tool_name: str) -> ToolRoute:
        """Return a tool route or raise a stable no-such-tool error."""
        route = self._routes.get(tool_name)
        if route is None:
            raise NoSuchToolError(tool_name)
        return route

    def prepare_request(self, tool_name: str, payload: object) -> PreparedToolCall:
        """Validate a request payload against the registered tool contract."""
        route = self.route_for(tool_name)
        return PreparedToolCall(
            route=route,
            payload=route.request_model.model_validate(payload),
        )
