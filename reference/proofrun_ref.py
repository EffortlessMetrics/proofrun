#!/usr/bin/env python3
from __future__ import annotations

import argparse
import dataclasses
import datetime as dt
import fnmatch
import hashlib
import json
import os
import re
import shlex
import subprocess
import sys
import textwrap
import tomllib
from pathlib import Path
from typing import Any

TOOL_VERSION = "0.1.0-ref"


DEFAULT_CONFIG_TOML = """
version = 1

[defaults]
output_dir = ".proofrun"

[profiles.local]
always = ["workspace:smoke"]

[profiles.ci]
always = ["workspace:smoke"]

[[surface]]
id = "tests.pkg"
covers = ["pkg:{pkg}:tests"]
cost = 3
run = ["cargo", "nextest", "run", "--profile", "{profile}", "-E", "package({pkg})"]

[[surface]]
id = "tests.rdeps"
covers = ["pkg:{pkg}:rdeps"]
cost = 8
run = ["cargo", "nextest", "run", "--profile", "{profile}", "-E", "rdeps({pkg})"]

[[surface]]
id = "workspace.all-tests"
covers = ["pkg:*:tests"]
cost = 10
run = ["cargo", "nextest", "run", "--profile", "{profile}", "--workspace"]

[[surface]]
id = "mutation.diff"
covers = ["pkg:{pkg}:mutation-diff"]
cost = 13
run = ["cargo", "mutants", "--in-diff", "{artifacts.diff_patch}", "--package", "{pkg}"]

[[surface]]
id = "workspace.docs"
covers = ["workspace:docs"]
cost = 4
run = ["cargo", "doc", "--workspace", "--no-deps"]

[[surface]]
id = "workspace.smoke"
covers = ["workspace:smoke"]
cost = 2
run = ["cargo", "test", "--workspace", "--quiet"]

[[rule]]
when.paths = ["crates/*/src/**/*.rs", "crates/*/tests/**/*.rs"]
emit = ["pkg:{owner}:tests", "pkg:{owner}:mutation-diff"]

[[rule]]
when.paths = ["**/Cargo.toml", "Cargo.lock", ".cargo/**", "**/build.rs"]
emit = ["pkg:{owner}:tests", "pkg:{owner}:rdeps", "workspace:smoke"]

[[rule]]
when.paths = ["docs/**", "book/**", "**/*.md"]
emit = ["workspace:docs"]

[unknown]
fallback = ["workspace:smoke"]
mode = "fail-closed"
"""


@dataclasses.dataclass(frozen=True)
class Package:
    name: str
    rel_dir: str
    manifest: str
    dependencies: tuple[str, ...]


@dataclasses.dataclass(frozen=True)
class ChangedPath:
    path: str
    status: str
    owner: str | None


@dataclasses.dataclass(frozen=True)
class CandidateSurface:
    id: str
    template: str
    cost: float
    covers: tuple[str, ...]
    run: tuple[str, ...]


