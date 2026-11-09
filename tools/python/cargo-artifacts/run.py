#!/usr/bin/env python3
"""Inventory and reclaim RIPR Cargo artifacts without erasing target/ripr.

Large Cargo intermediates live in a same-volume pool shared by sibling
worktrees and keyed by Cargo's workspace-path hash. Deletion is dry-run by
default, preserves live worktrees, and requires --apply.
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
DEFAULT_MAX_GB = 12
DEFAULT_TTL_DAYS = 7
DEFAULT_MIN_AGE_HOURS = 24
POOL_NAME = ".ripr-swarm-build"
SHARD_RE = re.compile(r"^[0-9a-f]{2}$")
HASH_RE = re.compile(r"^[0-9a-f]{8,62}$")
REPORT_JSON = Path("target/ripr/reports/cargo-artifacts.json")
REPORT_MD = Path("target/ripr/reports/cargo-artifacts.md")
OVERRIDE_ENV = ("CARGO_TARGET_DIR", "CARGO_BUILD_TARGET_DIR", "CARGO_BUILD_BUILD_DIR")


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
    live_worktree: bool
    unsafe: tuple[str, ...]

    @property
    def eligible(self) -> bool:
        return not self.current and not self.live_worktree and not self.unsafe


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
    parser.add_argument("--root", type=Path, help="Override the managed pool (recovery/tests).")
    parser.add_argument("--max-size-gb", type=non_negative, default=DEFAULT_MAX_GB)
    parser.add_argument("--ttl-days", type=non_negative, default=DEFAULT_TTL_DAYS)
    parser.add_argument("--min-age-hours", type=non_negative, default=DEFAULT_MIN_AGE_HOURS)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--no-receipt", action="store_true")
    parser.add_argument("--now", type=float, help=argparse.SUPPRESS)
    return parser.parse_args(argv)


def reject_overrides() -> None:
    active = [name for name in OVERRIDE_ENV if os.environ.get(name, "").strip()]
    if active:
        raise ArtifactError(
            "unset Cargo artifact overrides before using the repo-owned lifecycle: "
            + ", ".join(active)
        )


def workspace_root(start: Path) -> Path:
    current = start.resolve()
    for candidate in (current, *current.parents):
        if (candidate / "Cargo.toml").is_file() and (candidate / ".cargo/config.toml").is_file():
            return candidate
    raise ArtifactError(f"run from the RIPR repository; no workspace found above {start}")


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
        raise ArtifactError(f"could not execute cargo metadata in {root}: {exc}") from exc
    if result.returncode:
        detail = result.stderr.strip() or result.stdout.strip() or "no diagnostics"
        raise ArtifactError(f"cargo metadata failed in {root} ({result.returncode}): {detail}")
    try:
        value = json.loads(result.stdout)
        fields = [value[name] for name in ("workspace_root", "target_directory", "build_directory")]
    except (json.JSONDecodeError, KeyError, TypeError) as exc:
        raise ArtifactError(
            f"cargo metadata in {root} lacks build_directory; use the pinned Cargo toolchain"
        ) from exc
    if not all(isinstance(field, str) and field for field in fields):
        raise ArtifactError(f"cargo metadata in {root} returned empty artifact paths")
    return CargoMetadata(*(Path(os.path.abspath(field)) for field in fields))


def managed_root(metadata: CargoMetadata) -> Path:
    return Path(os.path.abspath(metadata.workspace_root.parent / POOL_NAME))


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
    lexical = Path(os.path.abspath(root.expanduser()))
    if lexical.name != POOL_NAME or lexical == Path(lexical.anchor):
        raise ArtifactError(f"managed root must end in {POOL_NAME}: {lexical}")
    require_plain_directory(lexical, "managed root")
    return lexical.resolve(strict=False)


def within(path: Path, parent: Path) -> bool:
    try:
        path.resolve(strict=False).relative_to(parent.resolve(strict=False))
        return True
    except ValueError:
        return False


def parse_worktree_porcelain(raw: bytes) -> tuple[Path, ...]:
    paths: list[Path] = []
    block: list[bytes] = []
    for token in raw.split(b"\0"):
        if token:
            block.append(token)
            continue
        if block:
            path_token = next((item for item in block if item.startswith(b"worktree ")), None)
            if path_token is not None and b"bare" not in block:
                paths.append(Path(os.fsdecode(path_token[len(b"worktree ") :])))
            block = []
    if block:
        path_token = next((item for item in block if item.startswith(b"worktree ")), None)
        if path_token is not None and b"bare" not in block:
            paths.append(Path(os.fsdecode(path_token[len(b"worktree ") :])))
    return tuple(paths)


def registered_worktrees(repo: Path) -> tuple[Path, ...]:
    try:
        result = subprocess.run(
            ["git", "worktree", "list", "--porcelain", "-z"],
            cwd=repo,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except OSError as exc:
        raise ArtifactError(f"could not enumerate git worktrees: {exc}") from exc
    if result.returncode:
        detail = os.fsdecode(result.stderr).strip() or "no diagnostics"
        raise ArtifactError(f"git worktree list failed ({result.returncode}): {detail}")
    return parse_worktree_porcelain(result.stdout)


def sibling_workspaces(repo: Path) -> tuple[Path, ...]:
    parent = repo.parent.resolve(strict=False)
    candidates: set[Path] = {
        path
        for path in registered_worktrees(repo)
        if path.parent.resolve(strict=False) == parent
    }
    require_plain_directory(parent, "worktree parent", missing_ok=False)
    for entry in parent.iterdir():
        try:
            info = entry.lstat()
        except OSError:
            continue
        if is_link(info) or not stat.S_ISDIR(info.st_mode):
            continue
        config = entry / ".cargo/config.toml"
        if not (entry / "Cargo.toml").is_file() or not config.is_file():
            continue
        try:
            config_text = config.read_text(encoding="utf-8")
        except OSError as exc:
            raise ArtifactError(f"could not inspect sibling Cargo config {config}: {exc}") from exc
        if POOL_NAME not in config_text:
            continue
        candidates.add(entry)
    return tuple(sorted((path.resolve(strict=False) for path in candidates), key=str))


def live_build_directories(repo: Path, pool: Path) -> tuple[Path, ...]:
    builds: set[Path] = set()
    for candidate in sibling_workspaces(repo):
        if not candidate.is_dir():
            raise ArtifactError(
                f"registered worktree is missing: {candidate}; repair or prune git worktrees first"
            )
        metadata = cargo_metadata(candidate)
        if within(metadata.build_directory, pool):
            builds.add(metadata.build_directory)
    return tuple(sorted(builds, key=str))


def pool_anomalies(root: Path) -> tuple[str, ...]:
    if not root.exists():
        return ()
    anomalies: list[str] = []
    try:
        shards = sorted(root.iterdir(), key=lambda item: item.name)
    except OSError as exc:
        raise ArtifactError(f"could not inspect managed root {root}: {exc}") from exc
    for shard in shards:
        try:
            info = shard.lstat()
        except OSError as exc:
            anomalies.append(f"{shard}: {exc}")
            continue
        if is_link(info) or not stat.S_ISDIR(info.st_mode) or not SHARD_RE.fullmatch(shard.name):
            anomalies.append(str(shard))
            continue
        try:
            leaves = sorted(shard.iterdir(), key=lambda item: item.name)
        except OSError as exc:
            anomalies.append(f"{shard}: {exc}")
            continue
        for leaf in leaves:
            try:
                leaf_info = leaf.lstat()
            except OSError as exc:
                anomalies.append(f"{leaf}: {exc}")
                continue
            if (
                is_link(leaf_info)
                or not stat.S_ISDIR(leaf_info.st_mode)
                or not HASH_RE.fullmatch(leaf.name)
            ):
                anomalies.append(str(leaf))
    return tuple(sorted(anomalies))


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


def inventory(
    root: Path, current_build: Path, live_builds: Sequence[Path], now: float
) -> list[BuildTree]:
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
                any(within(build, leaf) for build in live_builds),
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
    require_plain_directory(path, "selected build tree", missing_ok=False)
    resolved = path.resolve(strict=False)
    if resolved == root or not within(resolved, root):
        raise ArtifactError(f"refusing deletion outside managed root: {resolved}")
    measured = measure_tree(path)
    if measured.unsafe:
        raise ArtifactError(f"selected build tree became unsafe: {measured.unsafe[0]}")
    shutil.rmtree(path)
    try:
        path.parent.rmdir()
    except OSError:
        pass


def validate_current(metadata: CargoMetadata, root: Path) -> None:
    expected_target = Path(os.path.abspath(metadata.workspace_root / "target"))
    actual_target = Path(os.path.abspath(metadata.target_directory))
    if os.path.normcase(str(actual_target)) != os.path.normcase(str(expected_target)):
        raise ArtifactError(
            "clean-current refuses an external target directory: "
            f"{metadata.target_directory}; expected {expected_target}"
        )
    if not within(metadata.build_directory, root):
        raise ArtifactError(
            f"current build directory is outside {root}: {metadata.build_directory}"
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
        measured = measure_tree(path)
        if measured.unsafe:
            raise ArtifactError(f"target entry became unsafe: {measured.unsafe[0]}")
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


def make_report(
    args: argparse.Namespace,
    now: float,
    root: Path,
    metadata: CargoMetadata,
    live_builds: Sequence[Path],
    anomalies: Sequence[str],
    trees: Sequence[BuildTree],
    selection: Selection,
    deleted: Sequence[str],
    selected_target: Sequence[Path],
    target_measurement: Measurement,
    target_deleted: Sequence[str],
) -> dict[str, object]:
    return {
        "schema_version": 2,
        "generated_at": timestamp(now),
        "command": args.command,
        "mode": "apply" if args.apply else "dry_run",
        "workspace_root": str(metadata.workspace_root),
        "cargo_build_root": str(root),
        "current_build_directory": str(metadata.build_directory),
        "current_target_directory": str(metadata.target_directory),
        "live_build_directories": [str(path) for path in live_builds],
        "pool_anomalies": list(anomalies),
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
                    "live_worktree": tree.live_worktree,
                    "unsafe_paths": list(tree.unsafe),
                    "selected_reasons": list(selection.reasons.get(tree.relative, ())),
                }
                for tree in sorted(trees, key=lambda item: item.relative)
            ],
        },
        "current_target": {
            "preserved_entry": "ripr",
            "selected_entries": [path.name for path in selected_target],
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
        f"- Protected live build directories: {len(value['live_build_directories'])}",
        f"- Pool anomalies: {len(value['pool_anomalies'])}",
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
    anomalies = value["pool_anomalies"]
    if anomalies:
        lines += ["## Pool anomalies", ""]
        lines += [f"- `{path}`" for path in anomalies]
        lines.append("")
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
            if entry["live_worktree"]:
                state.append("live-worktree")
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


def ensure_plain_directory_chain(repo: Path, relative: Path) -> Path:
    current = repo
    for part in relative.parts:
        current = current / part
        if current.exists():
            require_plain_directory(current, "receipt directory", missing_ok=False)
        else:
            current.mkdir()
    return current


def write_plain_text(path: Path, content: str) -> None:
    try:
        info = path.lstat()
    except FileNotFoundError:
        pass
    else:
        if is_link(info) or not stat.S_ISREG(info.st_mode):
            raise ArtifactError(f"refusing non-regular receipt path: {path}")
    path.write_text(content, encoding="utf-8")


def write_receipt(repo: Path, value: dict[str, object], rendered: str) -> None:
    report_dir = ensure_plain_directory_chain(repo, REPORT_JSON.parent)
    write_plain_text(
        report_dir / REPORT_JSON.name,
        json.dumps(value, indent=2, sort_keys=True) + "\n",
    )
    write_plain_text(report_dir / REPORT_MD.name, rendered)


def execute(args: argparse.Namespace, cwd: Path | None = None) -> int:
    reject_overrides()
    repo = workspace_root(cwd or Path.cwd())
    metadata = cargo_metadata(repo)
    configured_root = validate_root(managed_root(metadata))
    root = validate_root(args.root or configured_root)
    if args.apply and root != configured_root:
        raise ArtifactError(
            f"--apply refuses an override pool: {root}; configured pool is {configured_root}"
        )
    anomalies = pool_anomalies(root)
    if args.apply and anomalies:
        raise ArtifactError(
            "managed pool contains unrecognized or linked paths: " + ", ".join(anomalies[:5])
        )
    now = args.now if args.now is not None else time.time()
    live_builds = () if args.command == "clean-current" else live_build_directories(repo, root)
    trees = inventory(root, metadata.build_directory, live_builds, now)
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
        latest_live = live_build_directories(repo, root)
        for tree in selection.trees:
            if any(within(build, tree.path) for build in latest_live):
                raise ArtifactError(f"selected tree became live before deletion: {tree.path}")
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

    value = make_report(
        args,
        now,
        root,
        metadata,
        live_builds,
        anomalies,
        trees,
        selection,
        deleted,
        selected_target,
        target_measurement,
        target_deleted,
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
