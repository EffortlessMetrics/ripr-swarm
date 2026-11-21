"""Admit relative server archive subjects against the checked-out distribution policy."""

import argparse
import collections
import hashlib
import json
from pathlib import Path
import re
import sys
import tomllib


def unique_keys(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def subjects(args):
    raw = args.manifest.read_bytes()
    if hashlib.sha256(raw).hexdigest() != args.digest:
        raise ValueError("raw manifest digest mismatch")
    manifest = json.loads(raw, object_pairs_hook=unique_keys)
    build = manifest["build_identity"]
    if (manifest["schema_version"] != "2" or "version" in manifest
            or manifest["product_version"] != args.version
            or manifest["source_repository"] != args.repository
            or build["repository"] != args.repository
            or build["candidate_sha"] != args.commit or build["candidate_tree"] != args.tree):
        raise ValueError("manifest schema, product or source identity mismatch")

    policy = Path(__file__).resolve().parents[2] / "policy/distribution.toml"
    targets = {row["rust_target"]: row["archive"] for row in tomllib.loads(policy.read_text())["target"]}
    assets = manifest["assets"]
    if sorted(manifest["target_set"]["targets"]) != sorted(targets) or set(assets) != set(targets):
        raise ValueError("manifest target inventory differs from distribution policy")
    inventory = collections.Counter(row["name"] for row in json.loads(args.inventory.read_bytes())["assets"])
    prefix = f"ripr-server-v{args.version}-"
    expected = {}
    for target, extension in targets.items():
        asset = assets[target]
        name = f"{prefix}{target}.{extension}"
        if ("url" in asset or asset["subject"] != name or asset["archive_format"] != extension
                or type(asset["archive_size"]) is not int or asset["archive_size"] <= 0
                or not re.fullmatch(r"[0-9a-f]{64}", asset["sha256"])):
            raise ValueError(f"invalid archive subject for {target}")
        expected[name] = asset
    observed = {name: count for name, count in inventory.items()
                if name.startswith(prefix) and name.endswith((".tar.gz", ".zip"))}
    if observed != dict.fromkeys(expected, 1):
        raise ValueError("missing, unexpected or duplicate release archive")
    if args.archives:
        for name, asset in expected.items():
            path = args.archives / name
            if path.is_symlink() or not path.is_file() or path.stat().st_size != asset["archive_size"]:
                raise ValueError(f"archive size or file type mismatch: {name}")
            with path.open("rb") as stream:
                if hashlib.file_digest(stream, "sha256").hexdigest() != asset["sha256"]:
                    raise ValueError(f"archive digest mismatch: {name}")
    return sorted(expected)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ("manifest", "inventory"):
        parser.add_argument(f"--{key}", type=Path, required=True)
    for key in ("digest", "repository", "version", "commit", "tree"):
        parser.add_argument(f"--{key}", required=True)
    parser.add_argument("--archives", type=Path)
    try:
        print("\n".join(subjects(parser.parse_args())))
    except (KeyError, TypeError, ValueError, OSError) as exc:
        sys.exit(f"server archive admission failed: {exc}")
