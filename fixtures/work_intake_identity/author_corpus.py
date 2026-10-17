"""Author fixtures/work_intake_identity/corpus.json for RIPR-SPEC-0275 (#1706 PR B).

References the committed RIPR-SPEC-0244 captured directories; no captured
bytes are duplicated.
"""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent

HEAD = "7cb64d60c9dd0dda59586e6b72dcdf150296a26f"
SNAPSHOT_MAIN = "work-portfolio:sha256:f13413b564d05b401866a5194c6f8265fd03c890597a904648c5c332b3b92005"
SNAPSHOT_STANDALONE = "work-portfolio:sha256:06c8d9fa8eb6feee4e6d2815b56dd8567d39460be40134e158d691a7c8bc2715"
REPO = "EffortlessMetrics/ripr-swarm"

QUALIFIED = "qualified"
NEEDS_TRIAGE = "needs_triage"


def intake(intake_id, issue_number, snapshot, qualification, placement=None,
           relevant=(), overlaps=(), contracts=("RIPR-SPEC-0202",),
           single_agent=False, derived_from=None, legacy_goal=None,
           legacy_item=None) -> dict:
    return {
        "intake_id": intake_id,
        "repository": REPO,
        "portfolio_snapshot": snapshot,
        "basis_sha": HEAD,
        "current_main_sha": HEAD,
        "issue_number": issue_number,
        "qualification": qualification,
        "campaign_placement": placement,
        "relevant_campaigns": list(relevant),
        "open_overlaps": list(overlaps),
        "accepted_contracts": list(contracts),
        "single_agent": single_agent,
        "derived_from": derived_from if derived_from is not None else [
            "fixtures/work_portfolio/corpus/issues.json"
        ],
        "legacy_active_goal_ref": legacy_goal,
        "legacy_current_work_item_ref": legacy_item,
    }


def case(body, expect, violations=()):
    return {
        "intake": body,
        "expect": expect,
        "expect_violations": [
            {"law": law, "route": route} for law, route in violations
        ],
    }


scenarios = [
    {
        "id": "intake-01-in-campaign-qualified-passes",
        "title": "An issue recorded in one campaign qualifies with placement and contracts",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(intake(
                "intake:issue:9105",
                9105, SNAPSHOT_MAIN, QUALIFIED,
                placement="campaign-rust-repair",
                relevant=["campaign-rust-repair"],
                overlaps=["issue:9106"],
                contracts=["RIPR-SPEC-0202", "REQ-dup-family"],
            ), "pass"),
        ],
    },
    {
        "id": "intake-02-out-of-campaign-still-qualifies",
        "title": "An issue outside every campaign may still qualify; placement stays a later decision",
        "captured": "work_selection_identity/captured/standalone",
        "cases": [
            case(intake(
                "intake:issue:9106:standalone",
                9106, SNAPSHOT_STANDALONE, QUALIFIED,
                contracts=["RIPR-SPEC-0202", "REQ-dup-family"],
                derived_from=[
                    "fixtures/work_selection_identity/captured/standalone/issues.json"
                ],
            ), "pass"),
        ],
    },
    {
        "id": "intake-03-pr-repair-intake-overlap-visible",
        "title": "An intake for review/repair of an existing open PR keeps the overlap visible",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(intake(
                "intake:issue:9101:repair",
                9101, SNAPSHOT_MAIN, QUALIFIED,
                placement="campaign-rust-repair",
                relevant=["campaign-rust-repair"],
                overlaps=["pr:8801", "claim-9101"],
                contracts=["RIPR-SPEC-0200"],
            ), "pass"),
        ],
    },
    {
        "id": "intake-04-narrow-single-agent-intake-passes",
        "title": "A narrow one-file issue stays single-agent despite other active campaigns",
        "captured": "work_portfolio/corpus",
        "cases": [
            case(intake(
                "intake:issue:9202:narrow",
                9202, SNAPSHOT_MAIN, QUALIFIED,
                placement="campaign-editor-ux",
                relevant=["campaign-editor-ux"],
                contracts=["RIPR-SPEC-0207", "REQ-editor-badges"],
                single_agent=True,
            ), "pass"),
        ],
    },
    {
        "id": "intake-05-legacy-and-wrong-inputs-fail",
        "title": "A legacy authority attempt, an unknown placement, an unknown campaign, an empty contract set, an unknown issue and a wrong repository fail visibly",
        "captured": "work_portfolio/corpus",
        "cases": [],
    },
]

# Negative cases: same valid base intake, exactly one wrong identity each.
legacy_unknown = intake(
    "intake:issue:9105:legacy-unknown",
    9105, SNAPSHOT_MAIN, QUALIFIED,
    placement="campaign-rust-repair",
    relevant=["campaign-rust-repair"],
    legacy_goal="campaign-unknown",
)
scenarios[4]["cases"].append(case(legacy_unknown, "fail", [
    ("legacy_compatibility", "reconcile_selection"),
]))

unknown_placement = intake(
    "intake:issue:9105:unknown-placement",
    9105, SNAPSHOT_MAIN, QUALIFIED,
    placement="campaign-unknown",
    relevant=["campaign-rust-repair"],
)
scenarios[4]["cases"].append(case(unknown_placement, "fail", [
    ("campaign_ref_identity", "reconcile_selection"),
]))

unknown_relevant = intake(
    "intake:issue:9105:unknown-relevant",
    9105, SNAPSHOT_MAIN, QUALIFIED,
    placement="campaign-rust-repair",
    relevant=["campaign-unknown"],
)
scenarios[4]["cases"].append(case(unknown_relevant, "fail", [
    ("campaign_ref_identity", "reconcile_selection"),
]))

no_contracts = intake(
    "intake:issue:9105:no-contracts",
    9105, SNAPSHOT_MAIN, NEEDS_TRIAGE,
    placement="campaign-rust-repair",
    relevant=["campaign-rust-repair"],
    contracts=[],
)
scenarios[4]["cases"].append(case(no_contracts, "fail", [
    ("subject_identity", "reconcile_selection"),
]))

unknown_issue = intake(
    "intake:issue:9999:unknown",
    9999, SNAPSHOT_MAIN, NEEDS_TRIAGE,
    contracts=["RIPR-SPEC-0202"],
)
scenarios[4]["cases"].append(case(unknown_issue, "fail", [
    ("subject_identity", "reconcile_selection"),
]))

wrong_repo = intake(
    "intake:issue:9105:wrong-repo",
    9105, SNAPSHOT_MAIN, QUALIFIED,
    placement="campaign-rust-repair",
    relevant=["campaign-rust-repair"],
)
wrong_repo["repository"] = "OtherOrg/other-repo"
scenarios[4]["cases"].append(case(wrong_repo, "fail", [
    ("repository_identity", "recompile_basis"),
]))

wrong_main = intake(
    "intake:issue:9105:wrong-main",
    9105, SNAPSHOT_MAIN, QUALIFIED,
    placement="campaign-rust-repair",
    relevant=["campaign-rust-repair"],
)
wrong_main["current_main_sha"] = "3" * 40
scenarios[4]["cases"].append(case(wrong_main, "fail", [
    ("basis_identity", "recompile_basis"),
]))

corpus = {
    "schema_version": "work_intake_identity_corpus.v1",
    "repository": REPO,
    "selection_policy": "explicit-root-selection",
    "selection_policy_version": "v1",
    "scenarios": scenarios,
}

out = ROOT / "fixtures/work_intake_identity/corpus.json"
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(corpus, indent=1) + "\n")
print("wrote", out)
