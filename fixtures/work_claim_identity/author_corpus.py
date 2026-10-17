"""Author fixtures/work_claim_identity/corpus.json for RIPR-SPEC-0270 (#1706 PR B).

Reuses the committed RIPR-SPEC-0244 selection packets and captured
directories by reference; no captured bytes are duplicated. Packets that
claim a writer worktree add that worktree to the selection overlaps, as the
RIPR-SPEC-0244 worktree identity law requires.
"""

import copy
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
SELECTION = json.loads(
    (ROOT / "fixtures/work_selection_identity/corpus.json").read_text()
)

HEAD = "7cb64d60c9dd0dda59586e6b72dcdf150296a26f"
SNAPSHOT_MAIN = "work-portfolio:sha256:f13413b564d05b401866a5194c6f8265fd03c890597a904648c5c332b3b92005"
SNAPSHOT_STANDALONE = "work-portfolio:sha256:06c8d9fa8eb6feee4e6d2815b56dd8567d39460be40134e158d691a7c8bc2715"
REPO = "EffortlessMetrics/ripr-swarm"


def packet(scenario_id: str, case_index: int = 0) -> dict:
    scenario = next(s for s in SELECTION["scenarios"] if s["id"] == scenario_id)
    case = scenario["cases"][case_index]
    assert case["expect"] == "pass", scenario["id"]
    return copy.deepcopy(case["packet"])


def with_worktree(sel: dict, worktree: str) -> dict:
    out = copy.deepcopy(sel)
    if worktree not in out["overlaps"]["worktrees"]:
        out["overlaps"]["worktrees"].append(worktree)
        out["overlaps"]["worktrees"].sort()
    return out


def claim(claim_id, selection, snapshot, worktree, legacy_goal=None,
          legacy_item=None, basis_sha=HEAD) -> dict:
    return {
        "claim_id": claim_id,
        "repository": REPO,
        "portfolio_snapshot": snapshot,
        "basis_sha": basis_sha,
        "selection": copy.deepcopy(selection),
        "worktree": worktree,
        "claim_kind": "writer",
        "legacy_active_goal_ref": legacy_goal,
        "legacy_current_work_item_ref": legacy_item,
    }


def case(body, expect, violations=()):
    return {
        "claim": body,
        "expect": expect,
        "expect_violations": [
            {"law": law, "route": route} for law, route in violations
        ],
    }


base_9105 = packet("scenario-01-selected-issue-one-campaign")
standalone_9106 = packet("scenario-03-standalone-no-campaign-placement")
editor_1693 = packet("scenario-04-two-campaigns-independent-disjoint-resources", 0)
repair_9101 = packet("scenario-05-existing-open-pr-selected-for-repair")
resume_9102 = packet("scenario-11-fresh-root-resume-from-artifacts")

WT9101 = "<root>/wt-9101"
WT9102 = "<root>/wt-9102"
WT9103 = "<root>/wt-9103"
WT9107A = "<root>/wt-9107-a"
WT9108 = "<root>/wt-9108"

scenarios = [
    {
        "id": "claim-01-writer-claims-for-distinct-selections-pass",
        "title": "Writer claims for three distinct selections on three distinct worktrees pass",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(claim("claim-test-9105",
                       with_worktree(base_9105, WT9103), SNAPSHOT_MAIN, WT9103), "pass"),
            case(claim("claim-test-9101", repair_9101, SNAPSHOT_MAIN, WT9101), "pass"),
            case(claim("claim-test-9102", resume_9102, SNAPSHOT_MAIN, WT9102), "pass"),
        ],
    },
    {
        "id": "claim-02-standalone-selection-claim-passes",
        "title": "A standalone selection with zero campaigns is claimable",
        "captured": "work_selection_identity/captured/standalone",
        "cases": [
            case(claim("claim-test-9106",
                       with_worktree(standalone_9106, WT9108),
                       SNAPSHOT_STANDALONE, WT9108), "pass"),
        ],
    },
    {
        "id": "claim-03-stale-basis-reconciles-without-takeover",
        "title": "A worktree conflict reconciles without takeover and a stale basis keeps the claim recorded",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(claim("claim-test-a",
                       with_worktree(base_9105, WT9107A), SNAPSHOT_MAIN, WT9107A), "pass"),
            case(claim("claim-test-b",
                       with_worktree(editor_1693, WT9107A), SNAPSHOT_MAIN, WT9107A),
                 "fail", [("worktree_identity", "reconcile_resources")]),
            case(claim("claim-test-c",
                       with_worktree(editor_1693, WT9108), SNAPSHOT_MAIN, WT9108,
                       basis_sha="1" * 40), "fail", [("basis_identity", "recompile_basis")]),
        ],
    },
    {
        "id": "claim-04-double-claim-of-one-selection-fails",
        "title": "One selection cannot be claimed by two worktrees",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(claim("claim-test-d",
                       with_worktree(base_9105, WT9107A), SNAPSHOT_MAIN, WT9107A), "pass"),
            case(claim("claim-test-e",
                       with_worktree(base_9105, WT9108), SNAPSHOT_MAIN, WT9108),
                 "fail", [("subject_identity", "reconcile_selection")]),
        ],
    },
    {
        "id": "claim-05-legacy-claim-read-only-migration-posture",
        "title": "Legacy refs on a claim resolve read-only with migration posture",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(claim("claim-test-legacy", resume_9102, SNAPSHOT_MAIN, WT9102,
                       legacy_goal="campaign-rust-repair",
                       legacy_item="issue:9102"), "pass"),
            case(claim("claim-test-legacy-bad", repair_9101, SNAPSHOT_MAIN, WT9101,
                       legacy_item="issue:9999"),
                 "fail", [("legacy_compatibility", "reconcile_selection")]),
        ],
    },
]

corpus = {
    "schema_version": "work_claim_identity_corpus.v1",
    "repository": REPO,
    "selection_policy": "explicit-root-selection",
    "selection_policy_version": "v1",
    "scenarios": scenarios,
}

out = ROOT / "fixtures/work_claim_identity/corpus.json"
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(corpus, indent=1) + "\n")
print("wrote", out)
