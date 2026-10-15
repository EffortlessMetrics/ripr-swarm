use super::*;
use crate::reports::verdict_corpus::coverage::{SPECS_DIR, load_ledger, scan_specs};
use crate::reports::verdict_corpus::{build_report, case_violations, expected_report, load_corpus};
use serde_json::json;

fn repo_path(relative: &str) -> std::path::PathBuf {
    crate::dogfood::repo_rooted_fixture_path(relative)
}

fn expected(kind: &str, strength: &str, test: Option<&str>) -> ExpectedOracle {
    ExpectedOracle {
        kind: kind.to_string(),
        strength: strength.to_string(),
        test: test.map(str::to_string),
        spec: "RIPR-SPEC-0231#1".to_string(),
    }
}

fn observed(test: &str, kind: &str, strength: &str) -> ObservedOracle {
    ObservedOracle {
        test: test.to_string(),
        kind: kind.to_string(),
        strength: strength.to_string(),
    }
}

fn finding(related: &[(&str, &str, &str)]) -> Value {
    let tests: Vec<Value> = related
        .iter()
        .map(|(name, kind, strength)| {
            json!({"name": name, "oracle_kind": kind, "oracle_strength": strength})
        })
        .collect();
    json!({"classification": "weakly_exposed", "related_tests": tests})
}

#[test]
fn classify_names_each_of_the_five_outcomes() {
    let want = expected("relational_check", "weak", None);
    let case = |kind: &str, strength: &str| classify(&want, Some(&observed("t", kind, strength)));
    assert_eq!(case("relational_check", "weak"), OracleOutcome::Match);
    assert_eq!(case("exact_value", "strong"), OracleOutcome::Overclaim);
    // A stronger reading of the same kind is still an overclaim.
    assert_eq!(case("relational_check", "medium"), OracleOutcome::Overclaim);
    assert_eq!(case("smoke_only", "smoke"), OracleOutcome::Underclaim);
    // ripr's `unknown` and `none` strengths rank below smoke.
    assert_eq!(case("unknown", "unknown"), OracleOutcome::Underclaim);
    assert_eq!(case("unknown", "none"), OracleOutcome::Underclaim);
    assert_eq!(case("broad_error", "weak"), OracleOutcome::KindMismatch);
    assert_eq!(classify(&want, None), OracleOutcome::Unobserved);
}

#[test]
fn strength_ranks_smoke_below_weak_below_medium_below_strong() {
    let ranks: Vec<u8> = ["none", "unknown", "smoke", "weak", "medium", "strong"]
        .iter()
        .map(|s| strength_rank(s))
        .collect();
    assert!(ranks.windows(2).all(|w| w[0] < w[1]), "{ranks:?}");
}

#[test]
fn the_claim_is_the_strongest_entry_and_prefers_the_expected_kind_on_a_tie() {
    // A custom helper reports the call (strong) and its body (weak) for one
    // test: the consumer sees the strong reading.
    let entries = vec![
        observed("t", "exact_value", "strong"),
        observed("t", "relational_check", "weak"),
    ];
    let claim = claimed(&entries, "relational_check");
    assert_eq!(claim.map(|c| c.kind.as_str()), Some("exact_value"));
    let tie = vec![
        observed("t", "exact_value", "strong"),
        observed("t", "whole_object_equality", "strong"),
    ];
    assert_eq!(
        claimed(&tie, "whole_object_equality").map(|c| c.kind.as_str()),
        Some("whole_object_equality")
    );
    assert_eq!(
        claimed(&tie, "snapshot").map(|c| c.kind.as_str()),
        Some("exact_value")
    );
    assert!(claimed(&[], "exact_value").is_none());
}

