"""Author fixtures/work_synthesis_identity/corpus.json for RIPR-SPEC-0273 (#1706 PR B).

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

COVERED = "covered"
UNCOVERED = "uncovered"
CONTRADICTED = "contradicted"


def packet(scenario_id: str, case_index: int = 0) -> dict:
    scenario = next(s for s in SELECTION["scenarios"] if s["id"] == scenario_id)
    case = scenario["cases"][case_index]
    assert case["expect"] == "pass", scenario["id"]
    return copy.deepcopy(case["packet"])


def synthesis(synthesis_id, selection, snapshot, transition, coverage,
              contradictions, next_action, derived_from=None,
              campaign_ref=None, legacy_goal=None, legacy_item=None) -> dict:
    return {
        "synthesis_id": synthesis_id,
        "repository": REPO,
        "portfolio_snapshot": snapshot,
        "basis_sha": HEAD,
        "selection": copy.deepcopy(selection),
        "reported_transition": transition,
        "acceptance_coverage": coverage,
        "contradictions": contradictions,
        "next_action": next_action,
        "derived_from": derived_from if derived_from is not None else [
            "fixtures/work_selection_identity/corpus.json"
        ],
        "reported_campaign_ref": campaign_ref,
        "legacy_active_goal_ref": legacy_goal,
        "legacy_current_work_item_ref": legacy_item,
    }


def row(requirement, status):
    return {"requirement": requirement, "status": status}


def case(body, expect, violations=()):
    return {
        "synthesis": body,
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
        "id": "synthesis-01-covered-transition-synthesis-passes",
        "title": "A synthesis for a covered selected transition passes",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(synthesis(
                "synthesis:selection:start_build:issue:9105",
                base_9105, SNAPSHOT_MAIN, "start_build",
                [row("REQ-dup-family", COVERED), row("RIPR-SPEC-0202", COVERED)],
                [],
                "run the focused proof lane for slice-9105, then publish the candidate",
                campaign_ref="campaign-rust-repair",
            ), "pass"),
        ],
    },
    {
        "id": "synthesis-02-standalone-synthesis-passes",
        "title": "A standalone selection with zero campaigns supports an honest synthesis",
        "captured": "work_selection_identity/captured/standalone",
        "cases": [
            case(synthesis(
                "synthesis:selection:start_build:issue:9106:standalone",
                standalone_9106, SNAPSHOT_STANDALONE, "start_build",
                [row("REQ-dup-family", COVERED), row("RIPR-SPEC-0202", COVERED)],
                [],
                "compile the standalone packet and dispatch the single-agent writer",
            ), "pass"),
        ],
    },
    {
        "id": "synthesis-03-partial-coverage-contradiction-surfaced",
        "title": "Partial coverage after one campaign work item merges surfaces a named contradiction",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(synthesis(
                "synthesis:selection:verify_current_head:issue:9109:partial",
                partial_9109, SNAPSHOT_MAIN, "verify_current_head",
                [row("RIPR-SPEC-0205", UNCOVERED)],
                ["RIPR-SPEC-0205 remains uncovered: one campaign work item merged but issue acceptance is still open"],
                "reconcile the exact-head review before reporting the transition covered",
                derived_from=[
                    "fixtures/work_portfolio/corpus/pull_requests.json",
                    "fixtures/work_portfolio/corpus/issues.json",
                ],
                campaign_ref="campaign-rust-repair",
            ), "pass"),
        ],
    },
    {
        "id": "synthesis-04-hidden-gap-silent-switch-and-wrong-inputs-fail",
        "title": "A hidden gap, a silent campaign switch, a mismatched transition, an evidence-free synthesis and a wrong repository fail visibly",
        "captured": "work_portfolio/corpus",
        "cases": [],
    },
    {
        "id": "synthesis-05-legacy-read-only-migration-posture",
        "title": "Legacy refs on a synthesis resolve read-only with migration posture",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(synthesis(
                "synthesis:selection:resume_pr:issue:9102",
                resume_9102, SNAPSHOT_MAIN, "resume_pr",
                [row("RIPR-SPEC-0200", COVERED)],
                [],
                "reconcile the resumed PR against the current exact head",
                derived_from=["fixtures/work_selection_identity/corpus.json"],
                legacy_goal="campaign-rust-repair",
                legacy_item="issue:9102",
            ), "pass"),
        ],
    },
]

# Negative cases: same valid base synthesis, exactly one wrong identity each.
hidden_gap = synthesis(
    "synthesis:selection:start_build:issue:9105:hidden-gap",
    base_9105, SNAPSHOT_MAIN, "start_build",
    [row("REQ-dup-family", UNCOVERED), row("RIPR-SPEC-0202", COVERED)],
    [],
    "report the transition covered",
    campaign_ref="campaign-rust-repair",
)
scenarios[3]["cases"].append(case(hidden_gap, "fail", [
    ("subject_identity", "reconcile_selection"),
]))

silent_switch = synthesis(
    "synthesis:selection:start_build:issue:9105:silent-switch",
    base_9105, SNAPSHOT_MAIN, "start_build",
    [row("REQ-dup-family", COVERED), row("RIPR-SPEC-0202", COVERED)],
    [],
    "continue the bounded slice",
    campaign_ref="campaign-editor-ux",
)
scenarios[3]["cases"].append(case(silent_switch, "fail", [
    ("campaign_ref_identity", "reconcile_selection"),
]))

wrong_transition = synthesis(
    "synthesis:selection:start_build:issue:9105:wrong-transition",
    base_9105, SNAPSHOT_MAIN, "repair_review",
    [row("REQ-dup-family", COVERED), row("RIPR-SPEC-0202", COVERED)],
    [],
    "continue the bounded slice",
)
# The reported transition disagrees with the bound selection's action, so
# the synthesis-level ActionIdentity law fires exactly once; the embedded
# selection itself stays a valid RIPR-SPEC-0244 packet.
scenarios[3]["cases"].append(case(wrong_transition, "fail", [
    ("action_identity", "reconcile_selection"),
]))

no_evidence = synthesis(
    "synthesis:selection:start_build:issue:9105:no-derived-from",
    base_9105, SNAPSHOT_MAIN, "start_build",
    [row("REQ-dup-family", COVERED), row("RIPR-SPEC-0202", COVERED)],
    [],
    "continue the bounded slice",
    derived_from=[],
)
scenarios[3]["cases"].append(case(no_evidence, "fail", [
    ("subject_identity", "reconcile_selection"),
]))

wrong_repo = synthesis(
    "synthesis:selection:start_build:issue:9105:wrong-repo",
    base_9105, SNAPSHOT_MAIN, "start_build",
    [row("REQ-dup-family", COVERED), row("RIPR-SPEC-0202", COVERED)],
    [],
    "continue the bounded slice",
)
wrong_repo["repository"] = "OtherOrg/other-repo"
scenarios[3]["cases"].append(case(wrong_repo, "fail", [
    ("repository_identity", "recompile_basis"),
]))

corpus = {
    "schema_version": "work_synthesis_identity_corpus.v1",
    "repository": REPO,
    "selection_policy": "explicit-root-selection",
    "selection_policy_version": "v1",
    "scenarios": scenarios,
}

out = ROOT / "fixtures/work_synthesis_identity/corpus.json"
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(corpus, indent=1) + "\n")
print("wrote", out)
