"""Author fixtures/work_adapter_identity/corpus.json for RIPR-SPEC-0277 (#1706 PR B).

Reuses the committed RIPR-SPEC-0244 selection packets and captured
directories by reference; no captured bytes are duplicated.
"""

import copy
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
SELECTION = json.loads(
    (ROOT / "fixtures/work_selection_identity/corpus.json").read_text(encoding="utf-8")
)

HEAD = "7cb64d60c9dd0dda59586e6b72dcdf150296a26f"
SNAPSHOT_MAIN = "work-portfolio:sha256:f13413b564d05b401866a5194c6f8265fd03c890597a904648c5c332b3b92005"
REPO = "EffortlessMetrics/ripr-swarm"


def packet(scenario_id: str, case_index: int = 0) -> dict:
    scenario = next(s for s in SELECTION["scenarios"] if s["id"] == scenario_id)
    case = scenario["cases"][case_index]
    assert case["expect"] == "pass", scenario["id"]
    return copy.deepcopy(case["packet"])


def adapter(adapter_id, selection, role, stage, result_schema="SubtaskResult",
            subagent_reads_portfolio=False, derived_from=None, legacy_goal=None,
            legacy_item=None, repository=REPO) -> dict:
    return {
        "adapter_id": adapter_id,
        "repository": repository,
        "portfolio_snapshot": SNAPSHOT_MAIN,
        "basis_sha": HEAD,
        "selection": copy.deepcopy(selection),
        "role": role,
        "flow_stage": stage,
        "subagent_reads_portfolio": subagent_reads_portfolio,
        "result_schema": result_schema,
        "derived_from": derived_from if derived_from is not None else [
            "fixtures/work_selection_identity/corpus.json"
        ],
        "legacy_active_goal_ref": legacy_goal,
        "legacy_current_work_item_ref": legacy_item,
    }


def case(body, expect="pass", violations=()):
    return {
        "adapter": body,
        "expect": expect,
        "expect_violations": [
            {"law": law, "route": route} for law, route in violations
        ],
    }


in_campaign_9105 = packet("scenario-01-selected-issue-one-campaign")
two_campaigns_1693 = packet("scenario-04-two-campaigns-independent-disjoint-resources")
blocked_9109 = packet("scenario-10-burndown-partial-acceptance-uncovered")
resume_9102 = packet("scenario-11-fresh-root-resume-from-artifacts")

scenarios = [
    {
        "id": "adapter-01-scout-read-only-delegation-passes",
        "title": "A read-only scout delegation over one selected issue passes",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(adapter(
                "adapter:scout:issue:9105",
                in_campaign_9105,
                "scout",
                "delegate",
                derived_from=[
                    "fixtures/work_selection_identity/corpus.json#scenario-01-selected-issue-one-campaign",
                ],
            )),
        ],
    },
    {
        "id": "adapter-02-builder-with-prepared-claim-passes",
        "title": "A builder delegation over a selection with a prepared claim and isolated worktree passes",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(adapter(
                "adapter:builder:issue:9102",
                resume_9102,
                "builder",
                "delegate",
                derived_from=[
                    "fixtures/work_selection_identity/corpus.json#scenario-11-fresh-root-resume-from-artifacts",
                ],
            )),
        ],
    },
    {
        "id": "adapter-03-planner-compile-delegation-passes",
        "title": "A work-planner delegation over an independent multi-campaign selection passes",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(adapter(
                "adapter:work_planner:issue:1693",
                two_campaigns_1693,
                "work_planner",
                "compile",
                result_schema="WorkPlannerResult",
                derived_from=[
                    "fixtures/work_selection_identity/corpus.json#scenario-04-two-campaigns-independent-disjoint-resources",
                ],
            )),
        ],
    },
    {
        "id": "adapter-04-resume-from-artifacts-review-passes",
        "title": "A reviewer delegation resumed from committed artifacts only passes",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(adapter(
                "adapter:reviewer:issue:9102",
                resume_9102,
                "reviewer",
                "verify_review",
                result_schema="ReviewResult",
                derived_from=[
                    "target/ripr/reports/work-result-check.json",
                    "fixtures/work_selection_identity/corpus.json#scenario-11-fresh-root-resume-from-artifacts",
                ],
            )),
        ],
    },
    {
        "id": "adapter-05-portfolio-reread-and-wrong-inputs-fail",
        "title": "A subagent portfolio re-read, an unprepared builder, a subagent claim-write, an empty derivation, a foreign repository and an unknown legacy goal each fail with the exact law/route pair",
        "captured": "work_portfolio/corpus",
        "cases": [
            # 0: a subagent never re-reads the portfolio; compilation stays
            # in the root thread.
            case(adapter(
                "adapter:scout-rereads-portfolio:issue:9105",
                in_campaign_9105,
                "scout",
                "delegate",
                subagent_reads_portfolio=True,
            ), "fail", [("subject_identity", "reconcile_selection")]),
            # 1: a write-capable builder requires a prepared claim or
            # isolated worktree on the bound selection.
            case(adapter(
                "adapter:builder-without-claim:issue:9105",
                in_campaign_9105,
                "builder",
                "delegate",
            ), "fail", [("subject_identity", "reconcile_selection")]),
            # 2: subagents never claim; claims stay in the root thread.
            case(adapter(
                "adapter:scout-claims-write:issue:9105",
                in_campaign_9105,
                "scout",
                "claim_write",
            ), "fail", [("subject_identity", "reconcile_selection")]),
            # 3: a delegation derived from nothing is a claim.
            case(adapter(
                "adapter:evidence-free:issue:9105",
                in_campaign_9105,
                "scout",
                "delegate",
                derived_from=[],
            ), "fail", [("subject_identity", "reconcile_selection")]),
            # 4: a delegation for another repository is wrong input.
            case(adapter(
                "adapter:foreign-repository:issue:9105",
                in_campaign_9105,
                "scout",
                "delegate",
                repository="someone/else",
            ), "fail", [("repository_identity", "recompile_basis")]),
            # 5: a legacy goal ref must resolve to a recorded historical
            # campaign.
            case(adapter(
                "adapter:legacy-unknown-goal:issue:9109",
                blocked_9109,
                "burn_down_reviewer",
                "verify_review",
                result_schema="BurnDownReviewResult",
                legacy_goal="campaign-unknown-goal",
            ), "fail", [("legacy_compatibility", "reconcile_selection")]),
        ],
    },
]

corpus = {
    "schema_version": "work_adapter_identity_corpus.v1",
    "repository": REPO,
    "selection_policy": "explicit-root-selection",
    "selection_policy_version": "v1",
    "scenarios": scenarios,
}

OUT = Path(__file__).resolve().parent
(OUT / "corpus.json").write_text(
    json.dumps(corpus, indent=1) + "\n", encoding="utf-8"
)

digest = hashlib.sha256((OUT / "corpus.json").read_bytes()).hexdigest()
provenance = {
    "schema_version": "work_adapter_identity_provenance.v1",
    "repository": REPO,
    "captured_at": "2026-10-09T12:00:00Z",
    "capture_method": "authored Codex adapter delegation corpus reusing the committed RIPR-SPEC-0244 selection packets and captured directories by reference",
    "files": [{"path": "corpus.json", "sha256": digest}],
}
(OUT / "provenance.json").write_text(
    json.dumps(provenance, indent=1) + "\n", encoding="utf-8"
)
print("wrote corpus.json and provenance.json")
