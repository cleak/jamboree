"""Tests for Blueberry project config loading."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from jam_maestro.project_config import load_blueberry_project_config

if TYPE_CHECKING:
    from pathlib import Path


def test_loads_mcp_servers_from_blueberry_project_config(tmp_path: Path) -> None:
    config = tmp_path / "blueberry.toml"
    config.write_text(
        """
name = "blueberry"
repo-path = "/home/caleb/blueberry"
trunk-branch = "main"

[mcp-servers]
tempyr = { url = "stdio:tempyr --mcp", enabled = true }
context7 = { url = "https://mcp.context7.com/mcp/v1", enabled = true }
github-mcp = { url = "https://api.githubcopilot.com/mcp/", enabled = true, auth = "github-pat" }
warpgrep = { url = "stdio:warpgrep", enabled = false }
""",
        encoding="utf-8",
    )

    loaded = load_blueberry_project_config(config)

    assert loaded.name == "blueberry"
    assert loaded.mcp_servers["tempyr"].url == "stdio:tempyr --mcp"
    assert loaded.mcp_servers["github-mcp"].auth == "github-pat"
    assert set(loaded.enabled_mcp_servers()) == {"tempyr", "context7", "github-mcp"}


def test_loads_composio_sidecar_as_toolkit_servers(tmp_path: Path) -> None:
    config = tmp_path / "blueberry.toml"
    config.write_text(
        """
name = "blueberry"

[mcp-servers]
tempyr = { url = "stdio:tempyr --mcp", enabled = true }
""",
        encoding="utf-8",
    )
    composio = tmp_path / "mcp-composio.toml"
    composio.write_text(
        """
endpoint = "https://connect.composio.dev/mcp"
secret-key = "mcp/composio"
enabled-toolkits = ["Linear", "slack", "linear"]
""",
        encoding="utf-8",
    )

    loaded = load_blueberry_project_config(config, composio_path=composio)

    assert loaded.mcp_servers["composio-linear"].url == "https://connect.composio.dev/mcp"
    assert loaded.mcp_servers["composio-linear"].auth == "mcp/composio"
    assert loaded.mcp_servers["composio-linear"].toolkits == ("linear",)
    assert loaded.mcp_servers["composio-slack"].toolkits == ("slack",)
    assert set(loaded.enabled_mcp_servers()) == {
        "tempyr",
        "composio-linear",
        "composio-slack",
    }


def test_rejects_composio_sidecar_name_conflict(tmp_path: Path) -> None:
    config = tmp_path / "blueberry.toml"
    config.write_text(
        """
name = "blueberry"

[mcp-servers]
composio-linear = { url = "https://example.invalid/mcp", enabled = true }
""",
        encoding="utf-8",
    )
    composio = tmp_path / "mcp-composio.toml"
    composio.write_text(
        """
secret-key = "mcp/composio"
enabled-toolkits = ["linear"]
""",
        encoding="utf-8",
    )

    with pytest.raises(TypeError, match="composio-linear"):
        load_blueberry_project_config(config, composio_path=composio)


def test_rejects_non_blueberry_project_name(tmp_path: Path) -> None:
    config = tmp_path / "strawberry.toml"
    config.write_text('name = "strawberry"\n', encoding="utf-8")

    with pytest.raises(ValidationError):
        load_blueberry_project_config(config)


def test_rejects_malformed_mcp_server_table(tmp_path: Path) -> None:
    config = tmp_path / "blueberry.toml"
    config.write_text(
        """
name = "blueberry"

[mcp-servers]
tempyr = "stdio:tempyr --mcp"
""",
        encoding="utf-8",
    )

    with pytest.raises(TypeError, match="mcp server 'tempyr'"):
        load_blueberry_project_config(config)
