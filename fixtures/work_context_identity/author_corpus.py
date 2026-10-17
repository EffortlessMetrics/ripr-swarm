"""Author fixtures/work_context_identity/corpus.json for RIPR-SPEC-0272 (#1706 PR B).

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
SNAPSHOT_MULTI = "work-portfolio:sha256:d421627ab261553ad372f6615bd67bd6e3ed375d516bf8705fe6f713263cfb69"
REPO = "EffortlessMetrics/ripr-swarm"

SCOPE_ONLY = "selected_work_only"
SCOPE_OVERFLOW = "selected_work_plus_relevant_overflow"


def packet(scenario_id: str, case_index: int = 0) -> dict:
    scenario = next(s for s in SELECTION["scenarios"] if s["id"] == scenario_id)
    case = scenario["cases"][case_index]
    assert case["expect"] == "pass", scenario["id"]
    return copy.deepcopy(case["packet"])


def context_packet(packet_id, selection, snapshot, scope, context_refs,
                   overflow_refs=(), compiled_from=None, legacy_goal=None,
                   legacy_item=None) -> dict:
    return {
        "packet_id": packet_id,
        "repository": REPO,
        "portfolio_snapshot": snapshot,
        "basis_sha": HEAD,
        "selection": copy.deepcopy(selection),
        "scope": scope,
        "context_refs": list(context_refs),
        "overflow_refs": list(overflow_refs),
        "compiled_from": compiled_from if compiled_from is not None else [
            "fixtures/work_selection_identity/corpus.json"
        ],
        "legacy_active_goal_ref": legacy_goal,
        "legacy_current_work_item_ref": legacy_item,
    }


def case(body, expect, violations=()):
    return {
        "packet": body,
        "expect": expect,
        "expect_violations": [
            {"law": law, "route": route} for law, route in violations
        ],
    }


base_9105 = packet("scenario-01-selected-issue-one-campaign")
multi_9105 = packet("scenario-02-selected-issue-several-related-campaigns")
standalone_9106 = packet("scenario-03-standalone-no-campaign-placement")
repair_9101 = packet("scenario-05-existing-open-pr-selected-for-repair")
narrow_9202 = packet("scenario-12-narrow-single-agent-despite-active-campaigns")

scenarios = [
    {
        "id": "context-01-one-campaign-packet-passes",
        "title": "A bounded packet for a one-campaign selection compiles and passes",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(context_packet(
                "packet:context:selection:start_build:issue:9105",
                base_9105, SNAPSHOT_MAIN, SCOPE_ONLY,
                ["campaign-rust-repair", "campaigns.json", "RIPR-SPEC-0202",
                 "REQ-dup-family", "slice-9105", "issue:9106"],
            ), "pass"),
        ],
    },
    {
        "id": "context-02-several-related-campaigns-packet-passes",
        "title": "Several related campaign references stay context, never permission",
        "captured": "work_selection_identity/captured/multi-campaign",
        "cases": [
            case(context_packet(
                "packet:context:selection:start_build:issue:9105:multi-campaign",
                multi_9105, SNAPSHOT_MULTI, SCOPE_ONLY,
                ["campaign-editor-ux", "campaign-rust-repair", "campaigns.json",
                 "RIPR-SPEC-0202", "REQ-dup-family", "issue:9106"],
            ), "pass"),
        ],
    },
    {
        "id": "context-03-standalone-no-campaign-packet-passes",
        "title": "A standalone issue with no campaign placement compiles a valid packet",
        "captured": "work_selection_identity/captured/standalone",
        "cases": [
            case(context_packet(
                "packet:context:selection:start_build:issue:9106:standalone",
                standalone_9106, SNAPSHOT_STANDALONE, SCOPE_ONLY,
                ["RIPR-SPEC-0202", "REQ-dup-family", "slice-9106", "issue:9105"],
            ), "pass"),
        ],
    },
    {
        "id": "context-04-pr-repair-packet-passes",
        "title": "A repair packet binds the recorded PR, claim and worktree as relevant overflow",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(context_packet(
                "packet:context:selection:repair_review:issue:9101",
                repair_9101, SNAPSHOT_MAIN, SCOPE_OVERFLOW,
                ["campaign-rust-repair", "campaigns.json", "RIPR-SPEC-0200",
                 "pr:8801"],
                overflow_refs=["claim-9101", "<root>/wt-9101"],
            ), "pass"),
        ],
    },
    {
        "id": "context-05-narrow-single-agent-packet-passes",
        "title": "A narrow one-file issue stays single-agent despite active campaigns",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(context_packet(
                "packet:context:selection:start_build:issue:9202:narrow",
                narrow_9202, SNAPSHOT_MAIN, SCOPE_ONLY,
                ["campaign-editor-ux", "campaigns.json", "RIPR-SPEC-0207",
                 "REQ-editor-badges", "slice-9202"],
            ), "pass"),
        ],
    },
    {
        "id": "context-06-legacy-and-wrong-context-refs-fail",
        "title": "Legacy authority, irrelevant refs, unbounded scope, evidence-free packets and wrong identities fail visibly",
        "captured": "work_portfolio/corpus",
        "cases": [],
    },
]

# Negative cases: same valid base packet, exactly one wrong identity each.
legacy_unknown = context_packet(
    "packet:context:selection:start_build:issue:9105:legacy-unknown",
    base_9105, SNAPSHOT_MAIN, SCOPE_ONLY,
    ["campaign-rust-repair", "RIPR-SPEC-0202"],
    legacy_goal="campaign-unknown",
)
scenarios[5]["cases"].append(case(legacy_unknown, "fail", [
    ("legacy_compatibility", "reconcile_selection"),
]))

irrelevant_ref = context_packet(
    "packet:context:selection:start_build:issue:9105:irrelevant-ref",
    base_9105, SNAPSHOT_MAIN, SCOPE_ONLY,
    ["campaign-rust-repair", "RIPR-SPEC-0202", "campaign-unrelated"],
)
scenarios[5]["cases"].append(case(irrelevant_ref, "fail", [
    ("campaign_ref_identity", "reconcile_selection"),
]))

unbounded = context_packet(
    "packet:context:selection:start_build:issue:9105:unbounded-overflow",
    base_9105, SNAPSHOT_MAIN, SCOPE_ONLY,
    ["campaign-rust-repair", "RIPR-SPEC-0202"],
    overflow_refs=["issue:9106"],
)
scenarios[5]["cases"].append(case(unbounded, "fail", [
    ("subject_identity", "reconcile_selection"),
]))

no_evidence = context_packet(
    "packet:context:selection:start_build:issue:9105:no-compiled-from",
    base_9105, SNAPSHOT_MAIN, SCOPE_ONLY,
    ["campaign-rust-repair", "RIPR-SPEC-0202"],
    compiled_from=[],
)
scenarios[5]["cases"].append(case(no_evidence, "fail", [
    ("subject_identity", "reconcile_selection"),
]))

wrong_repo = context_packet(
    "packet:context:selection:start_build:issue:9105:wrong-repo",
    base_9105, SNAPSHOT_MAIN, SCOPE_ONLY,
    ["campaign-rust-repair", "RIPR-SPEC-0202"],
)
wrong_repo["repository"] = "OtherOrg/other-repo"
scenarios[5]["cases"].append(case(wrong_repo, "fail", [
    ("repository_identity", "recompile_basis"),
]))

stale_basis = context_packet(
    "packet:context:selection:start_build:issue:9105:stale-basis",
    base_9105, SNAPSHOT_MAIN, SCOPE_ONLY,
    ["campaign-rust-repair", "RIPR-SPEC-0202"],
)
stale_basis["basis_sha"] = "1" * 40
scenarios[5]["cases"].append(case(stale_basis, "fail", [
    ("basis_identity", "recompile_basis"),
]))

corpus = {
    "schema_version": "work_context_identity_corpus.v1",
    "repository": REPO,
    "selection_policy": "explicit-root-selection",
    "selection_policy_version": "v1",
    "scenarios": scenarios,
}

out = ROOT / "fixtures/work_context_identity/corpus.json"
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(corpus, indent=1) + "\n")
print("wrote", out)
