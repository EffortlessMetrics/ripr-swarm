//! Discriminating proof for `check-release-selection` (#2766).
//!
//! Every rule owns at least one fixture that violates exactly that rule, and
//! each fixture asserts the checker reports that rule and no other.
//! `every_rule_owns_a_negative_fixture` fails if a rule is added without a
//! fixture that can trip it, so a rule that cannot fail cannot ship.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::{
    RELEASE_SELECTION_PATH, RULE_DEPENDENCY, RULE_EVIDENCE, RULE_IDS, RULE_ISSUE_PARTITION,
    RULE_SCHEMA, RULE_STAGE_LAW, RULE_UNIQUENESS, RULE_VOCABULARY, evaluate_release_selection,
};

const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SENTENCE: &str = "The release makes no claim about this beyond the stated boundary.";

fn landed(id: &str, issue: u64) -> Value {
    json!({
        "claim_id": id,
        "owner_issue": issue,
        "materiality": "release_required",
        "resolution": "landed_and_accepted",
        "implementation_refs": ["pr:100"],
        "proof_refs": ["https://github.com/EffortlessMetrics/ripr-swarm/issues/1"],
        "candidate_effect": "Present in the cut."
    })
}

fn deferred(id: &str, issue: u64) -> Value {
    json!({
        "claim_id": id,
        "owner_issue": issue,
        "materiality": "release_optional",
        "resolution": "accepted_defer_with_non_claim",
        "candidate_effect": "Not in the cut.",
        "non_claim": SENTENCE
    })
}

fn pending(id: &str, issue: u64) -> Value {
    json!({
        "claim_id": id,
        "owner_issue": issue,
        "materiality": "release_required",
        "resolution": "pending",
        "candidate_effect": "Undecided.",
        "next": "Owner thread decides defer or land before the cut."
    })
}

/// A packet that satisfies every rule at `scope_pending`.
fn valid() -> Value {
    json!({
        "schema_version": "1",
        "kind": "ripr_release_selection_packet",
        "release": "0.11.0",
        "authority_issue": 2766,
        "stage": "scope_pending",
        "observed_main": { "sha": null },
        "claims": [landed("a", 10), deferred("b", 11), pending("c", 12)],
        "outside_selection": [
            { "issue": 20, "reason": "post_release", "note": SENTENCE }
        ],
        "open_prs": { "captured_at_main": null, "rows": [] },
        "census": { "captured_on": "2026-10-21", "open_release_check_issues": [10, 11, 12, 20] },
        "non_claims": ["Not a cut selection."]
    })
}

/// A packet that satisfies every rule at `hard_cut_eligible`.
fn eligible() -> Value {
    let mut packet = valid();
    packet["stage"] = json!("hard_cut_eligible");
    packet["observed_main"] = json!({ "sha": SHA_A });
    packet["claims"] = json!([landed("a", 10), deferred("b", 11)]);
    packet["census"]["open_release_check_issues"] = json!([10, 11, 20]);
    packet["open_prs"] = json!({
        "captured_at_main": SHA_A,
        "rows": [{
            "pr": 500,
            "head": SHA_B,
            "disposition": "unrelated_to_selected_candidate",
            "note": SENTENCE
        }]
    });
    packet
}

fn run(packet: &Value) -> Vec<String> {
    evaluate_release_selection(
        RELEASE_SELECTION_PATH,
        &serde_json::to_string(packet).unwrap_or_default(),
    )
    .violations
}

fn rules_hit(violations: &[String]) -> BTreeSet<String> {
    violations
        .iter()
        .filter_map(|violation| {
            let start = violation.find('[')? + 1;
            let end = violation[start..].find(']')? + start;
            Some(violation[start..end].to_string())
        })
        .collect()
}

fn assert_trips_only(rule: &str, packet: &Value) {
    let violations = run(packet);
    let hit = rules_hit(&violations);
    assert_eq!(
        hit,
        BTreeSet::from([rule.to_string()]),
        "expected only `{rule}`, got {violations:?}"
    );
}

#[test]
fn valid_packets_pass_at_both_stages() {
    assert_eq!(run(&valid()), Vec::<String>::new());
    assert_eq!(run(&eligible()), Vec::<String>::new());
}

