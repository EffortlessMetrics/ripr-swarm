"""Author fixtures/work_plan_identity/corpus.json for RIPR-SPEC-0276 (#1706 PR B).

Reuses the committed RIPR-SPEC-0244 selection packets and captured
directories by reference; no captured bytes are duplicated.
"""

import copy
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
SELECTION = json.loads(
    (ROOT / "fixtures/work_selection_identity/corpus.json").read_text(encoding="utf-8")
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


def plan(plan_id, selection, snapshot, disposition, target=None, reorders=False,
         blocked_by=(), derived_from=None, legacy_goal=None, legacy_item=None,
         repository=REPO) -> dict:
    return {
        "plan_id": plan_id,
        "repository": repository,
        "portfolio_snapshot": snapshot,
        "basis_sha": HEAD,
        "selection": copy.deepcopy(selection),
        "disposition": disposition,
        "target_campaign": target,
        "reorders_portfolio": reorders,
        "blocked_by": list(blocked_by),
        "derived_from": derived_from if derived_from is not None else [
            "fixtures/work_selection_identity/corpus.json"
        ],
        "legacy_active_goal_ref": legacy_goal,
        "legacy_current_work_item_ref": legacy_item,
    }


def case(body, expect="pass", violations=()):
    return {
        "plan": body,
        "expect": expect,
        "expect_violations": [
            {"law": law, "route": route} for law, route in violations
        ],
    }


in_campaign_9105 = packet("scenario-01-selected-issue-one-campaign")
standalone_9106 = packet("scenario-03-standalone-no-campaign-placement")
blocked_9109 = packet("scenario-10-burndown-partial-acceptance-uncovered")

scenarios = [
    {
        "id": "plan-01-single-scoped-pr-disposition-passes",
        "title": "A single scoped PR disposition for one selected issue passes",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(plan(
                "plan:single_scoped_pr:issue:9105",
                in_campaign_9105,
                SNAPSHOT_MAIN,
                "single_scoped_pr",
                derived_from=[
                    "fixtures/work_selection_identity/corpus.json#scenario-01-selected-issue-one-campaign",
                ],
            )),
        ],
    },
    {
        "id": "plan-02-standalone-issue-disposition-passes",
        "title": "A standalone issue disposition without campaign placement passes",
        "captured": "work_selection_identity/captured/standalone",
        "cases": [
            case(plan(
                "plan:standalone_issue_work:issue:9106",
                standalone_9106,
                SNAPSHOT_STANDALONE,
                "standalone_issue_work",
                derived_from=[
                    "fixtures/work_selection_identity/corpus.json#scenario-03-standalone-no-campaign-placement",
                ],
            )),
        ],
    },
    {
        "id": "plan-03-append-to-named-campaign-disposition-passes",
        "title": "An append disposition naming a recorded campaign passes",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(plan(
                "plan:append_to_named_campaign:issue:9105",
                in_campaign_9105,
                SNAPSHOT_MAIN,
                "append_to_named_campaign",
                target="campaign-rust-repair",
                derived_from=[
                    "fixtures/work_selection_identity/corpus.json#scenario-01-selected-issue-one-campaign",
                ],
            )),
        ],
    },
    {
        "id": "plan-04-blocked-by-contract-disposition-passes",
        "title": "A blocked disposition naming its blocking contract passes",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(plan(
                "plan:blocked_by_contract_or_decision:issue:9109",
                blocked_9109,
                SNAPSHOT_MAIN,
                "blocked_by_contract_or_decision",
                blocked_by=["RIPR-SPEC-0205 acceptance uncovered"],
                derived_from=[
                    "fixtures/work_selection_identity/corpus.json#scenario-10-burndown-partial-acceptance-uncovered",
                ],
            )),
        ],
    },
    {
        "id": "plan-05-reorder-and-wrong-inputs-fail",
        "title": "Reorder, missing or unknown campaign targets, unnamed blockers, a foreign repository and an unknown legacy goal each fail with the exact law/route pair",
        "captured": "work_portfolio/corpus",
        "cases": [
            # 0: read-only planning cannot reorder the portfolio.
            case(plan(
                "plan:reorders-portfolio:issue:9105",
                in_campaign_9105,
                SNAPSHOT_MAIN,
                "single_scoped_pr",
                reorders=True,
            ), "fail", [("subject_identity", "reconcile_selection")]),
            # 1: an append disposition must name its target campaign.
            case(plan(
                "plan:append-without-target:issue:9105",
                in_campaign_9105,
                SNAPSHOT_MAIN,
                "append_to_named_campaign",
            ), "fail", [("campaign_ref_identity", "reconcile_selection")]),
            # 2: the named target campaign must be recorded.
            case(plan(
                "plan:append-unknown-target:issue:9105",
                in_campaign_9105,
                SNAPSHOT_MAIN,
                "append_to_named_campaign",
                target="campaign-unknown",
            ), "fail", [("campaign_ref_identity", "reconcile_selection")]),
            # 3: a blocked disposition must name its blocker.
            case(plan(
                "plan:blocked-without-blocker:issue:9109",
                blocked_9109,
                SNAPSHOT_MAIN,
                "blocked_by_contract_or_decision",
            ), "fail", [("subject_identity", "reconcile_selection")]),
            # 4: a plan for another repository is wrong input.
            case(plan(
                "plan:foreign-repository:issue:9105",
                in_campaign_9105,
                SNAPSHOT_MAIN,
                "single_scoped_pr",
                repository="someone/else",
            ), "fail", [("repository_identity", "recompile_basis")]),
            # 5: a legacy goal ref must resolve to a recorded historical campaign.
            case(plan(
                "plan:legacy-unknown-goal:issue:9109",
                blocked_9109,
                SNAPSHOT_MAIN,
                "blocked_by_contract_or_decision",
                blocked_by=["RIPR-SPEC-0205 acceptance uncovered"],
                legacy_goal="campaign-unknown-goal",
            ), "fail", [("legacy_compatibility", "reconcile_selection")]),
        ],
    },
]

corpus = {
    "schema_version": "work_plan_identity_corpus.v1",
    "repository": REPO,
    "selection_policy": "explicit-root-selection",
    "selection_policy_version": "v1",
    "scenarios": scenarios,
}

OUT = Path(__file__).resolve().parent
(OUT / "corpus.json").write_text(
    json.dumps(corpus, indent=1) + "\n", encoding="utf-8"
)

import hashlib

digest = hashlib.sha256((OUT / "corpus.json").read_bytes()).hexdigest()
provenance = {
    "schema_version": "work_plan_identity_provenance.v1",
    "repository": REPO,
    "captured_at": "2026-10-09T12:00:00Z",
    "capture_method": "authored plan-disposition corpus reusing the committed RIPR-SPEC-0244 selection packets and captured directories by reference",
    "files": [{"path": "corpus.json", "sha256": digest}],
}
(OUT / "provenance.json").write_text(
    json.dumps(provenance, indent=1) + "\n", encoding="utf-8"
)
print("wrote corpus.json and provenance.json")
