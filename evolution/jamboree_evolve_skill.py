#!/usr/bin/env python3
"""Run the vendored Hermes self-evolution pipeline for one Jamboree skill.

The adapter preserves the §17.1 subprocess boundary. It prepares a temporary
Hermes-compatible skill tree, invokes the vendored `evolution.skills.evolve_skill`
module in a subprocess, then writes a unified diff for human review.
"""

from __future__ import annotations

import argparse
import difflib
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VENDOR_DIR = ROOT / "evolution" / "hermes-agent-self-evolution"
DEFAULT_CANDIDATE_DIR = Path(os.environ.get("JAM_HOME", str(Path.home() / ".jam"))) / (
    "skills-evolution-candidates"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run Hermes self-evolution for a Jamboree skill and write a candidate diff.",
    )
    parser.add_argument("--skill-path", required=True, help="Path to the Jamboree skill markdown file")
    parser.add_argument(
        "--candidate-dir",
        default=str(DEFAULT_CANDIDATE_DIR),
        help="Directory where <skill>.diff candidates are written",
    )
    parser.add_argument("--dataset-path", help="Existing golden/session eval dataset path")
    parser.add_argument(
        "--eval-source",
        default="golden",
        choices=["golden", "synthetic", "sessiondb"],
        help="Hermes evolution eval source",
    )
    parser.add_argument("--iterations", type=int, default=10, help="GEPA/MIPRO iteration count")
    parser.add_argument("--optimizer-model", default="openai/gpt-4.1")
    parser.add_argument("--eval-model", default="openai/gpt-4.1-mini")
    parser.add_argument("--run-tests", action="store_true", help="Enable upstream pytest constraint gate")
    parser.add_argument("--dry-run", action="store_true", help="Validate setup without LLM optimization")
    parser.add_argument("--work-dir", help="Temporary working directory to reuse for debugging")
    return parser.parse_args()


def parse_frontmatter(raw: str) -> tuple[str, str]:
    if not raw.lstrip().startswith("---"):
        return "", raw
    match = re.match(r"\A---\s*\n(.*?)\n---\s*\n?(.*)\Z", raw, re.DOTALL)
    if not match:
        return "", raw
    return match.group(1).strip(), match.group(2).strip()


def frontmatter_value(frontmatter: str, key: str) -> str | None:
    for line in frontmatter.splitlines():
        stripped = line.strip()
        if stripped.startswith(f"{key}:"):
            return stripped.split(":", 1)[1].strip().strip("'\"")
    return None


def first_heading(body: str) -> str | None:
    for line in body.splitlines():
        stripped = line.strip()
        if stripped.startswith("#"):
            return stripped.lstrip("#").strip()
    return None


def slugify(value: str) -> str:
    slug = re.sub(r"[^a-zA-Z0-9]+", "-", value.strip().lower()).strip("-")
    return slug or "skill"


def hermes_skill_text(original: str, source_path: Path) -> tuple[str, str]:
    frontmatter, body = parse_frontmatter(original)
    scope = frontmatter_value(frontmatter, "scope")
    name = slugify(scope or source_path.stem)
    description = first_heading(body) or f"Jamboree skill {scope or source_path.stem}"
    hermes_frontmatter = "\n".join(
        [
            f"name: {name}",
            f"description: {description}",
            "metadata:",
            "  jamboree:",
            f"    source_path: {source_path}",
        ]
    )
    return f"---\n{hermes_frontmatter}\n---\n\n{body.strip()}\n", name


def newest_output_dir(work_dir: Path, skill_name: str) -> Path:
    output_root = work_dir / "output" / skill_name
    if not output_root.exists():
        raise RuntimeError(f"expected Hermes output under {output_root}")
    candidates = [path for path in output_root.iterdir() if path.is_dir()]
    if not candidates:
        raise RuntimeError(f"no timestamped output directory under {output_root}")
    return max(candidates, key=lambda path: path.stat().st_mtime)


def reassemble_original(original: str, evolved_hermes: str) -> str:
    original_frontmatter, _ = parse_frontmatter(original)
    _, evolved_body = parse_frontmatter(evolved_hermes)
    if original_frontmatter:
        return f"---\n{original_frontmatter}\n---\n\n{evolved_body.strip()}\n"
    return f"{evolved_body.strip()}\n"


def run_hermes(args: argparse.Namespace, hermes_repo: Path, work_dir: Path, skill_name: str) -> None:
    command = [
        sys.executable,
        "-m",
        "evolution.skills.evolve_skill",
        "--skill",
        skill_name,
        "--iterations",
        str(args.iterations),
        "--eval-source",
        args.eval_source,
        "--optimizer-model",
        args.optimizer_model,
        "--eval-model",
        args.eval_model,
        "--hermes-repo",
        str(hermes_repo),
    ]
    if args.dataset_path:
        command.extend(["--dataset-path", str(Path(args.dataset_path).resolve())])
    if args.run_tests:
        command.append("--run-tests")
    if args.dry_run:
        command.append("--dry-run")

    env = os.environ.copy()
    env["PYTHONPATH"] = f"{VENDOR_DIR}:{env.get('PYTHONPATH', '')}".rstrip(":")

    result = subprocess.run(command, cwd=work_dir, env=env, text=True)
    if result.returncode != 0:
        raise SystemExit(result.returncode)


def main() -> int:
    args = parse_args()
    skill_path = Path(args.skill_path).expanduser().resolve()
    if not skill_path.exists():
        raise SystemExit(f"skill path does not exist: {skill_path}")
    if not VENDOR_DIR.exists():
        raise SystemExit(f"vendored Hermes self-evolution directory is missing: {VENDOR_DIR}")

    original = skill_path.read_text()
    hermes_text, skill_name = hermes_skill_text(original, skill_path)
    managed_tmp = args.work_dir is None
    work_dir = Path(args.work_dir).expanduser().resolve() if args.work_dir else Path(tempfile.mkdtemp())

    try:
        hermes_repo = work_dir / "hermes-agent"
        target_skill = hermes_repo / "skills" / "jamboree" / skill_name / "SKILL.md"
        target_skill.parent.mkdir(parents=True, exist_ok=True)
        target_skill.write_text(hermes_text)

        run_hermes(args, hermes_repo, work_dir, skill_name)
        if args.dry_run:
            return 0

        output_dir = newest_output_dir(work_dir, skill_name)
        evolved = (output_dir / "evolved_skill.md").read_text()
        evolved_original = reassemble_original(original, evolved)
        rel_path = skill_path.relative_to(ROOT) if skill_path.is_relative_to(ROOT) else skill_path
        diff = "".join(
            difflib.unified_diff(
                original.splitlines(keepends=True),
                evolved_original.splitlines(keepends=True),
                fromfile=f"a/{rel_path}",
                tofile=f"b/{rel_path}",
            )
        )
        if not diff:
            raise SystemExit("Hermes produced no skill diff")

        candidate_dir = Path(args.candidate_dir).expanduser().resolve()
        candidate_dir.mkdir(parents=True, exist_ok=True)
        candidate_path = candidate_dir / f"{skill_name}.diff"
        candidate_path.write_text(diff)
        print(candidate_path)
        return 0
    finally:
        if managed_tmp:
            shutil.rmtree(work_dir, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
