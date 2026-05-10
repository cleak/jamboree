"""Generated Pydantic models for tool service I/O."""

from __future__ import annotations

from typing import Any

from pydantic import BaseModel, ConfigDict, Field


class StrictToolModel(BaseModel):
    """Base for closed tool contracts."""

    model_config = ConfigDict(extra="forbid", frozen=True, populate_by_name=True)


class FlexibleToolModel(BaseModel):
    """Base for open response contracts with service-owned extra fields."""

    model_config = ConfigDict(extra="allow", frozen=True, populate_by_name=True)


class SearchWebCrawlRequest(StrictToolModel):
    """search.web-crawl request."""

    root_url: str = Field(min_length=1)
    max_depth: int = Field(ge=0)
    max_pages: int | None = Field(default=None, ge=1)
    render_js: bool | None = None
    include_images: bool | None = None


class SearchWebCrawlResponse(StrictToolModel):
    """search.web-crawl response."""

    root_url: str
    pages: list[Any]
    routing: dict[str, Any]
    trace_id: str


class SearchWebExtractRequest(StrictToolModel):
    """search.web-extract request."""

    urls: list[Any]
    render_js: bool | None = None
    include_images: bool | None = None


class SearchWebExtractResponse(StrictToolModel):
    """search.web-extract response."""

    contents: list[Any]
    routing: dict[str, Any]
    trace_id: str


class SearchWebSearchRequest(StrictToolModel):
    """search.web-search request."""

    query: str = Field(min_length=1)
    intent: str | None = Field(default=None, min_length=1)
    time_range: str | None = Field(default=None, min_length=1)
    domains: list[Any] | None = None


__all__ = [
    "SearchWebCrawlRequest",
    "SearchWebCrawlResponse",
    "SearchWebExtractRequest",
    "SearchWebExtractResponse",
    "SearchWebSearchRequest",
]
