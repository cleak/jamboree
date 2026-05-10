#!/usr/bin/env python3
"""Generate Maestro Pydantic models from tool JSON schemas."""

from __future__ import annotations

import argparse
import json
import keyword
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, NoReturn

ROOT = Path(__file__).resolve().parents[1]
SCHEMAS_ROOT = ROOT / "crates/jam-tools-core/schemas"
OUT_DIR = ROOT / "maestro/src/jam_maestro/tools"
SCHEMA_FILENAME_PARTS = 2
SCHEMA_KINDS = {"request", "response"}
JSON_TYPE_MAP = {
    "string": "str",
    "integer": "int",
    "number": "float",
    "boolean": "bool",
    "array": "list[Any]",
    "object": "dict[str, Any]",
}


@dataclass(frozen=True)
class SchemaModel:
    service: str
    tool: str
    kind: str
    class_name: str
    schema: dict[str, Any]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if generated files drift")
    args = parser.parse_args()
    models = load_models()
    rendered = render_files(models)
    return write_files(rendered, check=args.check)


def load_models() -> list[SchemaModel]:
    models: list[SchemaModel] = []
    for path in sorted(SCHEMAS_ROOT.glob("*/*.json")):
        service = path.parent.name
        name_parts = path.name.removesuffix(".json").rsplit(".", 1)
        if len(name_parts) != SCHEMA_FILENAME_PARTS:
            fail(f"schema filename must be <tool>.<request|response>.json: {path}")
        tool, kind = name_parts
        if kind not in SCHEMA_KINDS:
            fail(f"schema kind must be request or response: {path}")
        with path.open(encoding="utf-8") as fh:
            schema = json.load(fh)
        class_name = f"{pascal(service)}{pascal(tool)}{pascal(kind)}"
        models.append(SchemaModel(service, tool, kind, class_name, schema))
    return models


def render_files(models: list[SchemaModel]) -> dict[Path, str]:
    by_service: dict[str, list[SchemaModel]] = {}
    for model in models:
        by_service.setdefault(model.service, []).append(model)

    rendered: dict[Path, str] = {
        OUT_DIR / "__init__.py": render_init(sorted(by_service)),
    }
    for service, service_models in sorted(by_service.items()):
        rendered[OUT_DIR / f"{service}.py"] = render_service(service_models)
    return rendered


def render_init(services: list[str]) -> str:
    lines = [
        '"""Generated typed tool I/O models for Maestro."""',
        "",
        "from __future__ import annotations",
        "",
    ]
    exported: list[str] = []
    for service in services:
        module_models = load_service_class_names(service)
        exported.extend(module_models)
        lines.append(f"from jam_maestro.tools.{service} import (")
        lines.extend(f"    {name}," for name in module_models)
        lines.append(")")
    lines.extend(["", "__all__ = ["])
    lines.extend(f'    "{name}",' for name in sorted(exported))
    lines.extend(["]", ""])
    return "\n".join(lines)


def load_service_class_names(service: str) -> list[str]:
    names = []
    for path in sorted((SCHEMAS_ROOT / service).glob("*.json")):
        tool, kind = path.name.removesuffix(".json").rsplit(".", 1)
        names.append(f"{pascal(service)}{pascal(tool)}{pascal(kind)}")
    return sorted(names, key=str.lower)


def render_service(models: list[SchemaModel]) -> str:
    blocks = [render_model(model) for model in models]
    flat_blocks = "\n".join(line for block in blocks for line in block)
    needs_datetime = "datetime" in flat_blocks
    needs_any = "Any" in flat_blocks
    needs_literal = "Literal[" in flat_blocks
    lines = [
        '"""Generated Pydantic models for tool service I/O."""',
        "",
        "from __future__ import annotations",
        "",
    ]
    if needs_datetime:
        lines.append("from datetime import datetime  # noqa: TC003")
    typing_imports = []
    if needs_any:
        typing_imports.append("Any")
    if needs_literal:
        typing_imports.append("Literal")
    if typing_imports:
        lines.append(f"from typing import {', '.join(typing_imports)}")
    if needs_datetime or typing_imports:
        lines.append("")
    lines.extend(
        [
            "from pydantic import BaseModel, ConfigDict, Field",
            "",
            "",
            "class StrictToolModel(BaseModel):",
            '    """Base for closed tool contracts."""',
            "",
        '    model_config = ConfigDict(extra="forbid", frozen=True, populate_by_name=True)',
            "",
            "",
            "class FlexibleToolModel(BaseModel):",
            '    """Base for open response contracts with service-owned extra fields."""',
            "",
        '    model_config = ConfigDict(extra="allow", frozen=True, populate_by_name=True)',
            "",
        ]
    )
    exported: list[str] = []
    for model, block in zip(models, blocks, strict=True):
        if exported:
            lines.append("")
        exported.append(model.class_name)
        lines.extend(block)
    lines.extend(["", "", "__all__ = ["])
    lines.extend(f'    "{name}",' for name in sorted(exported))
    lines.extend(["]", ""])
    return "\n".join(lines)