#[test]
fn observed_oracles_read_the_labeled_test_across_every_anchored_finding() -> Result<(), String> {
    let first = finding(&[
        ("score_is_not_zero", "exact_value", "strong"),
        ("score_is_not_zero", "relational_check", "weak"),
        ("other_test", "snapshot", "medium"),
    ]);
    let second = finding(&[("score_is_not_zero", "exact_value", "strong")]);
    let findings = vec![&first, &second];
    // A label may spell the module path; ripr reports the bare name.
    let named = observed_oracles(&findings, Some("tests::score_is_not_zero"));
    assert_eq!(
        named,
        vec![
            observed("score_is_not_zero", "exact_value", "strong"),
            observed("score_is_not_zero", "relational_check", "weak"),
        ],
        "duplicates collapse and the other test is left out"
    );
    assert_eq!(observed_oracles(&findings, None).len(), 3);
    assert!(observed_oracles(&findings, Some("missing")).is_empty());

    let row = oracle_row(
        "case",
        &expected("relational_check", "weak", Some("score_is_not_zero")),
        &findings,
    )?;
    assert_eq!(row.outcome, OracleOutcome::Overclaim);
    assert_eq!(row.observed_kind.as_deref(), Some("exact_value"));
    assert_eq!(row.observed_all.len(), 2);
    let none = oracle_row("case", &expected("exact_value", "strong", None), &[])?;
    assert_eq!(none.outcome, OracleOutcome::Unobserved);
    assert!(none.observed_kind.is_none());
    Ok(())
}

#[test]
fn a_label_that_could_be_elided_or_names_two_tests_is_refused() -> Result<(), String> {
    let label = expected("exact_value", "strong", Some("works"));
    // Ten related tests, eight rendered: a missing label may be one of the
    // two the JSON dropped, so `unobserved` would be a guess.
    let mut capped = finding(&[("other_test", "exact_value", "strong")]);
    capped["related_tests_total"] = json!(10);
    let refused = oracle_row("case", &label, &[&capped])
        .err()
        .ok_or("an elided list scored a missing label as unobserved")?;
    assert!(refused.contains("elided"), "{refused}");
    // The same miss with the whole list rendered is a real `unobserved`.
    capped["related_tests_total"] = json!(1);
    assert_eq!(
        oracle_row("case", &label, &[&capped])?.outcome,
        OracleOutcome::Unobserved
    );
    // A found label is scored even when other entries were elided.
    let mut found = finding(&[("works", "exact_value", "strong")]);
    found["related_tests_total"] = json!(10);
    assert_eq!(
        oracle_row("case", &label, &[&found])?.outcome,
        OracleOutcome::Match
    );

    // Two `works` tests in different files: neither reading belongs to the
    // label alone.
    let twins = json!({"related_tests": [
        {"name": "works", "file": "tests/alpha.rs", "line": 3,
         "oracle_kind": "relational_check", "oracle_strength": "weak"},
        {"name": "works", "file": "tests/beta.rs", "line": 3,
         "oracle_kind": "exact_value", "oracle_strength": "strong"},
    ]});
    let refused = oracle_row("case", &label, &[&twins])
        .err()
        .ok_or("a label matching two tests was scored")?;
    assert!(refused.contains("matches 2 reported tests"), "{refused}");
    // One test reported by two findings is still one test.
    let once = json!({"related_tests": [
        {"name": "works", "file": "tests/alpha.rs", "line": 3,
         "oracle_kind": "exact_value", "oracle_strength": "strong"},
    ]});
    assert_eq!(
        oracle_row("case", &label, &[&once, &once])?.outcome,
        OracleOutcome::Match
    );
    Ok(())
}

fn labeled_case(edit: impl Fn(&mut Value)) -> Result<Vec<String>, String> {
    let dir = repo_path(CORPUS_DIR);
    let corpus = load_corpus(&dir)?;
    let first = corpus.cases.first().ok_or("corpus has no cases")?;
    let mut case = serde_json::to_value(first).map_err(|e| e.to_string())?;
    case["expected_oracle"] =
        json!({"kind": "exact_value", "strength": "strong", "spec": "RIPR-SPEC-0231#6"});
    edit(&mut case);
    let case: Case = serde_json::from_value(case).map_err(|e| e.to_string())?;
    let subject = corpus
        .subjects
        .iter()
        .find(|s| s.subject_id == case.subject_id)
        .ok_or("subject missing")?;
    Ok(case_violations(&case, subject, &dir))
}

