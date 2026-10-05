#!/usr/bin/env python3
"""Python mirror of the RIPR-SPEC-0235 identity law engine.

Replays fixtures/work_selection_identity/corpus.json against the same laws
implemented in xtask/src/work_selection_identity.rs so corpus/logic
mismatches surface before CI. Not shipped; a development aid only.
"""

import json
import sys
from pathlib import Path

FIXTURES = Path("fixtures")


def load_captured(rel):
    root = FIXTURES / rel
    manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
    out = {"manifest": manifest}
    for name in ["issues", "pull_requests", "claims", "campaigns"]:
        out[name] = json.loads((root / f"{name}.json").read_text(encoding="utf-8"))
    return out


def compile_snapshot(captured):
    """Approximate the #1704 classification for the corpus issues we use."""
    issues = {i["number"]: i for i in captured["issues"]["issues"]}
    prs = captured["pull_requests"]["pull_requests"]
    claims = captured["claims"]["claims"]
    campaigns = {c["id"]: c for c in captured["campaigns"]["campaigns"]}
    return issues, prs, claims, campaigns


def candidate_kind(number, issues, prs, claims):
    open_prs = [p for p in prs if p["state"] == "open" and number in p["linked_issues"]]
    merged_prs = [p for p in prs if p["state"] == "merged" and number in p["linked_issues"]]
    active = [c for c in claims if c.get("issue") == number and c["state"] == "active"]
    if len(open_prs) > 1:
        return "verify_current_head"
    if open_prs:
        p = open_prs[0]
        if p["review_state"] == "changes_requested" or p["unresolved_review_findings"] > 0:
            return "repair_review"
        if not p["draft"] and p["review_state"] == "approved" and p["checks_state"] == "success":
            return "merge_ready"
        return "resume_pr"
    if issues[number]["lifecycle_disposition"] == "completed":
        return "complete"
    if merged_prs:
        return "verify_current_head"
    if issues[number]["blocked_by"]:
        return "blocked"
    if active:
        return "resume_pr"
    return "start_build"


def live_overlaps(number, prs, claims, issues):
    o = {"pull_requests": [], "claims": [], "worktrees": [], "resources": []}
    for p in prs:
        if p["state"] == "open" and number in p["linked_issues"]:
            o["pull_requests"].append(p["number"])
            if p.get("worktree_path"):
                o["worktrees"].append(portable(p["worktree_path"], ROOT_SPELL))
    for c in claims:
        if c.get("issue") == number and c["state"] == "active":
            o["claims"].append(c["id"])
            if c.get("worktree"):
                o["worktrees"].append(portable(c["worktree"], ROOT_SPELL))
    o["worktrees"] = sorted(set(o["worktrees"]))
    o["resources"] = list(issues[number].get("conflict_resources", []))
    return o


def portable(path, root):
    norm = path.replace("\\", "/")
    nroot = root.replace("\\", "/").rstrip("/")
    if norm == nroot:
        return "<root>"
    if norm.startswith(nroot + "/"):
        return "<root>/" + norm[len(nroot) + 1 :]
    return norm


