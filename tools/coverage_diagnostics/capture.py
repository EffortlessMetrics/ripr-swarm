"""Bounded read-only profile evidence; never repairs or filters merge inputs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import signal
import stat
import subprocess
import time

MAX_ENTRIES = 8192
MAX_INVENTORY = 512
MAX_INVENTORY_BYTES = 65536
MAX_LIST_BYTES = 8388608
MAX_LIST_ENTRIES = 65536
MAX_PROFILES = 8
MAX_PROFILE_BYTES = 65536
MAX_TOOL_BYTES = 16384
MAX_ARTIFACT_BYTES = 1048576
MAX_ARTIFACT_FILES = 16


def bounded_tool(argv, timeout=30):
    """Drain both pipes with fixed retained bytes and a process-group deadline."""
    try:
        child = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                 start_new_session=True)
    except OSError as error:
        return {"exit": None, "stdout": "", "stderr": str(error), "truncated": False}
    streams = selectors.DefaultSelector()
    buffers = {"stdout": bytearray(), "stderr": bytearray()}
    for name in buffers:
        streams.register(getattr(child, name), selectors.EVENT_READ, name)
    deadline = time.monotonic() + timeout
    truncated = False
    timed_out = False
    while streams.get_map():
        if time.monotonic() >= deadline and not timed_out:
            timed_out = True
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        for key, _ in streams.select(0.1):
            data = os.read(key.fileobj.fileno(), 8192)
            if not data:
                streams.unregister(key.fileobj)
                key.fileobj.close()
                continue
            buffer = buffers[key.data]
            retained = data[:max(0, MAX_TOOL_BYTES - len(buffer))]
            buffer.extend(retained)
            truncated |= len(retained) != len(data)
    streams.close()
    return {"exit": child.wait(), "stdout": buffers["stdout"].decode("utf-8", "replace"),
            "stderr": buffers["stderr"].decode("utf-8", "replace"),
            "truncated": truncated, "timed_out": timed_out}


class ArtifactBudget:
    def __init__(self, directory):
        self.directory = directory
        self.bytes = 0
        self.files = 0

    def write(self, name, data):
        if self.files >= MAX_ARTIFACT_FILES or self.bytes + len(data) > MAX_ARTIFACT_BYTES:
            raise ValueError("diagnostic artifact budget exceeded")
        with (self.directory / name).open("xb") as target:
            target.write(data)
        self.bytes += len(data)
        self.files += 1


def prefix(path, limit):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as source:
        if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
            raise ValueError("diagnostic input is not a regular file")
        return source.read(limit)


def capture(profiles, output, run=bounded_tool, profile_list=None):
    profiles = profiles.absolute()
    output = output.absolute()
    # Never follow planted directories or overwrite earlier evidence.
    for path in [profiles, output.parent]:
        if any(parent.is_symlink() for parent in [path, *path.parents]):
            raise ValueError("diagnostic paths must not contain symlinks")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.mkdir()
    budget = ArtifactBudget(output)
    report = {"schema": 1, "original_coverage_outcome": "failure",
              "producer": "unresolved; filename PID/module are hints, not executable attribution",
              "limits": {"directory_entries": MAX_ENTRIES, "inventory": MAX_INVENTORY,
                         "inventory_json_bytes": MAX_INVENTORY_BYTES,
                         "profiles": MAX_PROFILES, "profile_bytes_each": MAX_PROFILE_BYTES,
                         "tool_bytes_per_stream": MAX_TOOL_BYTES,
                         "artifact_files": MAX_ARTIFACT_FILES, "artifact_bytes": MAX_ARTIFACT_BYTES},
              "inventory": [], "inventory_truncated": False, "captured": []}
    lists = [profile_list.absolute()] if profile_list is not None else []
    inventory_bytes = 0
    if profiles.is_dir():
        with os.scandir(profiles) as entries:
            for index, entry in enumerate(entries):
                if index >= MAX_ENTRIES:
                    report["inventory_truncated"] = True
                    break
                if not entry.is_file(follow_symlinks=False):
                    continue
                name = entry.name
                if profile_list is None and name.endswith("-profraw-list") and len(lists) < 16:
                    lists.append(Path(entry.path))
                if not name.endswith(".profraw"):
                    continue
                if len(report["inventory"]) >= MAX_INVENTORY:
                    report["inventory_truncated"] = True
                    continue
                info = entry.stat(follow_symlinks=False)
                row = {"name": name[:240], "bytes": info.st_size, "mtime_ns": info.st_mtime_ns}
                row_bytes = len(json.dumps(row).encode())
                if inventory_bytes + row_bytes > MAX_INVENTORY_BYTES:
                    report["inventory_truncated"] = True
                    continue
                inventory_bytes += row_bytes
                report["inventory"].append(row)
    else:
        report["profiles_missing"] = True
    version = run(["rustc", "-vV"])
    sysroot = run(["rustc", "--print", "sysroot"])
    report["rustc"] = version
    commit = run(["git", "rev-parse", "HEAD"])
    report["checkout_sha"] = (commit["stdout"].strip() if commit["exit"] == 0
                              and re.fullmatch(r"[0-9a-f]{40}", commit["stdout"].strip()) else None)
    host = re.search(r"(?m)^host: ([A-Za-z0-9_-]+)$", version["stdout"])
    merge = {"exit": None, "stderr": "profile list or matching LLVM tool unavailable"}
    safe_list = False
    if len(lists) == 1:
        try:
            if lists[0].parent.resolve() != profiles.resolve():
                raise ValueError("profile list must belong to the profile directory")
            content = prefix(lists[0], MAX_LIST_BYTES + 1)
            lines = content.decode().splitlines()
            safe_list = len(content) <= MAX_LIST_BYTES and len(lines) <= MAX_LIST_ENTRIES
            for line in lines:
                path = Path(line)
                if not path.is_absolute():
                    path = Path.cwd() / path
                safe_list &= (path.parent.resolve() == profiles.resolve()
                              and path.suffix == ".profraw"
                              and stat.S_ISREG(path.lstat().st_mode))
        except (UnicodeError, OSError, ValueError):
            safe_list = False
        report["profile_list_replay_safe"] = bool(safe_list)
        report["profile_list_validation_limits"] = {"bytes": MAX_LIST_BYTES, "entries": MAX_LIST_ENTRIES}
    if version["exit"] == 0 and host and sysroot["exit"] == 0 and len(lists) == 1 and safe_list:
        tool = Path(sysroot["stdout"].strip()) / "lib/rustlib" / host[1] / "bin/llvm-profdata"
        report["llvm_profdata"] = run([str(tool), "--version"])
        report["profile_list"] = {"name": lists[0].name[:240], "bytes": lists[0].stat().st_size}
        budget.write("profile-list-prefix.txt", prefix(lists[0], MAX_PROFILE_BYTES))
        # Read the ORIGINAL full list, unfiltered. Only the diagnostic output
        # goes to the null device; original profiles/profdata/LCOV are untouched.
        merge = run([str(tool), "merge", "-sparse", "-f", str(lists[0]), "-o", os.devnull])
    else:
        report["profile_list_count"] = len(lists)
    report["merge_replay"] = merge
    report["merge_replay_scope"] = "diagnostic only; original failed coverage step remains failed"
    candidates = []
    for line in merge["stderr"].splitlines():
        if not line.startswith("warning: ") or ": invalid instrumentation profile data" not in line:
            continue
        path = Path(line[len("warning: "):].split(": invalid instrumentation profile data", 1)[0])
        if not path.is_absolute():
            path = Path.cwd() / path
        if path.parent.resolve() == profiles.resolve() and path.suffix == ".profraw":
            if path not in candidates:
                candidates.append(path)
    report["failed_profile_capture_truncated"] = len(candidates) > MAX_PROFILES
    for index, path in enumerate(candidates[:MAX_PROFILES]):
        try:
            info = path.lstat()
            data = prefix(path, MAX_PROFILE_BYTES)
        except (OSError, ValueError) as error:
            report["captured"].append({"name": path.name[:240], "error": str(error)[:512]})
            continue
        name = f"profile-{index:02}.prefix.profraw"
        budget.write(name, data)
        hint = re.search(r"-(\d+)-([0-9_]+)\.profraw$", path.name)
        report["captured"].append({"name": path.name[:240], "artifact": name,
                                   "original_bytes": info.st_size, "retained_bytes": len(data),
                                   "complete": len(data) == info.st_size,
                                   "prefix_sha256": hashlib.sha256(data).hexdigest(),
                                   "first_256_hex": data[:256].hex(),
                                   "pid_hint": int(hint[1]) if hint else None,
                                   "module_hint": hint[2] if hint else None})
    report["artifact_bytes_before_manifest"] = budget.bytes
    payload = json.dumps(report, indent=2).encode()
    budget.write("manifest.json", payload)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profiles", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--list", type=Path)
    args = parser.parse_args()
    capture(args.profiles, args.output, profile_list=args.list)
    print("Bounded diagnostics retained; original coverage failure remains unchanged.")


if __name__ == "__main__":
    main()
