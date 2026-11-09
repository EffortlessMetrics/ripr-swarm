#!/usr/bin/env python3
"""Bound RIPR Cargo build artifacts without erasing ``target/ripr`` evidence.

Cargo intermediates live below
``{cargo-cache-home}/ripr-swarm-build/{workspace-path-hash}``. This tool reports
that managed root, garbage-collects stale worktree trees, or removes the exact
current worktree build tree before disposal. Deletion always requires
``--apply``.
"""

from __future__ import annotations

import argparse
import dataclasses
import datetime as dt
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import time
from pathlib import Path
from typing import Iterable, Iterator, Sequence

GB = 1_000_000_000
DAY = 86_400
DEFAULT_MAX_GB = 20
DEFAULT_TTL_DAYS = 14
DEFAULT_MIN_AGE_HOURS = 24
SHARD_RE = re.compile(r"^[0-9a-f]{2}$")
HASH_RE = re.compile(r"^[0-9a-f]{8,62}$")
REPORT_JSON = Path("target/ripr/reports/cargo-artifacts.json")
REPORT_MD = Path("target/ripr/reports/cargo-artifacts.md")


class ArtifactError(RuntimeError):
    pass


@dataclasses.dataclass(frozen=True)
class CargoMetadata:
    workspace_root: Path
    target_directory: Path
    build_directory: Path


@dataclasses.dataclass(frozen=True)
class Measurement:
    bytes: int
    files: int
    newest_mtime: float
    unsafe: tuple[str, ...]


@dataclasses.dataclass(frozen=True)
class BuildTree:
    path: Path
    relative: str
    bytes: int
    files: int
    newest_mtime: float
    age_seconds: float
    current: bool
    unsafe: tuple[str, ...]

    @property
    def eligible(self) -> bool:
        return not self.current and not self.unsafe


@dataclasses.dataclass(frozen=True)
class Selection:
    trees: tuple[BuildTree, ...]
    reasons: dict[str, tuple[str, ...]]
    projected_bytes: int
    unresolved_over_budget_bytes: int


def non_negative(value: str) -> int:
    parsed = int(value)
    if parsed < 0:
        raise argparse.ArgumentTypeError("must be non-negative")
    return parsed


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "command", nargs="?", choices=("report", "gc", "clean-current"), default="report"
    )
    parser.add_argument("--root", type=Path, help="Override managed root (recovery/tests).")
    parser.add_argument("--max-size-gb", type=non_negative, default=DEFAULT_MAX_GB)
    parser.add_argument("--ttl-days", type=non_negative, default=DEFAULT_TTL_DAYS)
    parser.add_argument("--min-age-hours", type=non_negative, default=DEFAULT_MIN_AGE_HOURS)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--no-receipt", action="store_true")
    parser.add_argument("--now", type=float, help=argparse.SUPPRESS)
    return parser.parse_args(argv)


def workspace_root(start: Path) -> Path:
    current = start.resolve()
    for candidate in (current, *current.parents):
        if (candidate / "Cargo.toml").is_file() and (candidate / ".cargo/config.toml").is_file():
            return candidate
    raise ArtifactError(f"run from the RIPR repository; no workspace found above {start}")


def cargo_home(env: dict[str, str] | None = None) -> Path:
    values = os.environ if env is None else env
    configured = values.get("CARGO_HOME", "").strip()
    if configured:
        return Path(configured).expanduser().resolve()
    home = values.get("HOME", "").strip() or values.get("USERPROFILE", "").strip()
    if not home:
        raise ArtifactError("CARGO_HOME, HOME, and USERPROFILE are unavailable")
    return (Path(home).expanduser() / ".cargo").resolve()


def managed_root(env: dict[str, str] | None = None) -> Path:
    return cargo_home(env) / "ripr-swarm-build"


def cargo_metadata(root: Path) -> CargoMetadata:
    try:
        result = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            cwd=root,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
        )
    except OSError as exc:
        raise ArtifactError(f"could not execute cargo metadata: {exc}") from exc
    if result.returncode:
        detail = result.stderr.strip() or result.stdout.strip() or "no diagnostics"
        raise ArtifactError(f"cargo metadata failed ({result.returncode}): {detail}")
    try:
        value = json.loads(result.stdout)
        fields = [value[name] for name in ("workspace_root", "target_directory", "build_directory")]
    except (json.JSONDecodeError, KeyError, TypeError) as exc:
        raise ArtifactError(
            "cargo metadata lacks build_directory; use the pinned Cargo toolchain"
        ) from exc
    if not all(isinstance(field, str) and field for field in fields):
        raise ArtifactError("cargo metadata returned empty artifact paths")
    return CargoMetadata(*(Path(field).resolve() for field in fields))


