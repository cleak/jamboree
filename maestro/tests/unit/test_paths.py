from __future__ import annotations

import os
from pathlib import Path
from unittest.mock import patch

from jam_maestro.paths import MAESTRO_JAM_HOME, default_jam_home_for, jam_home


def test_default_jam_home_for_maestro() -> None:
    assert default_jam_home_for("maestro", Path("/home/maestro")) == MAESTRO_JAM_HOME


def test_default_jam_home_for_caleb() -> None:
    assert default_jam_home_for("caleb", Path("/home/caleb")) == Path("/home/caleb/.jam")


def test_jam_home_env_wins() -> None:
    with patch.dict(
        os.environ,
        {"JAM_HOME": "/home/caleb/custom-jam-home", "USER": "caleb", "HOME": "/home/caleb"},
        clear=True,
    ):
        assert jam_home() == Path("/home/caleb/custom-jam-home")


def test_jam_home_uses_home_for_cli_user() -> None:
    with patch.dict(os.environ, {"USER": "caleb", "HOME": "/home/caleb"}, clear=True):
        assert jam_home() == Path("/home/caleb/.jam")
