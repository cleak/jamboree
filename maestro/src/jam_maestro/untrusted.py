"""Type markers for content outside the trusted control boundary."""

from typing import NewType, cast

Untrusted = NewType("Untrusted", str)
TrustedText = NewType("TrustedText", str)


def mark_untrusted(value: str) -> Untrusted:
    """Mark outside-authored content as untrusted."""
    return Untrusted(value)


def trust_after_review(value: Untrusted) -> TrustedText:
    """Cross the trust boundary after local review or redaction."""
    return TrustedText(cast("str", value))