def is_link(info: os.stat_result) -> bool:
    attributes = getattr(info, "st_file_attributes", 0)
    reparse = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0)
    return stat.S_ISLNK(info.st_mode) or bool(reparse and attributes & reparse)


def require_plain_directory(path: Path, label: str, missing_ok: bool = True) -> None:
    try:
        info = path.lstat()
    except FileNotFoundError:
        if missing_ok:
            return
        raise ArtifactError(f"{label} does not exist: {path}")
    if is_link(info):
        raise ArtifactError(f"refusing symlink/reparse-point {label}: {path}")
    if not stat.S_ISDIR(info.st_mode):
        raise ArtifactError(f"{label} is not a directory: {path}")


def validate_root(root: Path) -> Path:
    resolved = root.expanduser().resolve(strict=False)
    if resolved.name != "ripr-swarm-build" or resolved == Path(resolved.anchor):
        raise ArtifactError(f"managed root must end in ripr-swarm-build: {resolved}")
    require_plain_directory(resolved, "managed root")
    return resolved


def within(path: Path, parent: Path) -> bool:
    try:
        path.resolve(strict=False).relative_to(parent.resolve(strict=False))
        return True
    except ValueError:
        return False


def build_leaves(root: Path) -> Iterator[Path]:
    if not root.exists():
        return
    for shard in sorted(root.iterdir(), key=lambda item: item.name):
        info = shard.lstat()
        if is_link(info) or not stat.S_ISDIR(info.st_mode) or not SHARD_RE.fullmatch(shard.name):
            continue
        for leaf in sorted(shard.iterdir(), key=lambda item: item.name):
            leaf_info = leaf.lstat()
            if (
                not is_link(leaf_info)
                and stat.S_ISDIR(leaf_info.st_mode)
                and HASH_RE.fullmatch(leaf.name)
            ):
                yield leaf


def measure_tree(root: Path) -> Measurement:
    size = 0
    files = 0
    newest = root.lstat().st_mtime
    unsafe: list[str] = []
    stack = [root]
    while stack:
        current = stack.pop()
        try:
            children = list(os.scandir(current))
        except OSError as exc:
            unsafe.append(f"{current}: {exc}")
            continue
        for child in children:
            path = Path(child.path)
            try:
                info = child.stat(follow_symlinks=False)
            except OSError as exc:
                unsafe.append(f"{path}: {exc}")
                continue
            newest = max(newest, info.st_mtime)
            if child.is_symlink() or is_link(info):
                unsafe.append(str(path))
            elif stat.S_ISDIR(info.st_mode):
                stack.append(path)
            elif stat.S_ISREG(info.st_mode):
                size += info.st_size
                files += 1
            else:
                unsafe.append(str(path))
    return Measurement(size, files, newest, tuple(sorted(unsafe)))


def inventory(root: Path, current_build: Path, now: float) -> list[BuildTree]:
    result: list[BuildTree] = []
    for leaf in build_leaves(root):
        measured = measure_tree(leaf)
        result.append(
            BuildTree(
                leaf,
                leaf.relative_to(root).as_posix(),
                measured.bytes,
                measured.files,
                measured.newest_mtime,
                max(0.0, now - measured.newest_mtime),
                within(current_build, leaf),
                measured.unsafe,
            )
        )
    return result


def select_gc(
    trees: Sequence[BuildTree], max_bytes: int, ttl_seconds: float, min_age_seconds: float
) -> Selection:
    selected: dict[str, BuildTree] = {}
    reasons: dict[str, set[str]] = {}

    def choose(tree: BuildTree, reason: str) -> None:
        selected[tree.relative] = tree
        reasons.setdefault(tree.relative, set()).add(reason)

    for tree in trees:
        if tree.eligible and tree.age_seconds >= ttl_seconds:
            choose(tree, "ttl")
    projected = sum(tree.bytes for tree in trees) - sum(tree.bytes for tree in selected.values())
    for tree in sorted(trees, key=lambda item: (item.newest_mtime, item.relative)):
        if projected <= max_bytes:
            break
        if tree.eligible and tree.relative not in selected and tree.age_seconds >= min_age_seconds:
            choose(tree, "budget")
            projected -= tree.bytes
    return Selection(
        tuple(sorted(selected.values(), key=lambda item: item.relative)),
        {key: tuple(sorted(value)) for key, value in sorted(reasons.items())},
        max(0, projected),
        max(0, projected - max_bytes),
    )


def remove_tree(path: Path, root: Path) -> None:
    resolved = path.resolve(strict=False)
    if resolved == root or not within(resolved, root):
        raise ArtifactError(f"refusing deletion outside managed root: {resolved}")
    require_plain_directory(resolved, "selected build tree", missing_ok=False)
    shutil.rmtree(resolved)
    try:
        resolved.parent.rmdir()
    except OSError:
        pass


