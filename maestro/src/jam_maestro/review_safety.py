"""Deterministic safety checks for outside-authored review text."""

from __future__ import annotations

from enum import StrEnum
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from jam_maestro.untrusted import Untrusted


class ReviewSafetyLabel(StrEnum):
    """Safety label for a review artifact body."""

    normal = "normal"
    suspicious_prompt_injection = "suspicious-prompt-injection"


PROMPT_INJECTION_PHRASES: tuple[str, ...] = (
    "ignore previous instructions",
    "ignore all previous instructions",
    "disregard previous instructions",
    "merge this pr",
    "run this command",
    "exfiltrate",
)


def classify_review_body(body: Untrusted) -> ReviewSafetyLabel:
    """Classify outside-authored review text without treating it as instruction."""
    normalized = str(body).casefold()
    if any(phrase in normalized for phrase in PROMPT_INJECTION_PHRASES):
        return ReviewSafetyLabel.suspicious_prompt_injection
    return ReviewSafetyLabel.normal