#[test]
fn validation_rejects_unknown_kinds_strengths_and_malformed_labels() -> Result<(), String> {
    assert!(labeled_case(|_| {})?.is_empty());
    let kind = labeled_case(|c| c["expected_oracle"]["kind"] = json!("exactish"))?;
    assert!(
        kind.iter().any(|v| v.contains("kind `exactish`")),
        "{kind:#?}"
    );
    // `unknown` and `none` are ripr strengths, but no spec adopts them.
    for strength in ["unknown", "none", "Strong"] {
        let bad = labeled_case(|c| c["expected_oracle"]["strength"] = json!(strength))?;
        assert!(
            bad.iter()
                .any(|v| v.contains(&format!("strength `{strength}`"))),
            "{bad:#?}"
        );
    }
    let spec = labeled_case(|c| c["expected_oracle"]["spec"] = json!("SPEC-0231#6"))?;
    assert!(
        spec.iter().any(|v| v.contains("is not `RIPR-SPEC-NNNN#K`")),
        "{spec:#?}"
    );
    let test = labeled_case(|c| c["expected_oracle"]["test"] = json!(" "))?;
    assert!(test.iter().any(|v| v.contains("test is present but empty")));
    // Unknown fields fail at parse time.
    let extra = labeled_case(|c| c["expected_oracle"]["strong"] = json!(true));
    if extra.is_ok() {
        return Err("an unknown expected_oracle field parsed".to_string());
    }
    Ok(())
}

#[test]
fn ledger_needs_a_gate_within_the_labeled_count_and_real_spec_examples() -> Result<(), String> {
    let dir = repo_path(CORPUS_DIR);
    let corpus = load_corpus(&dir)?;
    let specs = scan_specs(&repo_path(SPECS_DIR))?;
    let mut ledger = load_ledger(&dir)?;
    let labeled = corpus
        .cases
        .iter()
        .filter(|c| c.expected_oracle.is_some())
        .count();
    assert!(oracle_ledger_violations(&corpus, &ledger, &specs).is_empty());

    ledger.oracle = Some(OracleGate {
        match_floor: labeled + 1,
        overclaim_ceiling: labeled + 1,
    });
    let over = oracle_ledger_violations(&corpus, &ledger, &specs);
    assert_eq!(over.len(), 2, "{over:#?}");

    ledger.oracle = None;
    let missing = oracle_ledger_violations(&corpus, &ledger, &specs);
    assert!(
        missing.iter().any(|v| v.contains("has no [oracle] table")),
        "{missing:#?}"
    );

    let mut bad_spec = corpus.clone();
    if let Some(label) = bad_spec
        .cases
        .iter_mut()
        .find_map(|c| c.expected_oracle.as_mut())
    {
        label.spec = "RIPR-SPEC-0231#99".to_string();
    }
    let ledger = load_ledger(&dir)?;
    let cited = oracle_ledger_violations(&bad_spec, &ledger, &specs);
    assert!(
        cited.iter().any(|v| v.contains("`RIPR-SPEC-0231#99`")),
        "{cited:#?}"
    );
    Ok(())
}

fn gated(outcomes: &[OracleOutcome], floor: usize, ceiling: usize) -> OracleAccuracy {
    let mut accuracy = oracle_accuracy(outcomes.iter().copied());
    accuracy.match_floor = Some(floor);
    accuracy.overclaim_ceiling = Some(ceiling);
    accuracy
}

#[test]
fn gate_fails_below_the_floor_or_above_the_ceiling_and_invites_tightening() -> Result<(), String> {
    use OracleOutcome::{KindMismatch, Match, Overclaim, Unobserved};
    let outcomes = [Match, Match, Overclaim, Overclaim, KindMismatch, Unobserved];
    let accuracy = oracle_accuracy(outcomes.iter().copied());
    assert_eq!(accuracy.labeled, 6);
    assert_eq!(accuracy.oracle_accuracy.rate, "0.3333");
    assert_eq!(accuracy.oracle_overclaim_rate.numerator, 2);
    assert_eq!(accuracy.count(OracleOutcome::Underclaim), 0);

    // At floor and ceiling: passes silently.
    assert_eq!(oracle_gate(&gated(&outcomes, 2, 2))?, None);
    // Below the floor.
    let below = oracle_gate(&gated(&outcomes, 3, 2))
        .err()
        .unwrap_or_default();
    assert!(
        below.contains("matches fell to 2/6 below the floor 3"),
        "{below}"
    );
    assert!(below.contains("lower `match_floor`"), "{below}");
    // Above the ceiling.
    let above = oracle_gate(&gated(&outcomes, 2, 1))
        .err()
        .unwrap_or_default();
    assert!(
        above.contains("overclaims rose to 2/6 above the ceiling 1"),
        "{above}"
    );
    // Improvement passes and names the number to tighten.
    let better = oracle_gate(&gated(&outcomes, 1, 3))?.unwrap_or_default();
    assert!(
        better.contains("lower `overclaim_ceiling`") && better.contains("to 2"),
        "{better}"
    );
    assert!(
        better.contains("raise `match_floor`") && better.contains("to 2"),
        "{better}"
    );
    // Without a gate table the gate has nothing to hold.
    assert_eq!(
        oracle_gate(&oracle_accuracy(outcomes.iter().copied()))?,
        None
    );
    Ok(())
}