class ProofrunError(RuntimeError):
    pass


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def canonical_json(data: Any) -> str:
    return json.dumps(data, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def repo_root_from(path: Path) -> Path:
    result = subprocess.run(
        ["git", "-C", str(path), "rev-parse", "--show-toplevel"],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise ProofrunError(result.stderr.strip() or f"failed to locate git repo from {path}")
    return Path(result.stdout.strip())


def load_config(repo_root: Path) -> dict[str, Any]:
    config_path = repo_root / "proofrun.toml"
    if config_path.exists():
        raw = config_path.read_text(encoding="utf-8")
    else:
        raw = DEFAULT_CONFIG_TOML
    data = tomllib.loads(raw)
    data.setdefault("defaults", {})
    data["defaults"].setdefault("output_dir", ".proofrun")
    data.setdefault("profiles", {})
    data.setdefault("unknown", {})
    data["unknown"].setdefault("fallback", [])
    data["unknown"].setdefault("mode", "fail-closed")
    return data


def scan_workspace(repo_root: Path) -> tuple[dict[str, Package], dict[str, list[str]]]:
    package_candidates: list[tuple[Path, dict[str, Any]]] = []
    ignored = {".git", "target", ".proofrun", "__pycache__"}
    for current, dirs, files in os.walk(repo_root):
        dirs[:] = [d for d in dirs if d not in ignored]
        current_path = Path(current)
        if "Cargo.toml" not in files:
            continue
        manifest = current_path / "Cargo.toml"
        try:
            parsed = tomllib.loads(manifest.read_text(encoding="utf-8"))
        except Exception:
            continue
        if "package" in parsed and isinstance(parsed["package"], dict) and "name" in parsed["package"]:
            package_candidates.append((manifest, parsed))

    packages: dict[str, Package] = {}
    for manifest, parsed in package_candidates:
        name = str(parsed["package"]["name"])
        rel_dir = manifest.parent.relative_to(repo_root).as_posix() or "."
        deps = _workspace_dependency_names(repo_root, manifest, parsed)
        packages[name] = Package(
            name=name,
            rel_dir=rel_dir,
            manifest=manifest.relative_to(repo_root).as_posix(),
            dependencies=tuple(sorted(deps)),
        )

    reverse_deps: dict[str, list[str]] = {name: [] for name in packages}
    for pkg in packages.values():
        for dep in pkg.dependencies:
            if dep in reverse_deps:
                reverse_deps[dep].append(pkg.name)
    for name in reverse_deps:
        reverse_deps[name].sort()
    return packages, reverse_deps


def _workspace_dependency_names(repo_root: Path, manifest: Path, parsed: dict[str, Any]) -> set[str]:
    results: set[str] = set()
    rel_manifest_dir = manifest.parent
    dep_sections = [
        parsed.get("dependencies", {}),
        parsed.get("dev-dependencies", {}),
        parsed.get("build-dependencies", {}),
        parsed.get("target", {}),
    ]
    for section in dep_sections:
        if not isinstance(section, dict):
            continue
        if "cfg" in section:
            for inner in section.values():
                if isinstance(inner, dict):
                    _collect_dep_entries(repo_root, rel_manifest_dir, inner, results)
        else:
            _collect_dep_entries(repo_root, rel_manifest_dir, section, results)
    return results


def _collect_dep_entries(repo_root: Path, manifest_dir: Path, dep_map: dict[str, Any], results: set[str]) -> None:
    for dep_name, dep_value in dep_map.items():
        if isinstance(dep_value, str):
            results.add(dep_name)
        elif isinstance(dep_value, dict):
            package_name = dep_value.get("package", dep_name)
            if "path" in dep_value:
                dep_path = (manifest_dir / dep_value["path"]).resolve()
                try:
                    dep_manifest = dep_path / "Cargo.toml"
                    if dep_manifest.exists():
                        parsed = tomllib.loads(dep_manifest.read_text(encoding="utf-8"))
                        if isinstance(parsed.get("package"), dict) and "name" in parsed["package"]:
                            package_name = parsed["package"]["name"]
                except Exception:
                    pass
            results.add(str(package_name))


def owner_for_path(path: str, packages: dict[str, Package]) -> str | None:
    normalized = path.strip("/")
    best: tuple[int, str] | None = None
    for pkg in packages.values():
        prefix = pkg.rel_dir.strip("./")
        if not prefix or prefix == ".":
            continue
        if normalized == prefix or normalized.startswith(prefix + "/"):
            score = len(prefix)
            if best is None or score > best[0]:
                best = (score, pkg.name)
    return best[1] if best else None


def collect_git_changes(repo_root: Path, base: str, head: str) -> tuple[str, list[ChangedPath], str]:
    merge_base = _git_capture(repo_root, "merge-base", base, head).strip()
    name_status = _git_capture(repo_root, "diff", "--name-status", merge_base, head)
    changes: list[ChangedPath] = []
    for line in name_status.splitlines():
        if not line.strip():
            continue
        parts = line.split("\t")
        status = parts[0]
        if status.startswith(("R", "C")) and len(parts) >= 3:
            path = parts[2]
            status = status[0]
        elif len(parts) >= 2:
            path = parts[1]
        else:
            continue
        changes.append(ChangedPath(path=path, status=status, owner=None))
    return merge_base, changes, _git_capture(repo_root, "diff", "--binary", merge_base, head)


def _git_capture(repo_root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo_root), *args],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise ProofrunError(result.stderr.strip() or f"git {' '.join(args)} failed")
    return result.stdout


def glob_to_regex(pattern: str) -> re.Pattern[str]:
    pattern = pattern.strip("/")
    i = 0
    out = ["^"]
    while i < len(pattern):
        if pattern.startswith("**/", i):
            out.append("(?:.*/)?")
            i += 3
            continue
        if pattern.startswith("**", i):
            out.append(".*")
            i += 2
            continue
        ch = pattern[i]
        if ch == "*":
            out.append("[^/]*")
        elif ch == "?":
            out.append("[^/]")
        else:
            out.append(re.escape(ch))
        i += 1
    out.append("$")
    return re.compile("".join(out))


def match_path(path: str, pattern: str) -> bool:
    path = path.strip("/")
    return bool(glob_to_regex(pattern).match(path))


def expand_template(template: str, values: dict[str, str]) -> str:
    def repl(match: re.Match[str]) -> str:
        key = match.group(1)
        if key not in values:
            raise ProofrunError(f"missing template value for {key!r} in {template!r}")
        return values[key]
    return re.sub(r"\{([A-Za-z0-9_.-]+)\}", repl, template)


def derive_obligations(
    config: dict[str, Any],
    profile: str,
    changes: list[ChangedPath],
    packages: dict[str, Package],
) -> tuple[dict[str, list[dict[str, Any]]], list[str]]:
    obligations: dict[str, list[dict[str, Any]]] = {}
    diagnostics: list[str] = []

    def add_obligation(obligation_id: str, reason: dict[str, Any]) -> None:
        obligations.setdefault(obligation_id, []).append(reason)

    for change in changes:
        owner = owner_for_path(change.path, packages)
        object.__setattr__(change, "owner", owner)
        for idx, rule in enumerate(config.get("rule", []), start=1):
            patterns = list(rule.get("when", {}).get("paths", []))
            matched_pattern = next((pat for pat in patterns if match_path(change.path, pat)), None)
            if matched_pattern is None:
                continue
            for emit in rule.get("emit", []):
                if "{owner}" in emit and not owner:
                    diagnostics.append(f"unowned path {change.path} matched rule {idx}")
                    if config.get("unknown", {}).get("mode") == "fail-closed":
                        for fallback in config.get("unknown", {}).get("fallback", []):
                            add_obligation(
                                fallback,
                                {
                                    "source": "unknown-fallback",
                                    "path": change.path,
                                    "rule": f"rule:{idx}",
                                    "pattern": matched_pattern,
                                },
                            )
                    continue
                values = {"owner": owner or ""}
                obligation_id = expand_template(emit, values)
                add_obligation(
                    obligation_id,
                    {
                        "source": "rule",
                        "path": change.path,
                        "rule": f"rule:{idx}",
                        "pattern": matched_pattern,
                    },
                )

    always = list(config.get("profiles", {}).get(profile, {}).get("always", []))
    for obligation_id in always:
        add_obligation(obligation_id, {"source": "profile", "path": None, "rule": profile, "pattern": None})

    if not obligations and config.get("unknown", {}).get("mode") == "fail-closed":
        for fallback in config.get("unknown", {}).get("fallback", []):
            add_obligation(fallback, {"source": "empty-range-fallback", "path": None, "rule": None, "pattern": None})

    return obligations, diagnostics


def candidate_bindings(obligations: list[str]) -> list[dict[str, str]]:
    bindings: list[dict[str, str]] = [{}]
    pkgs = sorted({ob.split(":")[1] for ob in obligations if ob.startswith("pkg:") and len(ob.split(":")) >= 3})
    bindings.extend({"pkg": pkg} for pkg in pkgs)
    return bindings


def build_candidates(
    config: dict[str, Any],
    obligations: list[str],
    profile: str,
    output_dir: Path,
) -> list[CandidateSurface]:
    bindings = candidate_bindings(obligations)
    candidates: list[CandidateSurface] = []
    for surface in config.get("surface", []):
        has_pkg = any("{pkg}" in s for s in [surface.get("id", ""), *surface.get("covers", []), *surface.get("run", [])])
        active_bindings = [b for b in bindings if ("pkg" in b)] if has_pkg else [{}]
        if has_pkg and not active_bindings:
            continue
        for binding in active_bindings:
            values = {
                "profile": profile,
                "artifacts.diff_patch": str((output_dir / "diff.patch").as_posix()),
                **binding,
            }
            cover_patterns = [expand_template(pattern, values) for pattern in surface.get("covers", [])]
            covered = sorted({ob for ob in obligations if any(fnmatch.fnmatch(ob, pattern) for pattern in cover_patterns)})
            if not covered:
                continue
            run = tuple(expand_template(str(arg), values) for arg in surface.get("run", []))
            base_id = str(surface.get("id"))
            surface_id = base_id if not binding else f"{base_id}[{','.join(f'{k}={v}' for k, v in sorted(binding.items()))}]"
            candidates.append(
                CandidateSurface(
                    id=surface_id,
                    template=base_id,
                    cost=float(surface.get("cost", 0)),
                    covers=tuple(covered),
                    run=run,
                )
            )
    dedup: dict[tuple[str, tuple[str, ...]], CandidateSurface] = {}
    for candidate in candidates:
        key = (candidate.id, candidate.covers)
        dedup[key] = candidate
    return sorted(dedup.values(), key=lambda c: (c.cost, -len(c.covers), c.id))


def solve_exact_cover(obligations: list[str], candidates: list[CandidateSurface]) -> list[CandidateSurface]:
    remaining = set(obligations)
    by_obligation: dict[str, list[CandidateSurface]] = {ob: [] for ob in obligations}
    for candidate in candidates:
        for ob in candidate.covers:
            if ob in by_obligation:
                by_obligation[ob].append(candidate)
    for ob, covering in by_obligation.items():
        if not covering:
            raise ProofrunError(f"no candidate surface covers obligation {ob}")

    best_choice: list[CandidateSurface] | None = None
    best_cost: tuple[float, int, tuple[str, ...]] | None = None

    def recurse(remaining_obs: set[str], chosen: list[CandidateSurface], chosen_ids: set[str], cost: float) -> None:
        nonlocal best_choice, best_cost
        if not remaining_obs:
            signature = tuple(sorted(candidate.id for candidate in chosen))
            candidate_score = (cost, len(chosen), signature)
            if best_cost is None or candidate_score < best_cost:
                best_cost = candidate_score
                best_choice = sorted(chosen, key=lambda c: c.id)
            return

        if best_cost is not None and (cost, len(chosen)) >= best_cost[:2]:
            return

        target = min(
            remaining_obs,
            key=lambda ob: (len(by_obligation[ob]), ob),
        )
        options = sorted(by_obligation[target], key=lambda c: (c.cost, -len(c.covers), c.id))
        for candidate in options:
            if candidate.id in chosen_ids:
                continue
            new_remaining = remaining_obs.difference(candidate.covers)
            recurse(new_remaining, chosen + [candidate], chosen_ids | {candidate.id}, cost + candidate.cost)

    recurse(remaining, [], set(), 0.0)
    if best_choice is None:
        raise ProofrunError("failed to solve proof plan")
    return best_choice


def build_plan(
    repo_root: Path,
    base: str,
    head: str,
    profile: str,
    config: dict[str, Any],
) -> dict[str, Any]:
    packages, reverse_deps = scan_workspace(repo_root)
    merge_base, changes, patch = collect_git_changes(repo_root, base, head)
    output_dir = repo_root / config.get("defaults", {}).get("output_dir", ".proofrun")
    output_dir.mkdir(parents=True, exist_ok=True)
    diff_patch_path = output_dir / "diff.patch"
    diff_patch_path.write_text(patch, encoding="utf-8")

    obligations_map, diagnostics = derive_obligations(config, profile, changes, packages)
    obligations = sorted(obligations_map)
    candidates = build_candidates(config, obligations, profile, output_dir)
    selected = solve_exact_cover(obligations, candidates)
    selected_ids = {candidate.id for candidate in selected}

    omitted = [
        {"id": candidate.id, "reason": "not selected by optimal weighted cover"}
        for candidate in candidates
        if candidate.id not in selected_ids
    ]

    config_digest = sha256_text(canonical_json(config))
    plan = {
        "version": TOOL_VERSION,
        "created_at": utc_now(),
        "repo_root": str(repo_root),
        "base": base,
        "head": head,
        "merge_base": merge_base,
        "profile": profile,
        "config_digest": config_digest,
        "artifacts": {
            "output_dir": str(output_dir.relative_to(repo_root).as_posix()),
            "diff_patch": str(diff_patch_path.relative_to(repo_root).as_posix()),
            "plan_json": str((output_dir / "plan.json").relative_to(repo_root).as_posix()),
            "plan_markdown": str((output_dir / "plan.md").relative_to(repo_root).as_posix()),
            "commands_shell": str((output_dir / "commands.sh").relative_to(repo_root).as_posix()),
            "github_actions": str((output_dir / "github-actions.yml").relative_to(repo_root).as_posix()),
        },
        "workspace": {
            "packages": [
                {
                    "name": pkg.name,
                    "dir": pkg.rel_dir,
                    "manifest": pkg.manifest,
                    "dependencies": list(pkg.dependencies),
                    "reverse_dependencies": reverse_deps.get(pkg.name, []),
                }
                for pkg in sorted(packages.values(), key=lambda p: p.name)
            ]
        },
        "changed_paths": [
            {"path": change.path, "status": change.status, "owner": change.owner}
            for change in sorted(changes, key=lambda c: (c.path, c.status))
        ],
        "obligations": [
            {
                "id": obligation_id,
                "reasons": sorted(
                    obligations_map[obligation_id],
                    key=lambda r: ((r.get("path") or ""), (r.get("rule") or ""), (r.get("pattern") or "")),
                ),
            }
            for obligation_id in obligations
        ],
        "selected_surfaces": [
            {
                "id": candidate.id,
                "template": candidate.template,
                "cost": candidate.cost,
                "covers": list(candidate.covers),
                "run": list(candidate.run),
            }
            for candidate in selected
        ],
        "omitted_surfaces": sorted(omitted, key=lambda item: item["id"]),
        "diagnostics": diagnostics,
    }
    plan["plan_digest"] = sha256_text(canonical_json({k: v for k, v in plan.items() if k != "plan_digest"}))
    return plan


def plan_markdown(plan: dict[str, Any]) -> str:
    lines = []
    lines.append("# proofrun plan")
    lines.append("")
    lines.append(f"- range: `{plan['base']}..{plan['head']}`")
    lines.append(f"- merge base: `{plan['merge_base']}`")
    lines.append(f"- profile: `{plan['profile']}`")
    lines.append(f"- plan digest: `{plan['plan_digest']}`")
    lines.append("")
    lines.append("## Changed paths")
    lines.append("")
    for change in plan["changed_paths"]:
        owner = change["owner"] or "unowned"
        lines.append(f"- `{change['status']}` `{change['path']}` → `{owner}`")
    lines.append("")
    lines.append("## Obligations")
    lines.append("")
    for obligation in plan["obligations"]:
        lines.append(f"- `{obligation['id']}`")
        for reason in obligation["reasons"]:
            source = reason.get("source")
            path = reason.get("path")
            rule = reason.get("rule")
            pattern = reason.get("pattern")
            lines.append(f"  - source={source}, path={path}, rule={rule}, pattern={pattern}")
    lines.append("")
    lines.append("## Selected surfaces")
    lines.append("")
    for surface in plan["selected_surfaces"]:
        lines.append(f"- `{surface['id']}` — cost `{surface['cost']}`")
        lines.append(f"  - covers: {', '.join(surface['covers'])}")
        lines.append(f"  - run: `{shlex.join(surface['run'])}`")
    if plan.get("diagnostics"):
        lines.append("")
        lines.append("## Diagnostics")
        lines.append("")
        for diagnostic in plan["diagnostics"]:
            lines.append(f"- {diagnostic}")
    return "\n".join(lines) + "\n"


def emit_shell(plan: dict[str, Any]) -> str:
    lines = ["#!/usr/bin/env bash", "set -euo pipefail", ""]
    for surface in plan["selected_surfaces"]:
        lines.append(f"# {surface['id']}")
        lines.append(shlex.join(surface["run"]))
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def emit_github_actions(plan: dict[str, Any]) -> str:
    lines = [
        "steps:",
        "  - name: Execute proof plan",
        "    run: |",
    ]
    for surface in plan["selected_surfaces"]:
        lines.append(f"      {shlex.join(surface['run'])}")
    return "\n".join(lines) + "\n"


def write_plan_artifacts(repo_root: Path, plan: dict[str, Any]) -> None:
    artifacts = plan["artifacts"]
    plan_json = repo_root / artifacts["plan_json"]
    plan_md = repo_root / artifacts["plan_markdown"]
    shell_path = repo_root / artifacts["commands_shell"]
    gha_path = repo_root / artifacts["github_actions"]
    plan_json.write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    plan_md.write_text(plan_markdown(plan), encoding="utf-8")
    shell_path.write_text(emit_shell(plan), encoding="utf-8")
    shell_path.chmod(0o755)
    gha_path.write_text(emit_github_actions(plan), encoding="utf-8")


def load_plan(repo_root: Path, plan_path: Path) -> dict[str, Any]:
    path = plan_path if plan_path.is_absolute() else (repo_root / plan_path)
    return json.loads(path.read_text(encoding="utf-8"))


def execute_plan(repo_root: Path, plan: dict[str, Any], dry_run: bool = False) -> dict[str, Any]:
    output_dir = repo_root / plan["artifacts"]["output_dir"]
    logs_dir = output_dir / "logs"
    logs_dir.mkdir(parents=True, exist_ok=True)

    steps: list[dict[str, Any]] = []
    overall_status = "dry-run" if dry_run else "passed"

    for index, surface in enumerate(plan["selected_surfaces"], start=1):
        surface_id = surface["id"]
        stdout_path = logs_dir / f"{index:02d}-{surface_id}.stdout.log"
        stderr_path = logs_dir / f"{index:02d}-{surface_id}.stderr.log"
        command = list(surface["run"])
        if dry_run:
            stdout_path.write_text("", encoding="utf-8")
            stderr_path.write_text("", encoding="utf-8")
            steps.append(
                {
                    "id": surface_id,
                    "argv": command,
                    "exit_code": 0,
                    "duration_ms": 0,
                    "stdout_path": str(stdout_path.relative_to(repo_root).as_posix()),
                    "stderr_path": str(stderr_path.relative_to(repo_root).as_posix()),
                }
            )
            continue

        started = dt.datetime.now(dt.timezone.utc)
        proc = subprocess.run(
            command,
            cwd=repo_root,
            text=True,
            capture_output=True,
            check=False,
        )
        elapsed_ms = int((dt.datetime.now(dt.timezone.utc) - started).total_seconds() * 1000)
        stdout_path.write_text(proc.stdout, encoding="utf-8")
        stderr_path.write_text(proc.stderr, encoding="utf-8")
        steps.append(
            {
                "id": surface_id,
                "argv": command,
                "exit_code": proc.returncode,
                "duration_ms": elapsed_ms,
                "stdout_path": str(stdout_path.relative_to(repo_root).as_posix()),
                "stderr_path": str(stderr_path.relative_to(repo_root).as_posix()),
            }
        )
        if proc.returncode != 0:
            overall_status = "failed"
            break

    receipt = {
        "version": TOOL_VERSION,
        "executed_at": utc_now(),
        "plan_digest": plan["plan_digest"],
        "status": overall_status,
        "steps": steps,
    }
    (output_dir / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return receipt


def doctor(repo_root: Path) -> dict[str, Any]:
    config_path = repo_root / "proofrun.toml"
    cargo_path = repo_root / "Cargo.toml"
    config = load_config(repo_root)
    packages, _ = scan_workspace(repo_root)
    issues: list[str] = []
    if not cargo_path.exists():
        issues.append("missing Cargo.toml")
    if not config_path.exists():
        issues.append("proofrun.toml missing; using built-in default config")
    if not packages:
        issues.append("no Cargo packages discovered")
    if not config.get("profiles"):
        issues.append("no profiles configured")
    if not config.get("surface"):
        issues.append("no surfaces configured")
    if not config.get("rule"):
        issues.append("no rules configured")
    return {
        "repo_root": str(repo_root),
        "config_path": str(config_path),
        "cargo_manifest_path": str(cargo_path),
        "package_count": len(packages),
        "packages": sorted(packages),
        "issues": issues,
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="proofrun-ref",
        description="Reference implementation of a deterministic proof-plan compiler for Cargo workspaces.",
    )
    parser.add_argument("--repo", default=".", help="Path inside the target Git repo.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    plan = subparsers.add_parser("plan", help="Compile a proof plan.")
    plan.add_argument("--base", required=True)
    plan.add_argument("--head", required=True)
    plan.add_argument("--profile", default="ci")

    explain = subparsers.add_parser("explain", help="Render a plan summary.")
    explain.add_argument("--plan", default=".proofrun/plan.json")

    emit = subparsers.add_parser("emit", help="Emit derived artifacts from an existing plan.")
    emit_sub = emit.add_subparsers(dest="emit_kind", required=True)
    emit_shell_parser = emit_sub.add_parser("shell")
    emit_shell_parser.add_argument("--plan", default=".proofrun/plan.json")
    emit_gha_parser = emit_sub.add_parser("github-actions")
    emit_gha_parser.add_argument("--plan", default=".proofrun/plan.json")

    run = subparsers.add_parser("run", help="Execute or dry-run a plan.")
    run.add_argument("--plan", default=".proofrun/plan.json")
    run.add_argument("--dry-run", action="store_true")

    subparsers.add_parser("doctor", help="Check repo readiness.")

    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    repo_input = Path(args.repo).resolve()

    if args.command == "doctor":
        try:
            repo_root = repo_root_from(repo_input)
        except ProofrunError:
            repo_root = repo_input
        print(json.dumps(doctor(repo_root), indent=2, sort_keys=True))
        return 0

    repo_root = repo_root_from(repo_input)
    config = load_config(repo_root)

    if args.command == "plan":
        plan = build_plan(repo_root, args.base, args.head, args.profile, config)
        write_plan_artifacts(repo_root, plan)
        print(json.dumps(plan, indent=2, sort_keys=True))
        return 0

    if args.command == "explain":
        plan = load_plan(repo_root, Path(args.plan))
        print(plan_markdown(plan), end="")
        return 0

    if args.command == "emit":
        plan = load_plan(repo_root, Path(getattr(args, "plan")))
        if args.emit_kind == "shell":
            print(emit_shell(plan), end="")
            return 0
        if args.emit_kind == "github-actions":
            print(emit_github_actions(plan), end="")
            return 0
        raise ProofrunError(f"unknown emit kind: {args.emit_kind}")

    if args.command == "run":
        plan = load_plan(repo_root, Path(args.plan))
        receipt = execute_plan(repo_root, plan, dry_run=args.dry_run)
        print(json.dumps(receipt, indent=2, sort_keys=True))
        return 0

    raise ProofrunError(f"unknown command {args.command}")


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except ProofrunError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(2)