#[test]
fn committed_packet_passes() {
    let path = format!("{}/../{RELEASE_SELECTION_PATH}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(!text.is_empty(), "{path} must exist");
    let outcome = evaluate_release_selection(RELEASE_SELECTION_PATH, &text);
    assert_eq!(outcome.violations, Vec::<String>::new());
}

#[test]
fn schema_rejects_unknown_fields_and_wrong_identity() {
    let mut unknown = valid();
    unknown["claims"][0]["surprise"] = json!(true);
    assert_trips_only(RULE_SCHEMA, &unknown);

    let mut wrong_issue = valid();
    wrong_issue["authority_issue"] = json!(1);
    assert_trips_only(RULE_SCHEMA, &wrong_issue);

    let mut bad_sha = valid();
    bad_sha["observed_main"]["sha"] = json!("abc123");
    assert_trips_only(RULE_SCHEMA, &bad_sha);

    let mut no_census_date = valid();
    no_census_date["census"]["captured_on"] = json!(" ");
    assert_trips_only(RULE_SCHEMA, &no_census_date);

    let mut no_non_claims = valid();
    no_non_claims["non_claims"] = json!([]);
    assert_trips_only(RULE_SCHEMA, &no_non_claims);
}

#[test]
fn vocabulary_rejects_free_form_values() {
    let mut resolution = valid();
    resolution["claims"][0]["resolution"] = json!("done");
    assert_trips_only(RULE_VOCABULARY, &resolution);

    let mut materiality = valid();
    materiality["claims"][0]["materiality"] = json!("important");
    assert_trips_only(RULE_VOCABULARY, &materiality);

    let mut stage = valid();
    stage["stage"] = json!("candidate_materialized");
    assert_trips_only(RULE_VOCABULARY, &stage);

    let mut reason = valid();
    reason["outside_selection"][0]["reason"] = json!("someday");
    assert_trips_only(RULE_VOCABULARY, &reason);

    let mut disposition = eligible();
    disposition["open_prs"]["rows"][0]["disposition"] = json!("fine");
    assert_trips_only(RULE_VOCABULARY, &disposition);
}

#[test]
fn uniqueness_rejects_repeats() {
    let mut claim = valid();
    claim["claims"][1]["claim_id"] = json!("a");
    assert_trips_only(RULE_UNIQUENESS, &claim);

    let mut outside = valid();
    let row = outside["outside_selection"][0].clone();
    outside["outside_selection"] = json!([row.clone(), row]);
    assert_trips_only(RULE_UNIQUENESS, &outside);

    let mut census = valid();
    census["census"]["open_release_check_issues"] = json!([10, 10, 11, 12, 20]);
    assert_trips_only(RULE_UNIQUENESS, &census);

    let mut pr = eligible();
    let row = pr["open_prs"]["rows"][0].clone();
    pr["open_prs"]["rows"] = json!([row.clone(), row]);
    assert_trips_only(RULE_UNIQUENESS, &pr);
}

#[test]
fn evidence_requirements_follow_the_resolution() {
    let mut issue_link = valid();
    issue_link["claims"][0]["implementation_refs"] =
        json!(["https://github.com/EffortlessMetrics/ripr-swarm/issues/9"]);
    assert_trips_only(RULE_EVIDENCE, &issue_link);

    let mut no_landing = valid();
    no_landing["claims"][0]["implementation_refs"] = json!([]);
    assert_trips_only(RULE_EVIDENCE, &no_landing);

    let mut no_proof = valid();
    no_proof["claims"][0]["proof_refs"] = json!([]);
    assert_trips_only(RULE_EVIDENCE, &no_proof);

    let mut label_non_claim = valid();
    label_non_claim["claims"][1]["non_claim"] = json!("later");
    assert_trips_only(RULE_EVIDENCE, &label_non_claim);

    let mut pending_without_owner = valid();
    pending_without_owner["claims"][2]["next"] = json!("");
    assert_trips_only(RULE_EVIDENCE, &pending_without_owner);

    let mut exclusion = valid();
    exclusion["claims"][1]["resolution"] = json!("candidate_exclusion_with_manifest");
    assert_trips_only(RULE_EVIDENCE, &exclusion);

    let mut execution = valid();
    execution["claims"][1]["resolution"] = json!("final_candidate_execution_only");
    assert_trips_only(RULE_EVIDENCE, &execution);

    let mut hold = valid();
    hold["claims"][2]["resolution"] = json!("failed_release_hold");
    hold["claims"][2]["next"] = json!("x");
    assert_trips_only(RULE_EVIDENCE, &hold);

    let mut historical = valid();
    historical["claims"][1]["resolution"] = json!("historical_evidence_only");
    assert_trips_only(RULE_EVIDENCE, &historical);

    let mut no_effect = valid();
    no_effect["claims"][0]["candidate_effect"] = json!(" ");
    assert_trips_only(RULE_EVIDENCE, &no_effect);

    let mut short_sha = eligible();
    short_sha["open_prs"]["rows"][0]["head"] = json!("abc");
    assert_trips_only(RULE_EVIDENCE, &short_sha);
}

#[test]
fn dependency_must_name_another_claim() {
    let mut unknown = valid();
    unknown["claims"][0]["depends_on"] = json!(["missing"]);
    assert_trips_only(RULE_DEPENDENCY, &unknown);

    let mut itself = valid();
    itself["claims"][0]["depends_on"] = json!(["a"]);
    assert_trips_only(RULE_DEPENDENCY, &itself);
}

#[test]
fn an_issue_has_one_place_in_the_partition() {
    let mut both = valid();
    both["outside_selection"][0]["issue"] = json!(10);
    assert_trips_only(RULE_ISSUE_PARTITION, &both);

    let mut two_claims = valid();
    two_claims["claims"][1]["owner_issue"] = json!(10);
    assert_trips_only(RULE_ISSUE_PARTITION, &two_claims);

    let mut related_owner = valid();
    related_owner["claims"][0]["related_issues"] = json!([11]);
    assert_trips_only(RULE_ISSUE_PARTITION, &related_owner);

    let mut unplaced = valid();
    unplaced["census"]["open_release_check_issues"] = json!([10, 11, 12, 20, 99]);
    assert_trips_only(RULE_ISSUE_PARTITION, &unplaced);

    let mut related_twice = valid();
    related_twice["claims"][0]["related_issues"] = json!([30]);
    related_twice["claims"][1]["related_issues"] = json!([30]);
    assert_trips_only(RULE_ISSUE_PARTITION, &related_twice);
}

#[test]
fn stage_cannot_outrun_the_rows() {
    let mut closed_with_pending = valid();
    closed_with_pending["stage"] = json!("scope_closed");
    assert_trips_only(RULE_STAGE_LAW, &closed_with_pending);

    let mut hold = eligible();
    hold["claims"][1]["resolution"] = json!("failed_release_hold");
    hold["claims"][1]["next"] = json!("Fix the failing predicate; owner is the corpus thread.");
    assert_trips_only(RULE_STAGE_LAW, &hold);

    let mut no_main = eligible();
    no_main["observed_main"] = json!({ "sha": null });
    no_main["open_prs"]["captured_at_main"] = json!(null);
    no_main["open_prs"]["rows"] = json!([]);
    assert_trips_only(RULE_STAGE_LAW, &no_main);

    let mut roster_elsewhere = eligible();
    roster_elsewhere["open_prs"]["captured_at_main"] = json!(SHA_B);
    assert_trips_only(RULE_STAGE_LAW, &roster_elsewhere);

    let mut blocked = eligible();
    blocked["open_prs"]["rows"][0]["disposition"] = json!("blocked_on_named_authority");
    assert_trips_only(RULE_STAGE_LAW, &blocked);

    let mut rows_without_capture = valid();
    rows_without_capture["open_prs"]["rows"] = eligible()["open_prs"]["rows"].clone();
    assert_trips_only(RULE_STAGE_LAW, &rows_without_capture);
}

#[test]
fn unreadable_json_is_a_schema_violation() {
    let outcome = evaluate_release_selection(RELEASE_SELECTION_PATH, "{ not json");
    assert_eq!(
        rules_hit(&outcome.violations),
        BTreeSet::from([RULE_SCHEMA.to_string()])
    );
}

#[test]
fn every_rule_owns_a_negative_fixture() {
    // Each rule id must appear as the sole trip in at least one fixture above.
    // Collect them by replaying one representative edit per rule.
    let mut by_rule: Vec<(&str, Value)> = Vec::new();
    let mut schema = valid();
    schema["authority_issue"] = json!(1);
    by_rule.push((RULE_SCHEMA, schema));
    let mut vocabulary = valid();
    vocabulary["claims"][0]["resolution"] = json!("done");
    by_rule.push((RULE_VOCABULARY, vocabulary));
    let mut uniqueness = valid();
    uniqueness["claims"][1]["claim_id"] = json!("a");
    by_rule.push((RULE_UNIQUENESS, uniqueness));
    let mut evidence = valid();
    evidence["claims"][0]["proof_refs"] = json!([]);
    by_rule.push((RULE_EVIDENCE, evidence));
    let mut dependency = valid();
    dependency["claims"][0]["depends_on"] = json!(["missing"]);
    by_rule.push((RULE_DEPENDENCY, dependency));
    let mut partition = valid();
    partition["outside_selection"][0]["issue"] = json!(10);
    by_rule.push((RULE_ISSUE_PARTITION, partition));
    let mut stage = valid();
    stage["stage"] = json!("scope_closed");
    by_rule.push((RULE_STAGE_LAW, stage));

    let covered: BTreeSet<&str> = by_rule.iter().map(|(rule, _)| *rule).collect();
    let all: BTreeSet<&str> = RULE_IDS.iter().copied().collect();
    assert_eq!(covered, all, "a rule has no negative fixture");
    for (rule, packet) in &by_rule {
        assert_trips_only(rule, packet);
    }
}
