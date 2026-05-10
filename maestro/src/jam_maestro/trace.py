"""Trace ID helpers for Python-side Maestro sessions."""

from __future__ import annotations

import secrets
import time

CROCKFORD32 = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
TRACE_ID_LENGTH = 26


def new_trace_id() -> str:
    """Return a 26-character ULID-compatible trace id."""
    timestamp_ms = int(time.time() * 1000) & ((1 << 48) - 1)
    randomness = secrets.randbits(80)
    value = (timestamp_ms << 80) | randomness
    return "".join(CROCKFORD32[(value >> shift) & 0b11111] for shift in range(125, -1, -5))


def is_trace_id(value: str) -> bool:
    """Return whether `value` is a Jamboree trace ID."""
    return len(value) == TRACE_ID_LENGTH and all(char in CROCKFORD32 for char in value)