def render_model(model: SchemaModel) -> list[str]:
    schema = model.schema
    base = "FlexibleToolModel" if schema.get("additionalProperties") is True else "StrictToolModel"
    lines = [
        "",
        f"class {model.class_name}({base}):",
        f'    """{schema.get("title", model.class_name)}."""',
        "",
    ]
    properties = schema.get("properties", {})
    if not isinstance(properties, dict) or not properties:
        return lines
    required = set(schema.get("required", []))
    for field_name, raw_field_schema in properties.items():
        python_name = python_field_name(field_name)
        field_schema = raw_field_schema if isinstance(raw_field_schema, dict) else {}
        annotation = python_type(field_schema)
        default = ""
        if field_name not in required:
            annotation = f"{annotation} | None"
            default = " = None"
        field_args = field_constraints(field_schema)
        if python_name != field_name:
            field_args.insert(0, f"alias={json.dumps(field_name)}")
        if field_args:
            joined_args = ", ".join(field_args)
            default = (
                f" = Field({joined_args})"
                if field_name in required
                else f" = Field(default=None, {joined_args})"
            )
        lines.append(f"    {python_name}: {annotation}{default}")
    return lines


def python_field_name(field_name: str) -> str:
    if keyword.iskeyword(field_name):
        return f"{field_name}_"
    return field_name


def python_type(schema: dict[str, Any]) -> str:
    if "enum" in schema and isinstance(schema["enum"], list):
        values = ", ".join(json.dumps(value) for value in schema["enum"])
        return f"Literal[{values}]"
    type_name = schema.get("type")
    if isinstance(type_name, list):
        non_null = [item for item in type_name if item != "null"]
        type_name = non_null[0] if non_null else "string"
    if type_name == "string" and schema.get("format") == "date-time":
        result = "datetime"
    else:
        result = JSON_TYPE_MAP.get(type_name, "Any") if isinstance(type_name, str) else "Any"
    return result


def field_constraints(schema: dict[str, Any]) -> list[str]:
    constraints: list[str] = []
    if "minLength" in schema:
        constraints.append(f"min_length={int(schema['minLength'])}")
    if "maxLength" in schema:
        constraints.append(f"max_length={int(schema['maxLength'])}")
    if "minimum" in schema:
        constraints.append(f"ge={schema['minimum']}")
    return constraints


def write_files(rendered: dict[Path, str], *, check: bool) -> int:
    drift: list[Path] = []
    if not check:
        OUT_DIR.mkdir(parents=True, exist_ok=True)
    expected = set(rendered)
    existing = set(OUT_DIR.glob("*.py")) if OUT_DIR.exists() else set()
    for path, content in rendered.items():
        old = path.read_text(encoding="utf-8") if path.exists() else None
        if old != content:
            drift.append(path)
            if not check:
                path.write_text(content, encoding="utf-8")
    for path in sorted(existing - expected):
        drift.append(path)
        if not check:
            path.unlink()
    if drift and check:
        write_stderr("pydantic-gen: out of sync. Run `python3 tools/pydantic-gen.py`:")
        for path in drift:
            write_stderr(f"  {path.relative_to(ROOT)}")
        return 1
    if not check:
        write_stdout(f"pydantic-gen: regenerated {len(rendered)} file(s)")
    return 0


def pascal(raw: str) -> str:
    return "".join(part.capitalize() for part in raw.replace("_", "-").split("-") if part)


def fail(message: str) -> NoReturn:
    write_stderr(message)
    raise SystemExit(1)


def write_stdout(message: str) -> None:
    sys.stdout.write(f"{message}\n")


def write_stderr(message: str) -> None:
    sys.stderr.write(f"{message}\n")


if __name__ == "__main__":
    raise SystemExit(main())
