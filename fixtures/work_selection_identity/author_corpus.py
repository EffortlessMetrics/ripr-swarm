#!/usr/bin/env python3
"""Author the RIPR-SPEC-0235 selected-work identity corpus (#1706, PR A).

Regenerates:
  corpus.json                       the twelve required selection scenarios
  captured/<name>/*.json            the three captured portfolio variants
  provenance.json                   per-file SHA-256 bindings (RIPR-SPEC-0223)

The canonical captured inputs are copied from fixtures/work_portfolio/corpus
and modified only where a scenario needs a different portfolio state. Run
from the repository root:

    python3 fixtures/work_selection_identity/author_corpus.py
"""

import hashlib
import json
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parent
CANONICAL = ROOT.parent / "work_portfolio" / "corpus"
BASE_SHA = "7cb64d60c9dd0dda59586e6b72dcdf150296a26f"
REPOSITORY = "EffortlessMetrics/ripr-swarm"


def load(name):
    with open(CANONICAL / name, encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as handle:
        json.dump(value, handle, indent=2)
        handle.write("\n")


def copy_captured(target):
    target.mkdir(parents=True, exist_ok=True)
    for name in [
        "manifest.json",
        "campaigns.json",
        "issues.json",
        "pull_requests.json",
        "claims.json",
        "local_state.json",
        "surfaces.json",
        "cargo_allow.json",
    ]:
        shutil.copyfile(CANONICAL / name, target / name)


def build_captured_variants():
    multi = ROOT / "captured" / "multi-campaign"
    copy_captured(multi)
    campaigns = load("campaigns.json")
    for campaign in campaigns["campaigns"]:
        if campaign["id"] == "campaign-editor-ux":
            campaign["issues"] = sorted(campaign["issues"] + [9105])
    write_json(multi / "campaigns.json", campaigns)
    issues = load("issues.json")
    for issue in issues["issues"]:
        if issue["number"] == 9105:
            issue["campaigns"] = ["campaign-rust-repair", "campaign-editor-ux"]
    write_json(multi / "issues.json", issues)

    standalone = ROOT / "captured" / "standalone"
    copy_captured(standalone)
    campaigns = load("campaigns.json")
    for campaign in campaigns["campaigns"]:
        if campaign["id"] == "campaign-rust-repair":
            campaign["issues"] = [n for n in campaign["issues"] if n != 9106]
    write_json(standalone / "campaigns.json", campaigns)
    issues = load("issues.json")
    for issue in issues["issues"]:
        if issue["number"] == 9106:
            issue["campaigns"] = []
    write_json(standalone / "issues.json", issues)

    overlap = ROOT / "captured" / "overlap-introduced"
    copy_captured(overlap)
    pull_requests = load("pull_requests.json")
    pull_requests["pull_requests"].append(
        {
            "number": 8899,
            "title": "Overlapping duplicate-family preview fix",
            "state": "open",
            "draft": False,
            "head_branch": "feat/work-9105-overlap",
            "base_branch": "main",
            "linked_issues": [9105],
            "registered_claim": None,
            "review_state": "none",
            "unresolved_review_findings": 0,
            "checks_state": "unknown",
            "worktree_path": "\\\\builds\\ripr-swarm\\wt-9105-overlap",
        }
    )
    write_json(overlap / "pull_requests.json", pull_requests)
    local_state = load("local_state.json")
    local_state["branches"].append({"name": "feat/work-9105-overlap"})
    local_state["worktrees"].append({"path": "\\\\builds\\ripr-swarm\\wt-9105-overlap"})
    write_json(overlap / "local_state.json", local_state)


def campaign_ref(campaign_id, relation):
    return {"id": campaign_id, "relation": relation, "source": "campaigns.json"}


def packet(
    selection_id,
    number,
    action,
    campaign_refs,
    basis_sha=BASE_SHA,
    expected_head=BASE_SHA,
    pull_request=None,
    work_item=None,
    requirements=None,
    specs=None,
    slices=None,
    overlaps=None,
    role="writer",
    context_profile="bounded-candidate",
    budget="single-agent",
    single_agent=False,
    claim_boundary="Implement only the recorded slices; no campaign, spec or claim mutation.",
    stop_conditions=None,
    legacy_active_goal_ref=None,
    legacy_current_work_item_ref=None,
):
    return {
        "selection_id": selection_id,
        "repository": REPOSITORY,
        "candidate_id": "candidate:issue:{}".format(number),
        "issue": {"number": number, "identity": "issue:{}".format(number)},
        "work_item": (
            {"id": work_item, "identity": "work-item:{}".format(work_item)}
            if work_item
            else None
        ),
        "pull_request": pull_request,
        "lifecycle_action": action,
        "accepted_requirements": requirements or [],
        "spec_refs": specs or [],
        "implementation_slices": slices or [],
        "campaign_refs": campaign_refs,
        "basis_sha": basis_sha,
        "expected_head": expected_head,
        "overlaps": overlaps
        or {
            "issues": [],
            "pull_requests": [],
            "claims": [],
            "worktrees": [],
            "resources": [],
        },
        "role_context_budget": {
            "role": role,
            "context_profile": context_profile,
            "budget": budget,
            "single_agent": single_agent,
        },
        "claim_boundary": claim_boundary,
        "stop_conditions": stop_conditions
        or ["acceptance rows covered or explicitly omitted with reasons"],
        "legacy_active_goal_ref": legacy_active_goal_ref,
        "legacy_current_work_item_ref": legacy_current_work_item_ref,
    }


def case(packet_value=None, legacy_packet=None, expect="pass", violations=None,
         disposition="single_scoped_pr"):
    return {
        "packet": packet_value,
        "legacy_packet": legacy_packet,
        "expect": expect,
        "expect_violations": violations or [],
        "plan_disposition": disposition,
    }


def build_corpus():
    scenarios = [
        {
            "id": "scenario-01-selected-issue-one-campaign",
            "title": "Selected issue belongs to one campaign",
            "captured": "work_portfolio/corpus",
            "cases": [
                case(
                    packet(
                        "selection:issue:9105:start-build",
                        9105,
                        "start_build",
                        [campaign_ref("campaign-rust-repair", "member")],
                        requirements=["REQ-dup-family"],
                        specs=["RIPR-SPEC-0202"],
                        slices=["slice-9105"],
                    ),
                    disposition="append_to_named_campaign",
                )
            ],
        },
        {
            "id": "scenario-02-selected-issue-several-related-campaigns",
            "title": "Selected issue belongs to several related campaigns",
            "captured": "work_selection_identity/captured/multi-campaign",
            "cases": [
                case(
                    packet(
                        "selection:issue:9105:start-build",
                        9105,
                        "start_build",
                        [
                            campaign_ref("campaign-editor-ux", "member"),
                            campaign_ref("campaign-rust-repair", "member"),
                        ],
                        requirements=["REQ-dup-family"],
                        specs=["RIPR-SPEC-0202"],
                        slices=["slice-9105"],
                    ),
                    disposition="multi_pr_campaign",
                )
            ],
        },
        {
            "id": "scenario-03-standalone-no-campaign-placement",
            "title": "Valid standalone work with no campaign placement",
            "captured": "work_selection_identity/captured/standalone",
            "cases": [
                case(
                    packet(
                        "selection:issue:9106:start-build",
                        9106,
                        "start_build",
                        [],
                        requirements=["REQ-dup-family"],
                        specs=["RIPR-SPEC-0202"],
                        slices=["slice-9106"],
                    ),
                    disposition="standalone_issue_work",
                )
            ],
        },
        {
            "id": "scenario-04-two-campaigns-independent-disjoint-resources",
            "title": "Two campaigns with independent candidates and disjoint writer resources",
            "captured": "work_portfolio/corpus",
            "cases": [
                case(
                    packet(
                        "selection:issue:1693:start-build",
                        1693,
                        "start_build",
                        [campaign_ref("campaign-editor-ux", "member")],
                        requirements=["REQ-editor-preview"],
                        specs=["RIPR-SPEC-0206"],
                        slices=["slice-1693"],
                        overlaps={
                            "issues": [],
                            "pull_requests": [],
                            "claims": [],
                            "worktrees": [],
                            "resources": ["output-contract:preview-panel"],
                        },
                    ),
                    disposition="single_scoped_pr",
                ),
                case(
                    packet(
                        "selection:issue:9105:start-build",
                        9105,
                        "start_build",
                        [campaign_ref("campaign-rust-repair", "member")],
                        requirements=["REQ-dup-family"],
                        specs=["RIPR-SPEC-0202"],
                        slices=["slice-9105"],
                    ),
                    disposition="single_scoped_pr",
                ),
            ],
        },
        {
            "id": "scenario-05-existing-open-pr-selected-for-repair",
            "title": "Existing open PR selected for review/repair instead of a new build",
            "captured": "work_portfolio/corpus",
            "cases": [
                case(
                    packet(
                        "selection:issue:9101:repair-review",
                        9101,
                        "repair_review",
                        [campaign_ref("campaign-rust-repair", "member")],
                        pull_request=8801,
                        specs=["RIPR-SPEC-0200"],
                        overlaps={
                            "issues": [],
                            "pull_requests": [8801],
                            "claims": ["claim-9101"],
                            "worktrees": ["<root>/wt-9101"],
                            "resources": ["output-contract:rust-json"],
                        },
                        claim_boundary="Repair PR #8801 only: address each unresolved review finding, reply, resolve, re-request review.",
                    ),
                    disposition="already_planned",
                )
            ],
        },
        {
            "id": "scenario-06-legacy-writer-authorization-fails",
            "title": "Legacy active-goal packet attempting writer authorization fails",
            "captured": "work_portfolio/corpus",
            "cases": [
                case(
                    legacy_packet={
                        "legacy_active_goal_ref": "campaign-rust-repair",
                        "legacy_current_work_item_ref": "issue:9105",
                        "attempted_authorities": ["writer"],
                    },
                    expect="fail",
                    violations=[
                        {"law": "legacy_compatibility", "route": "reject_legacy_authority"}
                    ],
                    disposition="blocked_by_contract_or_decision",
                )
            ],
        },
        {
            "id": "scenario-07-snapshot-change-issue-head-compatible",
            "title": "Portfolio snapshot changes while selected issue/head remains compatible",
            "captured": "work_portfolio/variants/stale_local",
            "cases": [
                case(
                    packet(
                        "selection:issue:9105:start-build",
                        9105,
                        "start_build",
                        [campaign_ref("campaign-rust-repair", "member")],
                        requirements=["REQ-dup-family"],
                        specs=["RIPR-SPEC-0202"],
                        slices=["slice-9105"],
                    ),
                    disposition="append_to_named_campaign",
                )
            ],
        },
        {
            "id": "scenario-08-overlap-introduced-forces-reconcile",
            "title": "Portfolio change introduces an overlapping PR and forces reconcile",
            "captured": "work_selection_identity/captured/overlap-introduced",
            "cases": [
                case(
                    packet(
                        "selection:issue:9105:start-build",
                        9105,
                        "start_build",
                        [campaign_ref("campaign-rust-repair", "member")],
                        requirements=["REQ-dup-family"],
                        specs=["RIPR-SPEC-0202"],
                        slices=["slice-9105"],
                    ),
                    expect="fail",
                    violations=[
                        {"law": "overlap_visibility", "route": "reconcile_selection"}
                    ],
                    disposition="root_portfolio_decision_required",
                )
            ],
        },
        {
            "id": "scenario-09-wrong-identities-fail-visibly",
            "title": "Wrong repository, issue, action, basis, worktree or head fails visibly",
            "captured": "work_portfolio/corpus",
            "cases": [
                case(
                    packet(
                        "selection:issue:9105:start-build",
                        9105,
                        "start_build",
                        [campaign_ref("campaign-rust-repair", "member")],
                        requirements=["REQ-dup-family"],
                        specs=["RIPR-SPEC-0202"],
                        slices=["slice-9105"],
                    )
                    | {"repository": "SomeoneElse/other-repo"},
                    expect="fail",
                    violations=[
                        {"law": "repository_identity", "route": "recompile_basis"}
                    ],
                    disposition="root_portfolio_decision_required",
                ),
                case(
                    packet(
                        "selection:issue:999999:start-build",
                        999999,
                        "start_build",
                        [],
                    ),
                    expect="fail",
                    violations=[{"law": "subject_identity", "route": "reconcile_selection"}],
                    disposition="root_portfolio_decision_required",
                ),
                case(
                    packet(
                        "selection:issue:9105:resume-pr",
                        9105,
                        "resume_pr",
                        [campaign_ref("campaign-rust-repair", "member")],
                    ),
                    expect="fail",
                    violations=[{"law": "action_identity", "route": "reconcile_selection"}],
                    disposition="root_portfolio_decision_required",
                ),
                case(
                    packet(
                        "selection:issue:9105:start-build",
                        9105,
                        "start_build",
                        [campaign_ref("campaign-rust-repair", "member")],
                        basis_sha="0" * 64,
                        expected_head=None,
                    ),
                    expect="fail",
                    violations=[{"law": "basis_identity", "route": "recompile_basis"}],
                    disposition="root_portfolio_decision_required",
                ),
                case(
                    packet(
                        "selection:issue:9101:repair-review",
                        9101,
                        "repair_review",
                        [campaign_ref("campaign-rust-repair", "member")],
                        pull_request=8801,
                        specs=["RIPR-SPEC-0200"],
                        overlaps={
                            "issues": [],
                            "pull_requests": [8801],
                            "claims": ["claim-9101"],
                            "worktrees": ["<root>/wt-9101", "<root>/wt-unknown"],
                            "resources": ["output-contract:rust-json"],
                        },
                    ),
                    expect="fail",
                    violations=[
                        {"law": "worktree_identity", "route": "reconcile_resources"}
                    ],
                    disposition="root_portfolio_decision_required",
                ),
                case(
                    packet(
                        "selection:issue:9105:start-build",
                        9105,
                        "start_build",
                        [campaign_ref("campaign-rust-repair", "member")],
                        expected_head="f" * 64,
                    ),
                    expect="fail",
                    violations=[{"law": "head_identity", "route": "reconcile_head"}],
                    disposition="root_portfolio_decision_required",
                ),
            ],
        },
        {
            "id": "scenario-10-burndown-partial-acceptance-uncovered",
            "title": "Burn-down stays partial after one work item merges but acceptance is uncovered",
            "captured": "work_portfolio/corpus",
            "cases": [
                case(
                    packet(
                        "selection:issue:9109:verify-current-head",
                        9109,
                        "verify_current_head",
                        [campaign_ref("campaign-rust-repair", "member")],
                        specs=["RIPR-SPEC-0205"],
                        overlaps={
                            "issues": [],
                            "pull_requests": [8806],
                            "claims": [],
                            "worktrees": [],
                            "resources": [],
                        },
                        claim_boundary="Verify merged PR #8806 against current main and reconcile issue closeout; no new build.",
                    ),
                    disposition="focused_tracker_only",
                )
            ],
        },
        {
            "id": "scenario-11-fresh-root-resume-from-artifacts",
            "title": "Fresh root resumes from portfolio/selection/claim/PR artifacts without chat or singleton goal-file authority",
            "captured": "work_portfolio/corpus",
            "cases": [
                case(
                    packet(
                        "selection:issue:9102:resume-pr",
                        9102,
                        "resume_pr",
                        [campaign_ref("campaign-rust-repair", "historical")],
                        pull_request=8802,
                        specs=["RIPR-SPEC-0200"],
                        overlaps={
                            "issues": [],
                            "pull_requests": [8802],
                            "claims": ["claim-9102"],
                            "worktrees": ["<root>/wt-9102"],
                            "resources": ["output-contract:rust-json"],
                        },
                        legacy_active_goal_ref="campaign-rust-repair",
                        claim_boundary="Resume PR #8802 under the existing claim: push, wait for checks, rerun proof on the published head.",
                    ),
                    disposition="already_planned",
                )
            ],
        },
        {
            "id": "scenario-12-narrow-single-agent-despite-active-campaigns",
            "title": "Narrow one-file issue stays single-agent despite other active campaigns",
            "captured": "work_portfolio/corpus",
            "cases": [
                case(
                    packet(
                        "selection:issue:9202:start-build",
                        9202,
                        "start_build",
                        [campaign_ref("campaign-editor-ux", "member")],
                        requirements=["REQ-editor-badges"],
                        specs=["RIPR-SPEC-0207"],
                        slices=["slice-9202"],
                        single_agent=True,
                        claim_boundary="Edit only editors/vscode/src/badge.ts and its tests; other campaigns stay independent.",
                    ),
                    disposition="single_scoped_pr",
                )
            ],
        },
    ]
    corpus = {
        "schema_version": "work_selection_identity_corpus.v1",
        "repository": REPOSITORY,
        "selection_policy": "explicit-root-selection",
        "selection_policy_version": "v1",
        "scenarios": scenarios,
    }
    write_json(ROOT / "corpus.json", corpus)


def build_provenance():
    files = []
    for path in sorted(ROOT.rglob("*.json")):
        if path.name == "provenance.json":
            continue
        relative = path.relative_to(ROOT).as_posix()
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        files.append({"path": relative, "sha256": digest})
    provenance = {
        "schema_version": "work_selection_identity_provenance.v1",
        "repository": REPOSITORY,
        "captured_at": "2026-10-05T12:00:00Z",
        "capture_method": "authored captured-input corpora for the RIPR-SPEC-0235 selected-work identity law checker; regenerate with fixtures/work_selection_identity/author_corpus.py",
        "files": files,
    }
    write_json(ROOT / "provenance.json", provenance)


def main():
    build_captured_variants()
    build_corpus()
    build_provenance()
    print("authored fixtures/work_selection_identity")


if __name__ == "__main__":
    main()
