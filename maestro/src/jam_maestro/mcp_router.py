"""Dynamic MCP tool loading and response trust-boundary helpers."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, Final, Literal, Protocol

from pydantic import Field

from jam_maestro.models import StrictBaseModel, TraceId
from jam_maestro.project_config import (
    BlueberryProjectConfig,
    McpServerConfig,
    load_blueberry_project_config,
)
from jam_maestro.tempyr_journal import (
    CliTempyrJournal,
    DecisionEntry,
    JournalFinalizeResult,
    TempyrJournalClient,
)
from jam_maestro.untrusted import Untrusted, mark_untrusted

if TYPE_CHECKING:
    from collections.abc import Mapping

_SERVER_KEYWORDS: Final[dict[str, frozenset[str]]] = {
    "composio": frozenset(
        {
            "calendar",
            "gmail",
            "linear",
            "notion",
            "oauth",
            "slack",
            "ticket",
        }
    ),
    "context7": frozenset(
        {
            "api",
            "bevy",
            "crate",
            "docs",
            "documentation",
            "library",
            "rust",
            "version",
        }
    ),
    "github": frozenset(
        {
            "branch",
            "commit",
            "github",
            "issue",
            "pr",
            "pull",
            "repository",
            "review",
        }
    ),
    "tempyr": frozenset(
        {
            "decision",
            "design",
            "graph",
            "knowledge",
            "node",
            "task",
            "tempyr",
        }
    ),
}
_COMPOSIO_TOOLKIT_KEYWORDS: Final[dict[str, frozenset[str]]] = {
    "calendar": frozenset({"calendar", "event", "schedule"}),
    "gmail": frozenset({"email", "gmail", "mail"}),
    "linear": frozenset({"issue", "linear", "ticket"}),
    "notion": frozenset({"doc", "notion", "page", "wiki"}),
    "slack": frozenset({"channel", "message", "slack"}),
}
_COMPOSIO_GENERIC_KEYWORDS: Final[frozenset[str]] = frozenset(
    {"connect", "connector", "oauth"}
)


class McpRouterError(RuntimeError):
    """Raised when the MCP router cannot prepare or call a toolkit."""


class McpDiscoverAndLoadRequest(StrictBaseModel):
    """Inputs for the `mcp-discover-and-load` meta-tool."""

    intent: str = Field(min_length=1, max_length=500)
    project: Literal["blueberry"] = "blueberry"


class LoadedMcpServer(StrictBaseModel):
    """One enabled MCP server selected for the current intent."""

    name: str = Field(pattern=r"^[A-Za-z0-9_.-]+$", max_length=128)
    url: str = Field(min_length=1)
    auth_ref: str | None = Field(default=None, min_length=1)
    toolkits: tuple[str, ...] = Field(default_factory=tuple)


class McpDiscoverAndLoadResult(StrictBaseModel):
    """Result of loading MCP servers for an intent."""

    project: Literal["blueberry"]
    intent: str
    loaded_servers: list[LoadedMcpServer]
    trace_id: TraceId
    journal_logged: bool
    journal_flushed: bool = False
    journal_flush_error: str | None = None


class McpToolCallRequest(StrictBaseModel):
    """Inputs for calling a tool from a loaded MCP server."""

    server_name: str = Field(pattern=r"^[A-Za-z0-9_.-]+$", max_length=128)
    tool_name: str = Field(pattern=r"^[A-Za-z0-9_.-]+$", max_length=128)
    arguments: dict[str, object] = Field(default_factory=dict)
    project: Literal["blueberry"] = "blueberry"


@dataclass(frozen=True, slots=True)
class McpToolCallResult:
    """An MCP tool response after it crosses into the Maestro boundary."""

    server_name: str
    tool_name: str
    trace_id: str
    body: Untrusted
    journal_logged: bool
    journal_flushed: bool = False
    journal_flush_error: str | None = None


class McpClient(Protocol):
    """Small adapter surface for MCP clients owned by launch/runtime code."""

    async def call_tool(
        self,
        server: LoadedMcpServer,
        tool_name: str,
        arguments: Mapping[str, object],
        trace_id: str,
    ) -> str:
        """Call a tool and return the raw external response body."""
        ...


@dataclass(frozen=True, slots=True)
class McpRouterRuntime:
    """Runtime dependencies for MCP router operations."""

    config: BlueberryProjectConfig | None = None
    journal: TempyrJournalClient | None = None
    agent: str | None = None


async def mcp_discover_and_load(
    request: McpDiscoverAndLoadRequest,
    *,
    trace_id: TraceId,
    runtime: McpRouterRuntime | None = None,
) -> McpDiscoverAndLoadResult:
    """Load enabled MCP servers matching an intent and log the traced decision."""
    active_runtime = runtime or McpRouterRuntime()
    active_config = active_runtime.config or load_blueberry_project_config()
    _ensure_project(request.project, active_config)
    loaded = discover_mcp_servers(request.intent, active_config)

    finalize_result = await _log_decision(
        active_runtime.journal or CliTempyrJournal(),
        active_runtime.agent or _agent_for("mcp-router", trace_id),
        _discover_entry(request, loaded, trace_id),
    )

    return McpDiscoverAndLoadResult(
        project=request.project,
        intent=request.intent,
        loaded_servers=loaded,
        trace_id=trace_id,
        journal_logged=True,
        journal_flushed=finalize_result.flushed,
        journal_flush_error=finalize_result.flush_error,
    )


def discover_mcp_servers(
    intent: str,
    config: BlueberryProjectConfig,
) -> list[LoadedMcpServer]:
    """Return enabled MCP servers whose registry metadata matches the intent."""
    scored: list[tuple[int, str, McpServerConfig]] = []
    for name, server in config.enabled_mcp_servers().items():
        score = _server_score(name, server, intent)
        if score > 0:
            scored.append((score, name, server))

    if not scored and len(config.enabled_mcp_servers()) == 1:
        name, server = next(iter(config.enabled_mcp_servers().items()))
        scored.append((1, name, server))

    return [
        _loaded_server(name, server)
        for _, name, server in sorted(scored, key=lambda item: (-item[0], item[1]))
    ]


async def call_mcp_tool(
    request: McpToolCallRequest,
    *,
    trace_id: TraceId,
    client: McpClient,
    runtime: McpRouterRuntime | None = None,
) -> McpToolCallResult:
    """Call one MCP tool and wrap the raw response as untrusted content."""
    active_runtime = runtime or McpRouterRuntime()
    active_config = active_runtime.config or load_blueberry_project_config()
    _ensure_project(request.project, active_config)
    server = _enabled_server(request.server_name, active_config)

    body = await client.call_tool(
        _loaded_server(request.server_name, server),
        request.tool_name,
        request.arguments,
        trace_id,
    )
    finalize_result = await _log_decision(
        active_runtime.journal or CliTempyrJournal(),
        active_runtime.agent or _agent_for("mcp-tool-call", trace_id),
        _tool_call_entry(request, trace_id),
    )

    return McpToolCallResult(
        server_name=request.server_name,
        tool_name=request.tool_name,
        trace_id=trace_id,
        body=mark_untrusted(body),
        journal_logged=True,
        journal_flushed=finalize_result.flushed,
        journal_flush_error=finalize_result.flush_error,
    )


def _ensure_project(project: str, config: BlueberryProjectConfig) -> None:
    if project != config.name:
        msg = f"project config mismatch: request={project!r} config={config.name!r}"
        raise McpRouterError(msg)


def _enabled_server(name: str, config: BlueberryProjectConfig) -> McpServerConfig:
    server = config.enabled_mcp_servers().get(name)
    if server is None:
        msg = f"enabled MCP server not found: {name}"
        raise McpRouterError(msg)
    return server


def _loaded_server(name: str, server: McpServerConfig) -> LoadedMcpServer:
    return LoadedMcpServer(
        name=name,
        url=server.url,
        auth_ref=server.auth,
        toolkits=server.toolkits,
    )


def _server_score(name: str, server: McpServerConfig, intent: str) -> int:
    terms = _terms(intent)
    searchable = _terms(f"{name} {server.url} {' '.join(server.toolkits)}")
    score = len(terms.intersection(searchable))

    normalized_name = name.lower()
    normalized_url = server.url.lower()
    for alias, keywords in _SERVER_KEYWORDS.items():
        if alias in normalized_name or alias in normalized_url:
            if alias == "composio" and server.toolkits:
                score += _composio_toolkit_score(terms, server.toolkits)
                continue
            score += len(terms.intersection(keywords))
    return score


def _composio_toolkit_score(terms: frozenset[str], toolkits: tuple[str, ...]) -> int:
    score = len(terms.intersection(_COMPOSIO_GENERIC_KEYWORDS))
    for toolkit in toolkits:
        keywords = _COMPOSIO_TOOLKIT_KEYWORDS.get(toolkit, frozenset({toolkit}))
        score += len(terms.intersection(keywords))
    return score


def _terms(value: str) -> frozenset[str]:
    normalized = "".join(char.lower() if char.isalnum() else " " for char in value)
    return frozenset(part for part in normalized.split() if part)


async def _log_decision(
    journal: TempyrJournalClient,
    agent: str,
    entry: DecisionEntry,
) -> JournalFinalizeResult:
    await journal.bootstrap(agent)
    await journal.log_decision(agent, entry)
    return await journal.finalize(agent)


def _discover_entry(
    request: McpDiscoverAndLoadRequest,
    loaded: list[LoadedMcpServer],
    trace_id: str,
) -> DecisionEntry:
    loaded_names = ", ".join(server.name for server in loaded) or "none"
    return DecisionEntry(
        summary="Loaded MCP servers for Maestro intent",
        chosen=f"load MCP servers: {loaded_names}",
        rationale=(
            "The Tool Router pattern keeps MCP tools out of the base prompt and loads "
            "only enabled project servers that match the current intent."
        ),
        detail=(
            f"mcp-discover-and-load evaluated intent {request.intent!r} for "
            f"project {request.project} and selected {loaded_names}. "
            f"The decision is tied to trace {trace_id}."
        ),
        reversible=True,
        tags=[
            "tool:mcp-discover-and-load",
            f"project:{request.project}",
            f"trace:{trace_id}",
        ],
        refs=["task-mcp-discover-and-load"],
    )


def _tool_call_entry(request: McpToolCallRequest, trace_id: str) -> DecisionEntry:
    return DecisionEntry(
        summary="Called MCP tool through trusted router boundary",
        chosen=f"call {request.server_name}.{request.tool_name}",
        rationale=(
            "MCP tool responses are external content, so the router records the traced "
            "call and returns the body wrapped as Untrusted before any Maestro use."
        ),
        detail=(
            f"Called MCP tool {request.tool_name!r} on server {request.server_name!r} "
            f"for project {request.project}. The raw MCP response body was not logged "
            f"or trusted; it was wrapped as Untrusted for trace {trace_id}."
        ),
        reversible=True,
        tags=[
            "tool:mcp-call",
            f"mcp-server:{request.server_name}",
            f"project:{request.project}",
            f"trace:{trace_id}",
        ],
        refs=[
            "task-mcp-discover-and-load",
            "task-untrusted-content-wrapping-mcp",
        ],
    )


def _agent_for(scope: str, trace_id: str) -> str:
    return f"maestro:{scope}:{trace_id[-6:].lower()}"
