"""Blueberry project config loader."""

from __future__ import annotations

import re
import tomllib
from typing import TYPE_CHECKING, Any, Literal, cast

from pydantic import Field, field_validator

from jam_maestro.models import StrictBaseModel
from jam_maestro.paths import jam_home

if TYPE_CHECKING:
    from pathlib import Path

PROJECT_NAME = "blueberry"
COMPOSIO_CONFIG_FILENAME = "mcp-composio.toml"
DEFAULT_COMPOSIO_ENDPOINT = "https://connect.composio.dev/mcp"
TOOLKIT_PATTERN = re.compile(r"^[a-z0-9_.-]+$")
type TomlMap = dict[str, Any]


class McpServerConfig(StrictBaseModel):
    """One MCP server entry from `[mcp-servers]`."""

    url: str = Field(min_length=1)
    enabled: bool = True
    auth: str | None = Field(default=None, min_length=1)
    toolkits: tuple[str, ...] = Field(default_factory=tuple)


class ComposioConnectConfig(StrictBaseModel):
    """Composio Connect sidecar config from `mcp-composio.toml`."""

    endpoint: str = Field(default=DEFAULT_COMPOSIO_ENDPOINT, min_length=1)
    secret_key: str = Field(validation_alias="secret-key", min_length=1)
    enabled_toolkits: tuple[str, ...] = Field(
        min_length=1,
        validation_alias="enabled-toolkits",
    )

    @field_validator("enabled_toolkits")
    @classmethod
    def normalize_toolkits(cls, value: tuple[str, ...]) -> tuple[str, ...]:
        """Normalize toolkit names to the registry-safe form used by MCP servers."""
        normalized: list[str] = []
        seen: set[str] = set()
        for raw in value:
            toolkit = raw.strip().lower()
            if not TOOLKIT_PATTERN.fullmatch(toolkit):
                msg = f"invalid Composio toolkit name: {raw!r}"
                raise ValueError(msg)
            if toolkit not in seen:
                normalized.append(toolkit)
                seen.add(toolkit)
        return tuple(normalized)


class BlueberryProjectConfig(StrictBaseModel):
    """The v1 single-project config for Blueberry."""

    name: Literal["blueberry"] = PROJECT_NAME
    mcp_servers: dict[str, McpServerConfig] = Field(default_factory=dict)

    def enabled_mcp_servers(self) -> dict[str, McpServerConfig]:
        """Return enabled MCP servers keyed by registry name."""
        return {name: server for name, server in self.mcp_servers.items() if server.enabled}


def default_project_config_path() -> Path:
    """Return the default Blueberry project config path under `JAM_HOME`."""
    return jam_home() / "config" / "projects" / "blueberry.toml"


def default_composio_config_path() -> Path:
    """Return the default Composio MCP sidecar config path under `JAM_HOME`."""
    return jam_home() / "config" / COMPOSIO_CONFIG_FILENAME


def load_blueberry_project_config(
    path: Path | None = None,
    *,
    composio_path: Path | None = None,
) -> BlueberryProjectConfig:
    """Load Blueberry's project config."""
    active_path = path or default_project_config_path()
    raw = tomllib.loads(active_path.read_text(encoding="utf-8"))
    data = _blueberry_config_data(raw)
    active_composio_path = composio_path
    if active_composio_path is None and path is None:
        active_composio_path = default_composio_config_path()
    if active_composio_path is not None:
        composio = load_composio_connect_config(active_composio_path, missing_ok=True)
        if composio is not None:
            data["mcp_servers"] = _merge_composio_servers(
                cast("dict[str, TomlMap]", data["mcp_servers"]),
                composio,
            )
    return BlueberryProjectConfig.model_validate(data)


def load_composio_connect_config(
    path: Path,
    *,
    missing_ok: bool = False,
) -> ComposioConnectConfig | None:
    """Load the optional Composio Connect MCP sidecar config."""
    if missing_ok and not path.exists():
        return None
    raw = tomllib.loads(path.read_text(encoding="utf-8"))
    return ComposioConnectConfig.model_validate(raw)


def _blueberry_config_data(raw: TomlMap) -> TomlMap:
    name = raw.get("name", PROJECT_NAME)
    mcp_servers = raw.get("mcp-servers", {})
    if not isinstance(mcp_servers, dict):
        msg = "[mcp-servers] must be a TOML table"
        raise TypeError(msg)
    return {
        "name": name,
        "mcp_servers": _mcp_servers(cast("TomlMap", mcp_servers)),
    }


def _mcp_servers(raw: TomlMap) -> dict[str, TomlMap]:
    servers: dict[str, TomlMap] = {}
    for name, value in raw.items():
        if not isinstance(value, dict):
            msg = f"mcp server {name!r} must be an inline table or table"
            raise TypeError(msg)
        servers[name] = cast("TomlMap", value)
    return servers


def _merge_composio_servers(
    servers: dict[str, TomlMap],
    composio: ComposioConnectConfig,
) -> dict[str, TomlMap]:
    merged = dict(servers)
    endpoint = composio.endpoint.rstrip("/")
    for toolkit in composio.enabled_toolkits:
        name = f"composio-{toolkit}"
        if name in merged:
            msg = f"mcp server {name!r} conflicts with {COMPOSIO_CONFIG_FILENAME}"
            raise TypeError(msg)
        merged[name] = {
            "url": endpoint,
            "enabled": True,
            "auth": composio.secret_key,
            "toolkits": (toolkit,),
        }
    return merged
