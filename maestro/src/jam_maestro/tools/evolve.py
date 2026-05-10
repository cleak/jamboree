"""Generated Pydantic models for tool service I/O."""

from __future__ import annotations

from pydantic import BaseModel, ConfigDict, Field


class StrictToolModel(BaseModel):
    """Base for closed tool contracts."""

    model_config = ConfigDict(extra="forbid", frozen=True, populate_by_name=True)


class FlexibleToolModel(BaseModel):
    """Base for open response contracts with service-owned extra fields."""

    model_config = ConfigDict(extra="allow", frozen=True, populate_by_name=True)


class EvolveRequestSkillEvolutionRequest(StrictToolModel):
    """evolve.request-skill-evolution request."""

    skill_name: str = Field(min_length=1)
    eval_source: str | None = Field(default=None, min_length=1)
    reason: str | None = Field(default=None, min_length=1)


__all__ = [
    "EvolveRequestSkillEvolutionRequest",
]