#[test]
fn oracle_scores_stay_out_of_the_verdict_rates() -> Result<(), String> {
    let dir = repo_path(CORPUS_DIR);
    let corpus = load_corpus(&dir)?;
    // Every case reads an exact_value / strong related test on a gap: the
    // verdict rates are those of an all-gap run whatever the oracle labels.
    let checks: Vec<(String, Value)> = corpus
        .cases
        .iter()
        .map(|case| {
            let test = case
                .expected_oracle
                .as_ref()
                .and_then(|e| e.test.as_deref())
                .unwrap_or("t");
            let mut f = finding(&[(test, "exact_value", "strong")]);
            f["probe"] = json!({"file": case.anchor.file, "line": case.anchor.line});
            f["source_currentness"] = json!("candidate_current");
            (case.case_id.clone(), json!({"findings": [f]}))
        })
        .collect();
    let report = build_report(&corpus, &checks, &BTreeMap::new())?;
    let labeled: Vec<&ExpectedOracle> = corpus
        .cases
        .iter()
        .filter_map(|c| c.expected_oracle.as_ref())
        .collect();
    assert_eq!(report.oracle_accuracy.labeled, labeled.len());
    let want_overclaims = labeled.iter().filter(|e| e.strength != "strong").count();
    assert_eq!(
        report.oracle_accuracy.count(OracleOutcome::Overclaim),
        want_overclaims
    );
    let discriminated = corpus
        .cases
        .iter()
        .filter(|c| c.truth.state == super::super::TruthState::Discriminated)
        .count();
    assert_eq!(report.false_verdict_rate.numerator, discriminated);
    let rows_with_oracle = report.rows.iter().filter(|r| r.oracle.is_some()).count();
    assert_eq!(rows_with_oracle, labeled.len());
    Ok(())
}

#[test]
fn committed_labels_are_valid_and_the_expected_report_meets_the_gate() -> Result<(), String> {
    let dir = repo_path(CORPUS_DIR);
    let corpus = load_corpus(&dir)?;
    let ledger = load_ledger(&dir)?;
    let specs = scan_specs(&repo_path(SPECS_DIR))?;
    assert!(oracle_ledger_violations(&corpus, &ledger, &specs).is_empty());
    let spec0231: Vec<&Case> = corpus
        .cases
        .iter()
        .filter(|c| c.case_id.starts_with("spec0231-"))
        .collect();
    assert_eq!(spec0231.len(), 38);
    for case in &spec0231 {
        assert!(
            case.expected_oracle.is_some(),
            "{} has no expected_oracle",
            case.case_id
        );
    }
    let gate = ledger.oracle.ok_or("no [oracle] table")?;
    // The committed rows are the expected state; the summary derives from
    // them, so the gate must equal what they record.
    let report = expected_report(&dir)?;
    // Each row must carry its case's label and the outcome its own observed
    // entries score to, so a hand-edited row cannot satisfy the counts.
    for case in &corpus.cases {
        let row = report
            .rows
            .iter()
            .find(|row| row.case_id == case.case_id)
            .ok_or(format!("no row for {}", case.case_id))?;
        match (&case.expected_oracle, &row.oracle) {
            (None, None) => {}
            (Some(label), Some(scored)) => {
                assert_eq!(
                    serde_json::to_value(label).map_err(|e| e.to_string())?,
                    serde_json::to_value(&scored.expected).map_err(|e| e.to_string())?,
                    "{}: the row's expected oracle is not the case label",
                    case.case_id
                );
                let claim = claimed(&scored.observed_all, &label.kind);
                assert_eq!(
                    scored.outcome,
                    classify(label, claim),
                    "{}: the row's outcome is not what its observed entries score",
                    case.case_id
                );
                assert_eq!(scored.observed_kind, claim.map(|c| c.kind.clone()));
                assert_eq!(scored.observed_strength, claim.map(|c| c.strength.clone()));
            }
            _ => {
                return Err(format!(
                    "{}: label and row disagree on whether the case is scored",
                    case.case_id
                ));
            }
        }
    }
    let accuracy = report.oracle_accuracy;
    assert_eq!(
        accuracy.count(OracleOutcome::Match),
        gate.match_floor,
        "match_floor is not the committed match count"
    );
    assert_eq!(
        accuracy.count(OracleOutcome::Overclaim),
        gate.overclaim_ceiling,
        "overclaim_ceiling is not the committed overclaim count"
    );
    Ok(())
}

