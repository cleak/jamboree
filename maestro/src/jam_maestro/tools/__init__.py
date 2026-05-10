"""Generated typed tool I/O models for Maestro."""

from __future__ import annotations

from jam_maestro.tools.evolve import (
    EvolveRequestSkillEvolutionRequest,
)
from jam_maestro.tools.message import (
    MessageEnqueueMessageRequest,
    MessageFullStopRequest,
    MessageInterruptWithMessageRequest,
)
from jam_maestro.tools.observe import (
    ObserveBranchStalenessRequest,
    ObserveClassifyReviewArtifactsRequest,
    ObserveComputeReadinessRequest,
    ObserveListBlockersRequest,
    ObserveListReviewArtifactsRequest,
    ObserveQueryQuotaRequest,
    ObserveRefreshWorldSnapshotRequest,
    ObserveWorldSnapshotDeltaRequest,
    ObserveWorldSnapshotDeltaResponse,
    ObserveWorldSnapshotRequest,
    ObserveWorldSnapshotResponse,
)
from jam_maestro.tools.repo import (
    RepoMarkReviewArtifactHandledRequest,
    RepoOpenPrRequest,
    RepoPrepareMergeRequest,
    RepoPrStatusRequest,
    RepoReadPrCommentsRequest,
    RepoReplyToCommentRequest,
    RepoRequestHumanMergeRequest,
    RepoRequestReviewRequest,
)
from jam_maestro.tools.research import (
    ResearchRequestResearchRequest,
)
from jam_maestro.tools.search import (
    SearchWebCrawlRequest,
    SearchWebCrawlResponse,
    SearchWebExtractRequest,
    SearchWebExtractResponse,
    SearchWebSearchRequest,
)
from jam_maestro.tools.session import (
    SessionArchiveSessionRequest,
    SessionFullStopRequest,
    SessionInspectPickerRequest,
    SessionListActiveRequest,
    SessionPurgeSessionRequest,
    SessionSpawnPickerRequest,
)
from jam_maestro.tools.supervise import (
    SuperviseNotifyHumanRequest,
    SupervisePauseDispatchRequest,
    SuperviseResumeDispatchRequest,
)
from jam_maestro.tools.worktree import (
    WorktreeCreateRequest,
    WorktreeCreateResponse,
    WorktreeFindConflictsRequest,
    WorktreeFindConflictsResponse,
    WorktreeWorktreeDiffRequest,
    WorktreeWorktreeDiffResponse,
)

__all__ = [
    "EvolveRequestSkillEvolutionRequest",
    "MessageEnqueueMessageRequest",
    "MessageFullStopRequest",
    "MessageInterruptWithMessageRequest",
    "ObserveBranchStalenessRequest",
    "ObserveClassifyReviewArtifactsRequest",
    "ObserveComputeReadinessRequest",
    "ObserveListBlockersRequest",
    "ObserveListReviewArtifactsRequest",
    "ObserveQueryQuotaRequest",
    "ObserveRefreshWorldSnapshotRequest",
    "ObserveWorldSnapshotDeltaRequest",
    "ObserveWorldSnapshotDeltaResponse",
    "ObserveWorldSnapshotRequest",
    "ObserveWorldSnapshotResponse",
    "RepoMarkReviewArtifactHandledRequest",
    "RepoOpenPrRequest",
    "RepoPrStatusRequest",
    "RepoPrepareMergeRequest",
    "RepoReadPrCommentsRequest",
    "RepoReplyToCommentRequest",
    "RepoRequestHumanMergeRequest",
    "RepoRequestReviewRequest",
    "ResearchRequestResearchRequest",
    "SearchWebCrawlRequest",
    "SearchWebCrawlResponse",
    "SearchWebExtractRequest",
    "SearchWebExtractResponse",
    "SearchWebSearchRequest",
    "SessionArchiveSessionRequest",
    "SessionFullStopRequest",
    "SessionInspectPickerRequest",
    "SessionListActiveRequest",
    "SessionPurgeSessionRequest",
    "SessionSpawnPickerRequest",
    "SuperviseNotifyHumanRequest",
    "SupervisePauseDispatchRequest",
    "SuperviseResumeDispatchRequest",
    "WorktreeCreateRequest",
    "WorktreeCreateResponse",
    "WorktreeFindConflictsRequest",
    "WorktreeFindConflictsResponse",
    "WorktreeWorktreeDiffRequest",
    "WorktreeWorktreeDiffResponse",
]
