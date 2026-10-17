"""Author fixtures/work_wave_identity/corpus.json for RIPR-SPEC-0271 (#1706 PR B).

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
REPO = "EffortlessMetrics/ripr-swarm"


def packet(scenario_id: str, case_index: int = 0) -> dict:
    scenario = next(s for s in SELECTION["scenarios"] if s["id"] == scenario_id)
    case = scenario["cases"][case_index]
    assert case["expect"] == "pass", scenario["id"]
    return copy.deepcopy(case["packet"])


SEL_9105 = packet("scenario-01-selected-issue-one-campaign")
SEL_1693 = packet("scenario-04-two-campaigns-independent-disjoint-resources", 0)
SEL_9202 = packet("scenario-12-narrow-single-agent-despite-active-campaigns")


def wave(wave_id, selection_ids, resources) -> dict:
    return {"wave_id": wave_id, "selections": selection_ids, "resources": resources}


def plan(plan_id, selections, capacity, waves, basis_sha=HEAD,
         legacy_goal=None) -> dict:
    return {
        "plan_id": plan_id,
        "repository": REPO,
        "portfolio_snapshot": SNAPSHOT_MAIN,
        "basis_sha": basis_sha,
        "selections": copy.deepcopy(selections),
        "capacity": {"concurrent_writers": capacity},
        "waves": waves,
        "legacy_active_goal_ref": legacy_goal,
    }


def case(body, expect, violations=()):
    return {
        "plan": body,
        "expect": expect,
        "expect_violations": [
            {"law": law, "route": route} for law, route in violations
        ],
    }


ID_9105 = SEL_9105["selection_id"]
ID_1693 = SEL_1693["selection_id"]
ID_9202 = SEL_9202["selection_id"]

WAVES_DISJOINT = [
    wave("wave-editor", [ID_1693], [
        "campaign-record:campaign-editor-ux",
        "portfolio-schema",
        "spec-ledger",
        "output-contract:preview-panel",
        "issue:1693",
        "worktree:<root>/wt-9107-a",
    ]),
    wave("wave-rust-repair", [ID_9105], [
        "campaign-record:campaign-rust-repair",
        "portfolio-schema",
        "spec-ledger",
        "issue:9105",
        "worktree:<root>/wt-9108",
    ]),
]

scenarios = [
    {
        "id": "wave-01-disjoint-cross-campaign-waves-pass",
        "title": "Two campaigns with disjoint exclusive resources run as independent waves",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(plan("plan-disjoint", [SEL_1693, SEL_9105], 2, WAVES_DISJOINT), "pass"),
        ],
    },
    {
        "id": "wave-02-single-agent-wave-passes-despite-active-campaigns",
        "title": "The narrow single-agent issue gets its own wave while other campaigns stay active",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(plan("plan-single-agent", [SEL_9202, SEL_9105], 2, [
                wave("wave-narrow", [ID_9202], [
                    "campaign-record:campaign-editor-ux",
                    "portfolio-schema",
                    "issue:9202",
                    "worktree:<root>/wt-9107-a",
                ]),
                wave("wave-other-campaign", [ID_9105], [
                    "campaign-record:campaign-rust-repair",
                    "spec-ledger",
                    "issue:9105",
                    "worktree:<root>/wt-9108",
                ]),
            ]), "pass"),
        ],
    },
    {
        "id": "wave-03-overlapping-exclusive-resources-fail",
        "title": "An exclusive output contract shared by two waves forces reconcile",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(plan("plan-overlap", [SEL_1693, SEL_9105], 2, [
                wave("wave-editor", [ID_1693], [
                    "campaign-record:campaign-editor-ux",
                    "portfolio-schema",
                    "output-contract:preview-panel",
                    "issue:1693",
                    "worktree:<root>/wt-9107-a",
                ]),
                wave("wave-rust-repair", [ID_9105], [
                    "campaign-record:campaign-rust-repair",
                    "output-contract:preview-panel",
                    "issue:9105",
                    "worktree:<root>/wt-9108",
                ]),
            ]), "fail", [("overlap_visibility", "reconcile_resources")]),
        ],
    },
    {
        "id": "wave-04-retired-active-goal-resource-fails-closed",
        "title": "The retired generic active-goal resource and unknown shapes fail closed",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(plan("plan-retired-resource", [SEL_9105], 1, [
                wave("wave-singleton", [ID_9105], [
                    "active-goal",
                    "mystery-queue",
                ]),
            ]), "fail", [
                ("campaign_ref_identity", "reconcile_resources"),
                ("campaign_ref_identity", "reconcile_resources"),
            ]),
        ],
    },
    {
        "id": "wave-05-stale-basis-and-unassigned-candidate-fail",
        "title": "A stale basis routes recompilation and an unassigned selected candidate fails visibly",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(plan("plan-stale-basis", [SEL_1693, SEL_9105], 2, WAVES_DISJOINT,
                      basis_sha="1" * 40), "fail", [
                ("basis_identity", "recompile_basis"),
            ]),
            case(plan("plan-unassigned", [SEL_1693, SEL_9105], 2, [
                wave("wave-rust-repair-only", [ID_9105], [
                    "campaign-record:campaign-rust-repair",
                    "portfolio-schema",
                    "issue:9105",
                    "worktree:<root>/wt-9108",
                ]),
            ]), "fail", [
                ("subject_identity", "reconcile_selection"),
            ]),
        ],
    },
    {
        "id": "wave-06-capacity-exceeded-fails",
        "title": "A wave above the declared writer capacity forces reconcile",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(plan("plan-over-capacity", [SEL_1693, SEL_9105], 1, [
                wave("wave-too-wide", [ID_1693, ID_9105], [
                    "campaign-record:campaign-editor-ux",
                    "campaign-record:campaign-rust-repair",
                    "portfolio-schema",
                    "issue:1693",
                    "issue:9105",
                    "worktree:<root>/wt-9107-a",
                    "worktree:<root>/wt-9108",
                ]),
            ]), "fail", [
                ("worktree_identity", "reconcile_resources"),
            ]),
        ],
    },
]

corpus = {
    "schema_version": "work_wave_identity_corpus.v1",
    "repository": REPO,
    "selection_policy": "explicit-root-selection",
    "selection_policy_version": "v1",
    "scenarios": scenarios,
}

out = ROOT / "fixtures/work_wave_identity/corpus.json"
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(corpus, indent=1) + "\n")
print("wrote", out)
