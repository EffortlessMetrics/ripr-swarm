"""Author fixtures/work_burndown_identity/corpus.json for RIPR-SPEC-0274 (#1706 PR B).

Reuses the committed RIPR-SPEC-0244 selection packets and captured
directories by reference; no captured bytes are duplicated.
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

ACCEPTANCE_COVERED = "acceptance_covered"
ACCEPTANCE_OPEN = "acceptance_open"
PROOF_CURRENT = "proof_current"
PROOF_STALE = "proof_stale"
MERGED = "merged"

COMPLETE = "complete"
PARTIAL = "partial"
OPEN = "open"


def packet(scenario_id: str, case_index: int = 0) -> dict:
    scenario = next(s for s in SELECTION["scenarios"] if s["id"] == scenario_id)
    case = scenario["cases"][case_index]
    assert case["expect"] == "pass", scenario["id"]
    return copy.deepcopy(case["packet"])


def row(source, status):
    return {"source": source, "status": status}


def burndown(burndown_id, selection, snapshot, evidence, completion,
             campaign_records=(), derived_from=None, legacy_goal=None,
             legacy_item=None) -> dict:
    return {
        "burndown_id": burndown_id,
        "repository": REPO,
        "portfolio_snapshot": snapshot,
        "basis_sha": HEAD,
        "selection": copy.deepcopy(selection),
        "current_main_sha": HEAD,
        "issue_evidence": evidence,
        "completion": completion,
        "campaign_records": list(campaign_records),
        "derived_from": derived_from if derived_from is not None else [
            "fixtures/work_selection_identity/corpus.json"
        ],
        "legacy_active_goal_ref": legacy_goal,
        "legacy_current_work_item_ref": legacy_item,
    }


def case(body, expect, violations=()):
    return {
        "burndown": body,
        "expect": expect,
        "expect_violations": [
            {"law": law, "route": route} for law, route in violations
        ],
    }


base_9105 = packet("scenario-01-selected-issue-one-campaign")
standalone_9106 = packet("scenario-03-standalone-no-campaign-placement")
partial_9109 = packet("scenario-10-burndown-partial-acceptance-uncovered")
resume_9102 = packet("scenario-11-fresh-root-resume-from-artifacts")

scenarios = [
    {
        "id": "burndown-01-complete-evidence-burn-down-passes",
        "title": "A burn-down whose evidence rows are all covered, current or merged derives complete",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(burndown(
                "burndown:selection:start_build:issue:9105",
                base_9105, SNAPSHOT_MAIN,
                [
                    row("issue:9105 acceptance", ACCEPTANCE_COVERED),
                    row("RIPR-SPEC-0202 acceptance", ACCEPTANCE_COVERED),
                    row("slice-9105 exact-head review", PROOF_CURRENT),
                    row("slice-9105 implementation", MERGED),
                ],
                COMPLETE,
                campaign_records=["campaign-rust-repair"],
            ), "pass"),
        ],
    },
    {
        "id": "burndown-02-standalone-burn-down-passes",
        "title": "A standalone selection with zero campaigns derives an honest burn-down",
        "captured": "work_selection_identity/captured/standalone",
        "cases": [
            case(burndown(
                "burndown:selection:start_build:issue:9106:standalone",
                standalone_9106, SNAPSHOT_STANDALONE,
                [
                    row("issue:9106 acceptance", ACCEPTANCE_OPEN),
                    row("slice-9106 implementation", MERGED),
                ],
                PARTIAL,
            ), "pass"),
        ],
    },
    {
        "id": "burndown-03-partial-after-one-campaign-item-merges",
        "title": "Burn-down stays partial after one campaign work item merges but acceptance stays open",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(burndown(
                "burndown:selection:verify_current_head:issue:9109:partial",
                partial_9109, SNAPSHOT_MAIN,
                [
                    row("campaign-rust-repair work item", MERGED),
                    row("issue:9109 acceptance", ACCEPTANCE_OPEN),
                ],
                PARTIAL,
                campaign_records=["campaign-rust-repair"],
                derived_from=[
                    "fixtures/work_portfolio/corpus/pull_requests.json",
                    "fixtures/work_portfolio/corpus/issues.json",
                ],
            ), "pass"),
        ],
    },
    {
        "id": "burndown-04-campaign-override-and-wrong-inputs-fail",
        "title": "A campaign override of open evidence, a wrong current main, an unknown campaign record, an evidence-free burn-down and a wrong repository fail visibly",
        "captured": "work_portfolio/corpus",
        "cases": [],
    },
    {
        "id": "burndown-05-legacy-read-only-migration-posture",
        "title": "Legacy refs on a burn-down resolve read-only with migration posture",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(burndown(
                "burndown:selection:resume_pr:issue:9102",
                resume_9102, SNAPSHOT_MAIN,
                [
                    row("issue:9102 acceptance", ACCEPTANCE_COVERED),
                    row("pr:8802 exact-head review", PROOF_CURRENT),
                ],
                PARTIAL,
                campaign_records=["campaign-rust-repair"],
                derived_from=["fixtures/work_selection_identity/corpus.json"],
                legacy_goal="campaign-rust-repair",
                legacy_item="issue:9102",
            ), "pass"),
        ],
    },
]

# Negative cases: same valid base burn-down, exactly one wrong identity each.
campaign_override = burndown(
    "burndown:selection:start_build:issue:9105:campaign-override",
    base_9105, SNAPSHOT_MAIN,
    [
        row("issue:9105 acceptance", ACCEPTANCE_OPEN),
        row("slice-9105 implementation", MERGED),
    ],
    COMPLETE,
    campaign_records=["campaign-rust-repair"],
)
scenarios[3]["cases"].append(case(campaign_override, "fail", [
    ("subject_identity", "reconcile_selection"),
]))

wrong_main = burndown(
    "burndown:selection:start_build:issue:9105:wrong-main",
    base_9105, SNAPSHOT_MAIN,
    [
        row("issue:9105 acceptance", ACCEPTANCE_COVERED),
        row("slice-9105 exact-head review", PROOF_CURRENT),
    ],
    COMPLETE,
)
wrong_main["current_main_sha"] = "2" * 40
scenarios[3]["cases"].append(case(wrong_main, "fail", [
    ("basis_identity", "recompile_basis"),
]))

unknown_record = burndown(
    "burndown:selection:start_build:issue:9105:unknown-campaign-record",
    base_9105, SNAPSHOT_MAIN,
    [
        row("issue:9105 acceptance", ACCEPTANCE_COVERED),
        row("slice-9105 exact-head review", PROOF_CURRENT),
    ],
    COMPLETE,
    campaign_records=["campaign-unknown"],
)
scenarios[3]["cases"].append(case(unknown_record, "fail", [
    ("campaign_ref_identity", "reconcile_selection"),
]))

no_evidence = burndown(
    "burndown:selection:start_build:issue:9105:no-evidence",
    base_9105, SNAPSHOT_MAIN,
    [
        row("issue:9105 acceptance", ACCEPTANCE_COVERED),
    ],
    PARTIAL,
    derived_from=[],
)
scenarios[3]["cases"].append(case(no_evidence, "fail", [
    ("subject_identity", "reconcile_selection"),
]))

wrong_repo = burndown(
    "burndown:selection:start_build:issue:9105:wrong-repo",
    base_9105, SNAPSHOT_MAIN,
    [
        row("issue:9105 acceptance", ACCEPTANCE_COVERED),
        row("slice-9105 exact-head review", PROOF_CURRENT),
    ],
    COMPLETE,
)
wrong_repo["repository"] = "OtherOrg/other-repo"
scenarios[3]["cases"].append(case(wrong_repo, "fail", [
    ("repository_identity", "recompile_basis"),
]))

corpus = {
    "schema_version": "work_burndown_identity_corpus.v1",
    "repository": REPO,
    "selection_policy": "explicit-root-selection",
    "selection_policy_version": "v1",
    "scenarios": scenarios,
}

out = ROOT / "fixtures/work_burndown_identity/corpus.json"
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(corpus, indent=1) + "\n")
print("wrote", out)