def check_packet(packet, basis_sha, repo, issues, prs, claims, campaigns):
    v = []
    sha = basis_sha
    if packet["repository"] != repo:
        v.append(("repository_identity", "recompile_basis"))
    number = packet["issue"]["number"] if packet["issue"] else None
    known = number in issues if number is not None else False
    if packet["issue"] and packet["issue"]["identity"] != f"issue:{number}":
        v.append(("subject_identity", "reconcile_selection"))
    if number is not None and not known:
        v.append(("subject_identity", "reconcile_selection"))
    if packet["work_item"] and packet["work_item"]["identity"] != f"work-item:{packet['work_item']['id']}":
        v.append(("subject_identity", "reconcile_selection"))
    if packet["pull_request"] and not any(p["number"] == packet["pull_request"] for p in prs):
        v.append(("subject_identity", "reconcile_selection"))
    if known:
        kind = candidate_kind(number, issues, prs, claims)
        if packet["candidate_id"] != f"candidate:issue:{number}":
            v.append(("action_identity", "reconcile_selection"))
        if packet["lifecycle_action"] != kind:
            v.append(("action_identity", "reconcile_selection"))
        if issues[number]["single_agent_preferred"] and not packet["role_context_budget"]["single_agent"]:
            v.append(("subject_identity", "reconcile_selection"))
    if packet["basis_sha"] != sha:
        v.append(("basis_identity", "recompile_basis"))
    if packet["expected_head"] and packet["expected_head"] != sha:
        v.append(("head_identity", "reconcile_head"))
    known_wt = set()
    for c in claims:
        if c.get("worktree"):
            known_wt.add(portable(c["worktree"], ROOT_SPELL))
    for p in prs:
        if p.get("worktree_path"):
            known_wt.add(portable(p["worktree_path"], ROOT_SPELL))
    for wt in packet["overlaps"]["worktrees"]:
        if wt not in known_wt:
            v.append(("worktree_identity", "reconcile_resources"))
    seen = set()
    for ref in packet["campaign_refs"]:
        cid = ref["id"]
        if cid in seen:
            v.append(("campaign_ref_identity", "reconcile_selection"))
        seen.add(cid)
        if cid not in campaigns:
            v.append(("campaign_ref_identity", "reconcile_selection"))
        elif ref["relation"] == "member" and known and number not in campaigns[cid]["issues"]:
            v.append(("campaign_ref_identity", "reconcile_selection"))
    legacy = packet.get("legacy_active_goal_ref")
    if legacy:
        resolved = any(r["id"] == legacy and r["relation"] == "historical" for r in packet["campaign_refs"])
        if not resolved or legacy not in campaigns:
            v.append(("legacy_compatibility", "reconcile_selection"))
    legacy_wi = packet.get("legacy_current_work_item_ref")
    if legacy_wi and legacy_wi != (f"issue:{number}" if number is not None else None):
        v.append(("legacy_compatibility", "reconcile_selection"))
    if known:
        live = live_overlaps(number, prs, claims, issues)
        for key, law in [("pull_requests", "overlap_visibility"), ("claims", "overlap_visibility"), ("worktrees", "overlap_visibility"), ("resources", "overlap_visibility")]:
            for item in live[key]:
                if item not in packet["overlaps"][key]:
                    v.append((law, "reconcile_selection"))
    return v


def check_legacy(legacy, campaigns):
    v = []
    if legacy["attempted_authorities"]:
        v.append(("legacy_compatibility", "reject_legacy_authority"))
    ref = legacy.get("legacy_active_goal_ref")
    if ref and ref not in campaigns:
        v.append(("legacy_compatibility", "reconcile_selection"))
    return v


def main():
    corpus = json.loads((FIXTURES / "work_selection_identity" / "corpus.json").read_text(encoding="utf-8"))
    failures = 0
    total = 0
    for scenario in corpus["scenarios"]:
        captured = load_captured(scenario["captured"])
        manifest = captured["manifest"]
        global ROOT_SPELL
        ROOT_SPELL = manifest["root"]
        issues, prs, claims, campaigns = compile_snapshot(captured)
        for index, case in enumerate(scenario["cases"]):
            total += 1
            if case["packet"]:
                violations = check_packet(
                    case["packet"], manifest["default_branch_sha"], manifest["repository"],
                    issues, prs, claims, campaigns,
                )
            else:
                violations = check_legacy(case["legacy_packet"], campaigns)
            if case["expect"] == "pass":
                passed = not violations
            else:
                expected = {(e["law"], e["route"]) for e in case["expect_violations"]}
                passed = bool(violations) and expected.issubset(set(violations))
            status = "PASS" if passed else "FAIL"
            if not passed:
                failures += 1
            print(f"{status} {scenario['id']} case {index}: {violations}")
    print(f"{total - failures}/{total} cases meet their pinned expectations")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
