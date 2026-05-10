"""Runtime path defaults shared by the Python Maestro."""

from __future__ import annotations

import getpass
import os
from pathlib import Path

MAESTRO_USER = "maestro"
MAESTRO_JAM_HOME = Path("/home/maestro/.jam")


def jam_home() -> Path:
    """Resolve `JAM_HOME` for this process per security-setup §7.1."""
    explicit = os.environ.get("JAM_HOME")
    if explicit:
        return Path(explicit)

    user = os.environ.get("USER") or os.environ.get("LOGNAME") or getpass.getuser()
    home = os.environ.get("HOME")
    return default_jam_home_for(user, Path(home) if home else None)


def default_jam_home_for(user: str, home: Path | None) -> Path:
    """Return the default `JAM_HOME` for an already-known user/home pair."""
    if user == MAESTRO_USER:
        return MAESTRO_JAM_HOME
    if home is None:
        return MAESTRO_JAM_HOME
    return home / ".jam"