def validate_current(metadata: CargoMetadata, root: Path) -> None:
    if not within(metadata.build_directory, root):
        raise ArtifactError(
            f"current build directory is outside {root}: {metadata.build_directory}"
        )
    expected = (metadata.workspace_root / "target").resolve(strict=False)
    if metadata.target_directory != expected:
        raise ArtifactError(
            "clean-current refuses an external/overridden target directory: "
            f"{metadata.target_directory}; expected {expected}"
        )


def target_entries(metadata: CargoMetadata) -> list[Path]:
    target = metadata.target_directory
    if not target.exists():
        return []
    require_plain_directory(target, "current target", missing_ok=False)
    return sorted((entry for entry in target.iterdir() if entry.name != "ripr"), key=lambda p: p.name)


def measure_paths(paths: Iterable[Path]) -> Measurement:
    size = files = 0
    newest = 0.0
    unsafe: list[str] = []
    for path in paths:
        info = path.lstat()
        newest = max(newest, info.st_mtime)
        if is_link(info):
            unsafe.append(str(path))
        elif stat.S_ISDIR(info.st_mode):
            measured = measure_tree(path)
            size += measured.bytes
            files += measured.files
            newest = max(newest, measured.newest_mtime)
            unsafe.extend(measured.unsafe)
        elif stat.S_ISREG(info.st_mode):
            size += info.st_size
            files += 1
        else:
            unsafe.append(str(path))
    return Measurement(size, files, newest, tuple(sorted(unsafe)))


def remove_target_entry(path: Path, target: Path) -> None:
    if path.resolve(strict=False).parent != target.resolve(strict=False):
        raise ArtifactError(f"refusing non-direct target entry: {path}")
    info = path.lstat()
    if is_link(info):
        raise ArtifactError(f"refusing linked target entry: {path}")
    if stat.S_ISDIR(info.st_mode):
        shutil.rmtree(path)
    elif stat.S_ISREG(info.st_mode):
        path.unlink()
    else:
        raise ArtifactError(f"refusing special target entry: {path}")


def timestamp(value: float) -> str:
    return dt.datetime.fromtimestamp(value, tz=dt.timezone.utc).isoformat().replace("+00:00", "Z")


def human_bytes(value: int) -> str:
    amount = float(value)
    for unit in ("B", "KB", "MB", "GB", "TB"):
        if amount < 1000 or unit == "TB":
            return f"{int(amount)} B" if unit == "B" else f"{amount:.1f} {unit}"
        amount /= 1000
    return f"{value} B"


def report_value(
    command: str,
    apply: bool,
    now: float,
    root: Path,
    metadata: CargoMetadata,
    trees: Sequence[BuildTree],
    selection: Selection,
    deleted: Sequence[str],
    current_target: Sequence[Path],
    target_measurement: Measurement,
    target_deleted: Sequence[str],
    args: argparse.Namespace,
) -> dict[str, object]:
    return {
        "schema_version": 1,
        "generated_at": timestamp(now),
        "command": command,
        "mode": "apply" if apply else "dry_run",
        "workspace_root": str(metadata.workspace_root),
        "cargo_build_root": str(root),
        "current_build_directory": str(metadata.build_directory),
        "current_target_directory": str(metadata.target_directory),
        "limits": {
            "max_size_gb": args.max_size_gb,
            "ttl_days": args.ttl_days,
            "min_age_hours": args.min_age_hours,
        },
        "managed_builds": {
            "trees": len(trees),
            "bytes_before": sum(tree.bytes for tree in trees),
            "selected_trees": len(selection.trees),
            "selected_bytes": sum(tree.bytes for tree in selection.trees),
            "projected_bytes": selection.projected_bytes,
            "unresolved_over_budget_bytes": selection.unresolved_over_budget_bytes,
            "deleted": list(deleted),
            "entries": [
                {
                    "path": tree.relative,
                    "bytes": tree.bytes,
                    "files": tree.files,
                    "newest_mtime": timestamp(tree.newest_mtime),
                    "age_hours": round(tree.age_seconds / 3600, 2),
                    "current": tree.current,
                    "unsafe_paths": list(tree.unsafe),
                    "selected_reasons": list(selection.reasons.get(tree.relative, ())),
                }
                for tree in sorted(trees, key=lambda item: item.relative)
            ],
        },
        "current_target": {
            "preserved_entry": "ripr",
            "selected_entries": [path.name for path in current_target],
            "selected_bytes": target_measurement.bytes,
            "unsafe_paths": list(target_measurement.unsafe),
            "deleted": list(target_deleted),
        },
    }


