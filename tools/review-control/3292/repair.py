#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path
from typing import Any

PARENT_SHA = "723414bb1d98e50ee471b63fd1698e9cad803c71"
MERGE_SHA = "261aa86514ee1ae273ac65f7c6351e47cd50f47f"
EXPECTED_GOLDEN_COUNT = 176
ALLOWED_CURRENTNESS = {
    "candidate_current",
    "base_deleted",
    "moved_or_renamed",
    "unresolved_subject",
}
MARKER = "RIPR-SPEC-0151"
CANONICAL_REASON = (
    "RIPR-SPEC-0151: rebless check JSON for the additive source_currentness "
    "field; classifications, stages, confidence, counts, and recorded "
    "coordinates remain unchanged."
)


def git(repo: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        check=check,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def replace_exact(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


def json_at(repo: Path, commit: str, path: str) -> Any:
    result = git(repo, "show", f"{commit}:{path}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise SystemExit(f"{commit}:{path}: invalid JSON: {error}") from error


def strip_currentness(value: Any) -> Any:
    if isinstance(value, dict):
        return {
            key: strip_currentness(item)
            for key, item in value.items()
            if key != "source_currentness"
        }
    if isinstance(value, list):
        return [strip_currentness(item) for item in value]
    return value


def changed_paths(repo: Path) -> list[str]:
    result = git(repo, "diff", "--name-only", PARENT_SHA, MERGE_SHA)
    return [line for line in result.stdout.splitlines() if line]


def fixture_name(path: str) -> str:
    parts = Path(path).parts
    if len(parts) < 4 or parts[0] != "fixtures" or parts[-2:] != ("expected", "CHANGELOG.md"):
        raise SystemExit(f"unexpected fixture changelog path: {path}")
    return parts[1]


def canonicalize_changelog(path: Path, name: str) -> int:
    text = path.read_text(encoding="utf-8")
    pattern = re.compile(r"(?ms)^## Pending\n.*?(?=^## |\Z)")
    removed = 0

    def rewrite(match: re.Match[str]) -> str:
        nonlocal removed
        section = match.group(0)
        if MARKER in section:
            removed += 1
            return ""
        return section

    text = pattern.sub(rewrite, text).rstrip()
    block = (
        "\n\n## Pending\n\n"
        "Reason:\n"
        f"{CANONICAL_REASON}\n\n"
        "Command:\n"
        f"`cargo xtask goldens bless {name} --reason \"...\"`\n\n"
        "Updated:\n"
        "- `expected/check.json`\n"
    )
    updated = text + block
    if updated.count(MARKER) != 1:
        raise SystemExit(
            f"{path}: expected one canonical {MARKER} receipt, found {updated.count(MARKER)}"
        )
    path.write_text(updated, encoding="utf-8")
    return removed


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit("usage: repair.py <repository-root> <receipt-path>")
    repo = Path(sys.argv[1]).resolve()
    receipt_path = Path(sys.argv[2]).resolve()

    head = git(repo, "rev-parse", "HEAD").stdout.strip()
    if head != MERGE_SHA:
        raise SystemExit(f"expected exact merged #3291 head {MERGE_SHA}, got {head}")
    git(repo, "merge-base", "--is-ancestor", PARENT_SHA, MERGE_SHA)

    domain_path = repo / "crates/ripr/src/domain/probe.rs"
    domain = domain_path.read_text(encoding="utf-8")
    domain = replace_exact(
        domain,
        """    /// field existed. A `BaseDeleted` or `MovedOrRenamed` finding is
    /// base-side evidence, not a candidate edit target.
""",
        """    /// field existed. A `BaseDeleted` finding is base-side evidence; a
    /// `MovedOrRenamed` finding carries unresolved movement evidence.
    /// Neither is a candidate edit target.
""",
        "Finding source-currentness boundary",
    )
    domain = replace_exact(
        domain,
        """    /// The expression was removed on the candidate side. The retained
    /// evidence is base-side: it carries the base coordinate and is not a
    /// candidate edit target.
""",
        """    /// The expression was removed on the candidate side. The retained
    /// evidence is base-side and is not a candidate edit target. In the C1
    /// producer slice, the recorded probe coordinate remains the projected
    /// new-side coordinate; consumer re-coordination is owned by #3281.
""",
        "BaseDeleted coordinate documentation",
    )
    domain_path.write_text(domain, encoding="utf-8")

    spec_path = repo / "docs/specs/RIPR-SPEC-0151-source-currentness.md"
    spec = spec_path.read_text(encoding="utf-8")
    spec = replace_exact(
        spec,
        """- `crates/ripr/src/analysis/probes/diff.rs` resolves the disposition from
  diff evidence and records the base-side coordinate for removed-only
  probes.
""",
        """- `crates/ripr/src/analysis/probes/diff.rs` resolves the disposition from
  diff evidence while retaining the projected new-side coordinate for
  removed-only probes; the disposition carries base-side semantics until
  #3281 re-coordinates consumer surfaces.
""",
        "RIPR-SPEC-0151 implementation mapping",
    )
    spec_path.write_text(spec, encoding="utf-8")

    names = changed_paths(repo)
    check_paths = sorted(
        path
        for path in names
        if path.startswith("fixtures/") and path.endswith("/expected/check.json")
    )
    changelog_paths = sorted(
        path
        for path in names
        if path.startswith("fixtures/") and path.endswith("/expected/CHANGELOG.md")
    )
    if len(check_paths) != EXPECTED_GOLDEN_COUNT:
        raise SystemExit(
            f"expected {EXPECTED_GOLDEN_COUNT} changed golden check files, found {len(check_paths)}"
        )
    if len(changelog_paths) != EXPECTED_GOLDEN_COUNT:
        raise SystemExit(
            f"expected {EXPECTED_GOLDEN_COUNT} changed fixture changelogs, found {len(changelog_paths)}"
        )

    check_fixtures = {Path(path).parts[1] for path in check_paths}
    changelog_fixtures = {Path(path).parts[1] for path in changelog_paths}
    if check_fixtures != changelog_fixtures:
        raise SystemExit(
            "changed golden/check and changelog fixture sets differ: "
            + json.dumps(
                {
                    "check_only": sorted(check_fixtures - changelog_fixtures),
                    "changelog_only": sorted(changelog_fixtures - check_fixtures),
                },
                sort_keys=True,
            )
        )

    disposition_counts: Counter[str] = Counter()
    finding_count = 0
    for path in check_paths:
        before = json_at(repo, PARENT_SHA, path)
        current_path = repo / path
        current = json.loads(current_path.read_text(encoding="utf-8"))
        if strip_currentness(current) != before:
            raise SystemExit(
                f"{path}: #3291 golden is not semantically additive-only after removing source_currentness"
            )
        findings = current.get("findings", []) if isinstance(current, dict) else []
        if not isinstance(findings, list):
            raise SystemExit(f"{path}: findings must be an array")
        for index, finding in enumerate(findings):
            if not isinstance(finding, dict):
                raise SystemExit(f"{path}: finding {index} is not an object")
            value = finding.get("source_currentness")
            if value not in ALLOWED_CURRENTNESS:
                raise SystemExit(
                    f"{path}: finding {index} has invalid source_currentness {value!r}"
                )
            disposition_counts[value] += 1
            finding_count += 1

    removed_receipts = 0
    for path in changelog_paths:
        removed_receipts += canonicalize_changelog(repo / path, fixture_name(path))

    false_phrases = [
        "carries the base coordinate",
        "records the base-side coordinate",
        "record the base-side line coordinate",
    ]
    scoped_files = [domain_path, spec_path, *(repo / path for path in changelog_paths)]
    for path in scoped_files:
        text = path.read_text(encoding="utf-8")
        for phrase in false_phrases:
            if phrase in text:
                raise SystemExit(f"{path}: stale coordinate claim remains: {phrase}")

    temporary_body = repo / "pr-tmp/pr-3280-body.md"
    if not temporary_body.is_file():
        raise SystemExit("expected merged temporary PR-body file is missing before cleanup")
    temporary_body.unlink()

    seed = repo / "crates/ripr/proptest-regressions/output/json/mod.txt"
    if not seed.is_file():
        raise SystemExit("JSON renderer proptest regression seed must remain tracked")
    seed_sha256 = sha256(seed)

    fixture_list_digest = hashlib.sha256(
        ("\n".join(sorted(check_fixtures)) + "\n").encode("utf-8")
    ).hexdigest()
    receipt = {
        "schema": "ripr.source_currentness_review_repair.v1",
        "issue": 3292,
        "parent_sha": PARENT_SHA,
        "merged_pr_sha": MERGE_SHA,
        "golden_check_files": len(check_paths),
        "fixture_changelogs": len(changelog_paths),
        "fixture_list_sha256": fixture_list_digest,
        "finding_count": finding_count,
        "disposition_counts": dict(sorted(disposition_counts.items())),
        "semantic_proof": "all merged check JSON equals the pre-#3291 JSON after recursively removing source_currentness",
        "removed_intermediate_receipts": removed_receipts,
        "canonical_receipts": len(changelog_paths),
        "corrected_files": [
            "crates/ripr/src/domain/probe.rs",
            "docs/specs/RIPR-SPEC-0151-source-currentness.md",
        ],
        "deleted_files": ["pr-tmp/pr-3280-body.md"],
        "retained_proptest_seed": {
            "path": "crates/ripr/proptest-regressions/output/json/mod.txt",
            "sha256": seed_sha256,
        },
        "non_claims": [
            "no producer inference change",
            "no finding identity change",
            "no classification or count change",
            "no actionability or gate change",
            "no release-scope change",
        ],
    }
    receipt_path.parent.mkdir(parents=True, exist_ok=True)
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(receipt, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