#[test]
fn the_gate_arms_only_for_a_whole_corpus_run_with_an_oracle_table() {
    let gate = OracleGate {
        match_floor: 3,
        overclaim_ceiling: 1,
    };
    let fresh = || oracle_accuracy([OracleOutcome::Match]);

    let mut whole = fresh();
    arm_gate(
        &mut whole,
        "fixtures/x/spec-coverage.toml".to_string(),
        Some(&gate),
        false,
    );
    assert_eq!(whole.ledger, "fixtures/x/spec-coverage.toml");
    assert_eq!(
        (whole.match_floor, whole.overclaim_ceiling),
        (Some(3), Some(1))
    );
    // One match against a floor of three fails once armed.
    let armed = oracle_gate(&whole);
    assert!(
        matches!(&armed, Err(failure) if failure.contains("below the floor 3")),
        "{armed:?}"
    );

    let mut subset = fresh();
    arm_gate(
        &mut subset,
        "fixtures/x/spec-coverage.toml".to_string(),
        Some(&gate),
        true,
    );
    assert_eq!((subset.match_floor, subset.overclaim_ceiling), (None, None));
    assert_eq!(oracle_gate(&subset), Ok(None));

    let mut tableless = fresh();
    arm_gate(
        &mut tableless,
        "fixtures/y/spec-coverage.toml".to_string(),
        None,
        false,
    );
    assert_eq!(tableless.ledger, "fixtures/y/spec-coverage.toml");
    assert_eq!(oracle_gate(&tableless), Ok(None));
}

#[test]
fn a_corpus_without_a_ledger_refuses_oracle_labels() -> Result<(), String> {
    let dir = repo_path(CORPUS_DIR);
    let mut corpus = load_corpus(&dir)?;
    assert!(
        ungated_label_violations(&corpus, &dir)
            .iter()
            .any(|v| v.contains("has no spec-coverage.toml"))
    );
    for case in &mut corpus.cases {
        case.expected_oracle = None;
    }
    assert!(ungated_label_violations(&corpus, &dir).is_empty());
    Ok(())
}

#[test]
fn the_coverage_board_refuses_labels_without_an_oracle_table() -> Result<(), String> {
    // The dx-scoreboard coverage producer validates on its own; it must
    // reject the same ledger `validate` rejects instead of publishing a rate.
    let root = crate::tests::temp_dir("oracle-ledger-coverage-board");
    let corpus = root.join("fixtures/rust-verdict-corpus");
    crate::tests::copy_dir_recursive(&repo_path(CORPUS_DIR), &corpus)?;
    crate::tests::copy_dir_recursive(&repo_path(SPECS_DIR), &root.join(SPECS_DIR))?;
    let ledger_path = corpus.join("spec-coverage.toml");
    let ledger = std::fs::read_to_string(&ledger_path).map_err(|e| e.to_string())?;
    let stripped: String = ledger
        .lines()
        .filter(|line| {
            !(line.trim() == "[oracle]"
                || line.starts_with("match_floor")
                || line.starts_with("overclaim_ceiling"))
        })
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(
        ledger.lines().count() - stripped.lines().count(),
        3,
        "fixture: the committed ledger has an [oracle] table"
    );
    crate::tests::write(&ledger_path, &stripped);
    let err = crate::reports::verdict_corpus::coverage::spec_example_coverage_for_corpus(&corpus)
        .err()
        .ok_or("the coverage board scored a corpus whose labels have no [oracle] table")?;
    assert!(err.contains("no [oracle] table"), "{err}");
    Ok(())
}