def markdown(value: dict[str, object]) -> str:
    builds = value["managed_builds"]
    target = value["current_target"]
    assert isinstance(builds, dict) and isinstance(target, dict)
    lines = [
        "# Cargo artifact report",
        "",
        f"- Command: `{value['command']}`",
        f"- Mode: **{value['mode']}**",
        f"- Managed root: `{value['cargo_build_root']}`",
        f"- Current build directory: `{value['current_build_directory']}`",
        f"- Managed trees: {builds['trees']}",
        f"- Managed bytes before: {human_bytes(int(builds['bytes_before']))}",
        f"- Selected: {builds['selected_trees']} trees / {human_bytes(int(builds['selected_bytes']))}",
        f"- Projected managed bytes: {human_bytes(int(builds['projected_bytes']))}",
        f"- Current target selected: {len(target['selected_entries'])} entries / "
        f"{human_bytes(int(target['selected_bytes']))}",
        "- Preserved target entry: `ripr`",
        "",
    ]
    residual = int(builds["unresolved_over_budget_bytes"])
    if residual:
        lines.insert(-1, f"- Still over budget: **{human_bytes(residual)}**")
    entries = builds["entries"]
    if entries:
        lines += [
            "## Managed build trees",
            "",
            "| Tree | Size | Age | State |",
            "|---|---:|---:|---|",
        ]
        for entry in entries:
            state: list[str] = []
            if entry["current"]:
                state.append("current")
            if entry["unsafe_paths"]:
                state.append("unsafe")
            state += [str(reason) for reason in entry["selected_reasons"]]
            lines.append(
                f"| `{entry['path']}` | {human_bytes(int(entry['bytes']))} | "
                f"{float(entry['age_hours']):.1f} h | {', '.join(state) or 'keep'} |"
            )
        lines.append("")
    if value["mode"] == "dry_run" and (
        int(builds["selected_trees"]) or target["selected_entries"]
    ):
        lines += ["Re-run the same command with `--apply` to delete the selected paths.", ""]
    return "\n".join(lines)


def write_receipt(root: Path, value: dict[str, object], rendered: str) -> None:
    json_path = root / REPORT_JSON
    md_path = root / REPORT_MD
    json_path.parent.mkdir(parents=True, exist_ok=True)
    json_path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    md_path.write_text(rendered, encoding="utf-8")


def execute(args: argparse.Namespace, cwd: Path | None = None) -> int:
    repo = workspace_root(cwd or Path.cwd())
    metadata = cargo_metadata(repo)
    root = validate_root(args.root or managed_root())
    now = args.now if args.now is not None else time.time()
    trees = inventory(root, metadata.build_directory, now)
    selection = select_gc(
        trees,
        args.max_size_gb * GB,
        args.ttl_days * DAY,
        args.min_age_hours * 3600,
    )
    if args.command == "report" and args.apply:
        raise ArtifactError("--apply is valid only with gc or clean-current")

    deleted: list[str] = []
    selected_target: list[Path] = []
    target_measurement = Measurement(0, 0, 0.0, ())
    target_deleted: list[str] = []

    if args.command == "gc" and args.apply:
        if not within(metadata.build_directory, root):
            raise ArtifactError(f"current build directory is outside {root}")
        for tree in selection.trees:
            remove_tree(tree.path, root)
            deleted.append(tree.relative)
    elif args.command == "clean-current":
        validate_current(metadata, root)
        selected_target = target_entries(metadata)
        target_measurement = measure_paths(selected_target)
        if target_measurement.unsafe:
            raise ArtifactError(
                "clean-current found linked/special paths: "
                + ", ".join(target_measurement.unsafe[:5])
            )
        current = next((tree for tree in trees if within(metadata.build_directory, tree.path)), None)
        if metadata.build_directory.exists() and current is None:
            raise ArtifactError(
                f"current build directory is not a managed hash leaf: {metadata.build_directory}"
            )
        total = sum(tree.bytes for tree in trees)
        selection = (
            Selection((current,), {current.relative: ("current-clean",)}, total - current.bytes, 0)
            if current
            else Selection((), {}, total, 0)
        )
        if args.apply:
            if current:
                remove_tree(current.path, root)
                deleted.append(current.relative)
            for path in selected_target:
                remove_target_entry(path, metadata.target_directory)
                target_deleted.append(path.name)

    value = report_value(
        args.command,
        args.apply,
        now,
        root,
        metadata,
        trees,
        selection,
        deleted,
        selected_target,
        target_measurement,
        target_deleted,
        args,
    )
    rendered = markdown(value)
    if not args.no_receipt:
        write_receipt(repo, value, rendered)
    print(rendered, end="")
    return 0


def main(argv: Sequence[str] | None = None) -> int:
    try:
        return execute(parse_args(sys.argv[1:] if argv is None else argv))
    except ArtifactError as exc:
        print(f"cargo-artifacts: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
