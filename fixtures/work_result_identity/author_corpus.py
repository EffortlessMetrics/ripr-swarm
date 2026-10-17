"""Author fixtures/work_result_identity/corpus.json for RIPR-SPEC-0269 (#1706 PR B).

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


def packet(scenario_id: str, case_index: int = 0) -> dict:
    scenario = next(s for s in SELECTION["scenarios"] if s["id"] == scenario_id)
    case = scenario["cases"][case_index]
    assert case["expect"] == "pass", scenario["id"]
    return copy.deepcopy(case["packet"])


def result(result_id, kind, selection, snapshot, evidence, worktree=None,
           legacy_goal=None, legacy_item=None) -> dict:
    return {
        "result_id": result_id,
        "repository": REPO,
        "portfolio_snapshot": snapshot,
        "basis_sha": HEAD,
        "selection": copy.deepcopy(selection),
        "result_kind": kind,
        "worktree": worktree,
        "reported_head": HEAD,
        "evidence_refs": evidence,
        "legacy_active_goal_ref": legacy_goal,
        "legacy_current_work_item_ref": legacy_item,
    }


def case(body, expect, violations=()):
    return {
        "result": body,
        "expect": expect,
        "expect_violations": [
            {"law": law, "route": route} for law, route in violations
        ],
    }


base_9105 = packet("scenario-01-selected-issue-one-campaign")
standalone_9106 = packet("scenario-03-standalone-no-campaign-placement")
repair_9101 = packet("scenario-05-existing-open-pr-selected-for-repair")
resume_9102 = packet("scenario-11-fresh-root-resume-from-artifacts")

scenarios = [
    {
        "id": "result-01-one-campaign-selection-results-pass",
        "title": "Results for a selection in one campaign bind and pass",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(result(
                "result:context-compiled:selection:start_build:issue:9105",
                "context_compiled", base_9105, SNAPSHOT_MAIN,
                ["fixtures/work_selection_identity/corpus.json"],
            ), "pass"),
            case(result(
                "result:build-completed:selection:start_build:issue:9105",
                "build_completed", base_9105, SNAPSHOT_MAIN,
                ["target/ripr/reports/work-selection-check.json"],
            ), "pass"),
            case(result(
                "result:review-recorded:selection:start_build:issue:9105",
                "review_recorded", base_9105, SNAPSHOT_MAIN,
                ["target/ripr/reports/work-result-check.md"],
            ), "pass"),
        ],
    },
    {
        "id": "result-02-standalone-selection-results-pass",
        "title": "A standalone selection with zero campaigns still carries results",
        "captured": "work_selection_identity/captured/standalone",
        "cases": [
            case(result(
                "result:task-dispatched:selection:start_build:issue:9106",
                "task_dispatched", standalone_9106, SNAPSHOT_STANDALONE,
                ["fixtures/work_selection_identity/captured/standalone/issues.json"],
            ), "pass"),
        ],
    },
    {
        "id": "result-03-repair-selection-worktree-result-passes",
        "title": "A repair result binds the worktree the selection recorded",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(result(
                "result:verification-recorded:selection:repair_review:issue:9101",
                "verification_recorded", repair_9101, SNAPSHOT_MAIN,
                ["fixtures/work_portfolio/corpus/pull_requests.json"],
                worktree="<root>/wt-9101",
            ), "pass"),
        ],
    },
    {
        "id": "result-04-legacy-read-only-migration-posture",
        "title": "Legacy refs on a result resolve read-only with migration posture",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(result(
                "result:review-recorded:selection:resume_pr:issue:9102",
                "review_recorded", resume_9102, SNAPSHOT_MAIN,
                ["fixtures/work_selection_identity/corpus.json"],
                worktree="<root>/wt-9102",
                legacy_goal="campaign-rust-repair",
                legacy_item="issue:9102",
            ), "pass"),
        ],
    },
    {
        "id": "result-05-wrong-identities-fail-stale-or-wrong-input",
        "title": "Wrong repository, snapshot, basis, head, worktree, action, evidence or legacy ref fails visibly",
        "captured": "work_portfolio/corpus",
        "cases": [],
    },
]

# Negative cases: same valid base result, exactly one wrong identity each.
wrong_repo = result(
    "result:build-completed:selection:start_build:issue:9105",
    "build_completed", base_9105, SNAPSHOT_MAIN,
    ["target/ripr/reports/work-selection-check.json"],
)
wrong_repo["repository"] = "OtherOrg/other-repo"
scenarios[4]["cases"].append(case(wrong_repo, "fail", [
    ("repository_identity", "recompile_basis"),
]))

wrong_snapshot = result(
    "result:build-completed:selection:start_build:issue:9105",
    "build_completed", base_9105, SNAPSHOT_MAIN,
    ["target/ripr/reports/work-selection-check.json"],
)
wrong_snapshot["portfolio_snapshot"] = "work-portfolio:sha256:" + "f" * 64
scenarios[4]["cases"].append(case(wrong_snapshot, "fail", [
    ("basis_identity", "recompile_basis"),
]))

wrong_basis = result(
    "result:build-completed:selection:start_build:issue:9105",
    "build_completed", base_9105, SNAPSHOT_MAIN,
    ["target/ripr/reports/work-selection-check.json"],
)
wrong_basis["basis_sha"] = "1" * 40
scenarios[4]["cases"].append(case(wrong_basis, "fail", [
    ("basis_identity", "recompile_basis"),
]))

wrong_head = result(
    "result:build-completed:selection:start_build:issue:9105",
    "build_completed", base_9105, SNAPSHOT_MAIN,
    ["target/ripr/reports/work-selection-check.json"],
)
wrong_head["reported_head"] = "9" * 40
scenarios[4]["cases"].append(case(wrong_head, "fail", [
    ("head_identity", "reconcile_head"),
]))

unknown_worktree = result(
    "result:build-completed:selection:start_build:issue:9105",
    "build_completed", base_9105, SNAPSHOT_MAIN,
    ["target/ripr/reports/work-selection-check.json"],
    worktree="<root>/wt-9999",
)
scenarios[4]["cases"].append(case(unknown_worktree, "fail", [
    ("worktree_identity", "reconcile_resources"),
]))

wrong_action = result(
    "result:build-completed:selection:repair_review:issue:9105",
    "build_completed", base_9105, SNAPSHOT_MAIN,
    ["target/ripr/reports/work-selection-check.json"],
)
wrong_action["selection"]["lifecycle_action"] = "repair_review"
wrong_action["selection"]["selection_id"] = "selection:repair_review:issue:9105"
scenarios[4]["cases"].append(case(wrong_action, "fail", [
    ("action_identity", "reconcile_selection"),
]))

no_evidence = result(
    "result:build-completed:selection:start_build:issue:9105",
    "build_completed", base_9105, SNAPSHOT_MAIN, [],
)
scenarios[4]["cases"].append(case(no_evidence, "fail", [
    ("subject_identity", "reconcile_selection"),
]))

unknown_legacy = result(
    "result:review-recorded:selection:start_build:issue:9105",
    "review_recorded", base_9105, SNAPSHOT_MAIN,
    ["target/ripr/reports/work-selection-check.json"],
    legacy_goal="campaign-unknown",
)
scenarios[4]["cases"].append(case(unknown_legacy, "fail", [
    ("legacy_compatibility", "reconcile_selection"),
]))

corpus = {
    "schema_version": "work_result_identity_corpus.v1",
    "repository": REPO,
    "selection_policy": "explicit-root-selection",
    "selection_policy_version": "v1",
    "scenarios": scenarios,
}

out = ROOT / "fixtures/work_result_identity/corpus.json"
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(corpus, indent=1) + "\n")
print("wrote", out)
