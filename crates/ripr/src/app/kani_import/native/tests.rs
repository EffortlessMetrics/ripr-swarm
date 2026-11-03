//! Published contract fixtures, authored mutations and separately retained native captures.

use super::*;
use serde_json::json;

const SUCCESS: &str = include_str!("../fixtures/fork-completed-success.json");
const FAILURE: &str = include_str!("../fixtures/completed-failure.json");
type TestResult = Result<(), String>;

fn document(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| error.to_string())
}

// Authored controls: retain valid metadata, explicitly replace all verification results.
// These documents are constructed test inputs, never captured native runs.
fn resultless(outcomes: &[Value], selected: u64, state: &str) -> Result<String, String> {
    let mut value = document(SUCCESS)?;
    let template = value["harnesses"][0].clone();
    let mut harnesses = Vec::new();
    for (index, outcome) in outcomes.iter().enumerate() {
        let mut harness = template.clone();
        let _removed = harness
            .as_object_mut()
            .ok_or("harness object")?
            .remove("failure_kind");
        harness["name"] = json!(format!("resultless_{index}"));
        harness["outcome"] = outcome.clone();
        for key in ["n_properties", "n_failed"] {
            harness[key] = Value::Null;
        }
        for key in ["failed_properties", "unsupported_constructs"] {
            harness[key] = json!([]);
        }
        for domain in ["checks", "covers"] {
            for (key, count_or_list) in harness[domain].as_object_mut().ok_or("result bucket")? {
                *count_or_list = if matches!(key.as_str(), "total" | "success") {
                    Value::Null
                } else {
                    json!([])
                };
            }
        }
        harnesses.push(harness);
    }
    value["harnesses"] = json!(harnesses);
    value["harness_selection"]["matched_count"] = json!(selected);
    value["run_state"] = json!(state);
    value["harness_timeout_s"] = json!(30);
    for count in value["summary"]
        .as_object_mut()
        .ok_or("summary object")?
        .values_mut()
    {
        *count = json!(0);
    }
    value["summary"]["total"] = json!(outcomes.len());
    Ok(value.to_string())
}

fn partial() -> Result<String, String> {
    resultless(
        &[
            json!({"kind":"TIMEOUT"}),
            json!({"kind":"OUT_OF_MEMORY"}),
            json!({"kind":"CRASHED", "code":6, "message":null}),
        ],
        5,
        "PARTIAL",
    )
}

fn changed(base: &str, changes: &[(&str, Value)]) -> Result<Value, String> {
    let mut value = document(base)?;
    for (path, replacement) in changes {
        *value
            .pointer_mut(path)
            .ok_or_else(|| format!("missing test field {path}"))? = replacement.clone();
    }
    Ok(value)
}

fn rejected(text: &str, fragment: &str) -> TestResult {
    match parse_native(text) {
        Err(error) => {
            assert!(error.contains(fragment), "{error:?} lacks {fragment:?}");
            Ok(())
        }
        Ok(_) => Err(format!(
            "accepted invalid native document: expected {fragment}"
        )),
    }
}

#[test]
fn published_contract_fixtures_preserve_native_document_and_counts() -> TestResult {
    for fixture in [SUCCESS, FAILURE] {
        assert_eq!(parse_native(fixture)?.document, document(fixture)?);
    }
    let NativeDocument { run, document } = parse_native(SUCCESS)?;
    assert_eq!((run.matched_count, run.missing_count), (1, 0));
    assert_eq!(run.run_state, NativeRunState::Complete);
    let h = run.harnesses.first().ok_or("missing success harness")?;
    assert_eq!(
        (h.checks_total, h.checks_success, h.covers_total),
        (Some(3), Some(2), Some(1))
    );
    assert_eq!(
        h.outcome,
        NativeOutcome::Completed {
            verdict: NativeVerdict::Success,
            failure_kind: NativeFailureKind::None
        }
    );
    assert_eq!(
        document["harnesses"][0]["checks"]["unreachable"],
        json!(["check_ok.assertion.3"])
    );
    let omitted = changed(
        FAILURE,
        &[("/harnesses/0/unsupported_constructs", json!([]))],
    )?;
    rejected(&omitted.to_string(), "missing from unsupported_constructs")?;
    Ok(())
}

#[test]
fn partial_results_and_empty_selection_never_become_successes() -> TestResult {
    let partial = partial()?;
    let run = parse_native(&partial)?.run;
    assert_eq!(run.run_state, NativeRunState::Partial);
    assert_eq!(
        (run.matched_count, run.missing_count, run.harnesses.len()),
        (5, 2, 3)
    );
    let outcomes: Vec<_> = run.harnesses.iter().map(|h| h.outcome.clone()).collect();
    assert_eq!(
        outcomes,
        vec![
            NativeOutcome::Timeout,
            NativeOutcome::OutOfMemory,
            NativeOutcome::Crashed {
                code: Some(6),
                message: None
            }
        ]
    );
    assert!(
        run.harnesses
            .iter()
            .all(|h| h.checks_total.is_none() && h.checks_success.is_none())
    );
    let complete = changed(
        &partial,
        &[
            ("/run_state", json!("COMPLETE")),
            ("/harness_selection/matched_count", json!(3)),
        ],
    )?;
    let complete = parse_native(&complete.to_string())?.run;
    assert_eq!(complete.run_state, NativeRunState::Complete);
    assert_eq!(complete.missing_count, 0);
    assert_eq!(complete.harnesses, run.harnesses);
    let empty_document = resultless(&[], 0, "NO_HARNESSES_SELECTED")?;
    let empty = parse_native(&empty_document)?.run;
    assert_eq!(empty.run_state, NativeRunState::NoHarnessesSelected);
    assert_eq!((empty.matched_count, empty.missing_count), (0, 0));
    assert!(empty.harnesses.is_empty());
    let with_filter = changed(
        &empty_document,
        &[("/harness_selection/requested_filters", json!(["missing"]))],
    )?;
    rejected(&with_filter.to_string(), "NO_HARNESSES_SELECTED")
}

#[test]
fn malformed_duplicate_truncated_and_oversize_documents_are_rejected() -> TestResult {
    for input in [
        "",
        "{",
        "{} {}",
        r#"{"schema_version":"0.1.0","schema_version":"0.1.0"}"#,
        r#"{"extension":{"a":1,"a":2}}"#,
    ] {
        rejected(input, "invalid native JSON")?;
    }
    let truncated = SUCCESS.get(..SUCCESS.len() - 2).ok_or("truncation range")?;
    rejected(truncated, "invalid native JSON")?;
    rejected(&" ".repeat(MAX_NATIVE_BYTES + 1), "exceeds 16 MiB")
}

#[test]
fn unknown_closed_values_and_cross_field_mismatches_are_rejected() -> TestResult {
    for (path, value, message) in [
        (
            "/schema_version",
            json!("1.0"),
            "unsupported Kani schema_version",
        ),
        ("/run_state", json!("INCOMPLETE"), "run_state"),
        ("/harness_selection/matched_count", json!(2), "COMPLETE"),
        ("/summary/successful", json!(0), "summary.successful"),
        (
            "/harnesses/0/outcome/kind",
            json!("COMPLETED_NEW"),
            "unsupported native outcome.kind",
        ),
        ("/harnesses/0/outcome/verdict", json!("UNKNOWN"), "verdict"),
        (
            "/harnesses/0/failure_kind",
            json!("NEW_KIND"),
            "failure_kind",
        ),
        ("/harnesses/0/checks/total", json!(4), "bucket total"),
        ("/harnesses/0/checks/success", json!(null), "bucket counts"),
        ("/harnesses/0/n_properties", json!(3), "n_properties"),
        ("/harnesses/0/n_failed", json!(1), "n_failed"),
        (
            "/harnesses/0/checks/other",
            json!([{"id":"extra","status":"NEW_STATUS"}]),
            "unsupported native property status",
        ),
        (
            "/harnesses/0/is_bounded",
            json!(true),
            "automatically generated",
        ),
        (
            "/started_at",
            json!("2026-02-30T00:00:00Z"),
            "real UTC date",
        ),
        ("/kani_commit_dirty", json!(null), "null together"),
        (
            "/harnesses/0/checks/success",
            json!(u64::MAX),
            "count overflow",
        ),
    ] {
        rejected(&changed(SUCCESS, &[(path, value)])?.to_string(), message)?;
    }
    rejected(
        &changed(
            &partial()?,
            &[("/harness_selection/matched_count", json!(3))],
        )?
        .to_string(),
        "PARTIAL",
    )
}

#[test]
fn nullable_fields_still_have_required_presence_and_outcome_specific_shape() -> TestResult {
    let partial = partial()?;
    for key in ["resolved_solver", "resolved_unwind", "contract"] {
        let mut value = document(SUCCESS)?;
        let _removed = value["harnesses"][0]
            .as_object_mut()
            .ok_or("harness object")?
            .remove(key);
        rejected(&value.to_string(), "missing required field")?;
    }
    for change in [
        (
            "/harnesses/0/outcome",
            json!({"kind":"TIMEOUT", "verdict":"SUCCESS"}),
        ),
        ("/harnesses/0/checks/total", json!(0)),
        ("/harnesses/2/outcome", json!({"kind":"CRASHED", "code":6})),
        (
            "/harnesses/2/outcome",
            json!({"kind":"CRASHED", "message":null}),
        ),
        ("/harness_timeout_s", json!(null)),
    ] {
        match parse_native(&changed(&partial, &[change])?.to_string()) {
            Err(_) => (),
            Ok(_) => return Err("accepted malformed non-completed outcome".to_owned()),
        }
    }
    let zero = changed(&partial, &[("/harnesses/2/outcome/code", json!(0))])?;
    parse_native(&zero.to_string())?; // Exit zero can still mean incomplete CBMC results.
    Ok(())
}

#[test]
fn duplicate_subjects_and_property_identities_are_rejected() -> TestResult {
    let mut value = document(SUCCESS)?;
    let harness = value["harnesses"][0].clone();
    value["harnesses"] = json!([harness.clone(), harness]);
    value["harness_selection"]["matched_count"] = json!(2);
    for (_, count) in value["summary"].as_object_mut().ok_or("summary object")? {
        *count = json!(count.as_u64().ok_or("summary count")? * 2);
    }
    rejected(&value.to_string(), "duplicate (crate_name, name)")?;
    value["harnesses"][1]["crate_name"] = json!("another_crate");
    assert_eq!(parse_native(&value.to_string())?.run.harnesses.len(), 2);
    let dup = changed(
        SUCCESS,
        &[
            (
                "/harnesses/0/checks/unreachable",
                json!(["check_ok.assertion.3", "check_ok.assertion.3"]),
            ),
            ("/harnesses/0/checks/total", json!(4)),
        ],
    )?;
    rejected(&dup.to_string(), "duplicate property identity")?;
    let overlap = changed(
        SUCCESS,
        &[(
            "/harnesses/0/covers/satisfied",
            json!(["check_ok.assertion.3"]),
        )],
    )?;
    rejected(&overlap.to_string(), "checks and covers")
}

#[test]
fn expected_panic_success_keeps_its_failed_property_and_polarity() -> TestResult {
    let mut value = changed(
        SUCCESS,
        &[
            ("/harnesses/0/attributes/should_panic", json!(true)),
            ("/harnesses/0/failure_kind", json!("PANICS_ONLY")),
            ("/harnesses/0/checks/success", json!(1)),
            (
                "/harnesses/0/checks/failure",
                json!(["check_ok.assertion.1"]),
            ),
            ("/harnesses/0/n_failed", json!(1)),
            ("/summary/checks_success", json!(1)),
            (
                "/harnesses/0/failed_properties",
                json!([{"id":"check_ok.assertion.1", "status":"FAILURE",
            "class":"assertion", "description":"expected panic", "file":"test.rs", "line":"7", "trace_available":true}]),
            ),
        ],
    )?;
    let run = parse_native(&value.to_string())?.run;
    assert!(matches!(
        run.harnesses.first().map(|h| &h.outcome),
        Some(NativeOutcome::Completed {
            verdict: NativeVerdict::Success,
            failure_kind: NativeFailureKind::PanicsOnly
        })
    ));
    value["harnesses"][0]["attributes"]["should_panic"] = json!(false);
    rejected(&value.to_string(), "verdict contradicts")?;
    value["harnesses"][0]["outcome"]["verdict"] = json!("FAILURE");
    value["summary"]["successful"] = json!(0);
    value["summary"]["failed"] = json!(1);
    parse_native(&value.to_string())?;
    let no_panic = changed(
        SUCCESS,
        &[
            ("/harnesses/0/attributes/should_panic", json!(true)),
            ("/harnesses/0/failure_kind", json!("PANICS_ONLY")),
        ],
    )?;
    rejected(&no_panic.to_string(), "PANICS_ONLY requires nonempty")
}

#[test]
fn ignored_quantifier_error_does_not_require_an_error_property() -> TestResult {
    let value = changed(
        SUCCESS,
        &[
            ("/harnesses/0/failure_kind", json!("ERROR")),
            ("/harnesses/0/outcome/verdict", json!("FAILURE")),
            ("/summary/successful", json!(0)),
            ("/summary/failed", json!(1)),
        ],
    )?;
    let run = parse_native(&value.to_string())?.run;
    assert!(matches!(
        run.harnesses.first().map(|h| &h.outcome),
        Some(NativeOutcome::Completed {
            verdict: NativeVerdict::Failure,
            failure_kind: NativeFailureKind::Error
        })
    ));
    Ok(())
}

#[test]
fn empty_property_results_are_observations_and_additive_fields_are_preserved() -> TestResult {
    let mut value = changed(
        SUCCESS,
        &[
            ("/harnesses/0/checks/total", json!(0)),
            ("/harnesses/0/checks/success", json!(0)),
            ("/harnesses/0/checks/unreachable", json!([])),
            ("/harnesses/0/unsupported_constructs", json!([])),
            ("/harnesses/0/n_properties", json!(1)),
            ("/summary/checks_total", json!(0)),
            ("/summary/checks_success", json!(0)),
            ("/harnesses/0/resolved_solver", json!("future-solver")),
            ("/harnesses/0/attributes/solver", json!("FutureSolver")),
        ],
    )?;
    value["future_extension"] = json!({"preserve":"whole document"});
    value["harnesses"][0]["outcome"]["future_detail"] = json!([1, 2, 3]);
    let NativeDocument { run, document } = parse_native(&value.to_string())?;
    assert_eq!(document, value);
    assert_eq!(
        run.harnesses.first().ok_or("harness")?.checks_total,
        Some(0)
    );
    Ok(())
}

#[test]
fn captured_ordinary_success_and_failure_retain_native_polarity() -> TestResult {
    let success = parse_native(include_str!(
        "../fixtures/native-captures/success.native.json"
    ))?;
    let failure = parse_native(include_str!(
        "../fixtures/native-captures/failure.native.json"
    ))?;
    for (run, name, verdict, kind, successful, covers) in [
        (
            &success.run,
            "b_success",
            NativeVerdict::Success,
            NativeFailureKind::None,
            2,
            1,
        ),
        (
            &failure.run,
            "a_failure",
            NativeVerdict::Failure,
            NativeFailureKind::PanicsOnly,
            1,
            0,
        ),
    ] {
        assert_eq!(run.run_state, NativeRunState::Complete);
        assert_eq!(
            (run.matched_count, run.missing_count, run.harnesses.len()),
            (1, 0, 1)
        );
        let harness = run.harnesses.first().ok_or("captured harness")?;
        assert_eq!(
            (
                harness.crate_name.as_str(),
                harness.file.as_str(),
                harness.name.as_str()
            ),
            ("proof", "proof.rs", name)
        );
        assert!(!harness.should_panic);
        assert_eq!(
            harness.outcome,
            NativeOutcome::Completed {
                verdict,
                failure_kind: kind
            }
        );
        assert_eq!(
            (
                harness.checks_total,
                harness.checks_success,
                harness.covers_total
            ),
            (Some(2), Some(successful), Some(covers))
        );
    }
    assert_eq!(
        success.document["harnesses"][0]["covers"]["satisfied"],
        json!(["b_success.cover.1"])
    );
    assert_eq!(
        failure.document["harnesses"][0]["checks"]["failure"],
        json!(["a_failure.assertion.1"])
    );
    Ok(())
}

#[test]
fn captured_vacuous_success_keeps_unreachable_assertion_and_cover() -> TestResult {
    let native = parse_native(include_str!(
        "../fixtures/native-captures/vacuous.native.json"
    ))?;
    assert_eq!(native.run.harnesses.len(), 1);
    let harness = native.run.harnesses.first().ok_or("vacuous harness")?;
    assert_eq!(harness.name, "c_vacuous");
    assert_eq!(
        harness.outcome,
        NativeOutcome::Completed {
            verdict: NativeVerdict::Success,
            failure_kind: NativeFailureKind::None,
        }
    );
    assert_eq!(
        (
            harness.checks_total,
            harness.checks_success,
            harness.covers_total
        ),
        (Some(1), Some(0), Some(1))
    );
    let h = &native.document["harnesses"][0];
    assert_eq!(h["checks"]["unreachable"], json!(["c_vacuous.assertion.1"]));
    assert_eq!(h["covers"]["unreachable"], json!(["c_vacuous.cover.1"]));
    assert_eq!(h["covers"]["satisfied"], json!([]));
    Ok(())
}

#[test]
fn captured_expected_and_missing_panic_keep_opposite_native_verdicts() -> TestResult {
    for (text, name, verdict, kind, successes, failures) in [
        (
            include_str!("../fixtures/native-captures/expected-panic.native.json"),
            "d_expected_panic",
            NativeVerdict::Success,
            NativeFailureKind::PanicsOnly,
            0,
            1,
        ),
        (
            include_str!("../fixtures/native-captures/missing-expected-panic.native.json"),
            "e_missing_expected_panic",
            NativeVerdict::Failure,
            NativeFailureKind::None,
            1,
            0,
        ),
    ] {
        let native = parse_native(text)?;
        assert_eq!(native.run.harnesses.len(), 1);
        let harness = native.run.harnesses.first().ok_or("panic harness")?;
        assert_eq!(harness.name, name);
        assert!(harness.should_panic);
        assert_eq!(
            harness.outcome,
            NativeOutcome::Completed {
                verdict,
                failure_kind: kind
            }
        );
        assert_eq!(
            (harness.checks_total, harness.checks_success),
            (Some(1), Some(successes))
        );
        assert_eq!(native.document["harnesses"][0]["n_failed"], failures);
    }
    Ok(())
}

#[test]
fn captured_unwind_results_preserve_undetermined_and_successful_unsupported_checks() -> TestResult {
    let sufficient = parse_native(include_str!(
        "../fixtures/native-captures/bounded-success.native.json"
    ))?;
    let insufficient = parse_native(include_str!(
        "../fixtures/native-captures/insufficient-bound.native.json"
    ))?;
    for native in [&sufficient, &insufficient] {
        assert_eq!(native.run.harnesses.len(), 1);
        assert_eq!(native.document["harnesses"][0]["resolved_unwind"], 3);
        // is_bounded describes automatic argument generation, not loop unwinding.
        assert_eq!(native.document["harnesses"][0]["is_bounded"], false);
    }
    let good = sufficient
        .run
        .harnesses
        .first()
        .ok_or("sufficient-bound harness")?;
    assert_eq!(
        good.outcome,
        NativeOutcome::Completed {
            verdict: NativeVerdict::Success,
            failure_kind: NativeFailureKind::None,
        }
    );
    assert_eq!(
        (good.checks_total, good.checks_success),
        (Some(32), Some(31))
    );
    let unsupported = sufficient.document["harnesses"][0]["unsupported_constructs"]
        .as_array()
        .ok_or("unsupported records")?;
    assert_eq!(unsupported.len(), 2);
    assert_eq!(
        unsupported[0]["id"],
        "std::panic::Location::<'_>::caller.unsupported_construct.1"
    );
    assert_eq!(
        unsupported[1]["id"],
        "core::panicking::panic_nounwind_fmt::runtime.unsupported_construct.1"
    );
    assert!(
        unsupported
            .iter()
            .all(|property| property["status"] == "SUCCESS")
    );
    let bad = insufficient
        .run
        .harnesses
        .first()
        .ok_or("insufficient-bound harness")?;
    assert_eq!(
        bad.outcome,
        NativeOutcome::Completed {
            verdict: NativeVerdict::Failure,
            failure_kind: NativeFailureKind::Other,
        }
    );
    assert_eq!((bad.checks_total, bad.checks_success), (Some(32), Some(0)));
    let checks = &insufficient.document["harnesses"][0]["checks"];
    assert_eq!(checks["failure"], json!(["g_insufficient_bound.unwind.0"]));
    let undetermined = checks["undetermined"]
        .as_array()
        .ok_or("undetermined checks")?;
    assert_eq!(undetermined.len(), 31);
    assert!(undetermined.contains(&json!("g_insufficient_bound.assertion.2")));
    Ok(())
}

#[test]
fn captured_fail_fast_partial_does_not_invent_the_missing_harness() -> TestResult {
    let native = parse_native(include_str!(
        "../fixtures/native-captures/partial.native.json"
    ))?;
    assert_eq!(native.run.run_state, NativeRunState::Partial);
    assert_eq!(
        (
            native.run.matched_count,
            native.run.missing_count,
            native.run.harnesses.len()
        ),
        (2, 1, 1)
    );
    let harness = native.run.harnesses.first().ok_or("partial harness")?;
    assert_eq!(harness.name, "g_insufficient_bound");
    assert_eq!(
        harness.outcome,
        NativeOutcome::Completed {
            verdict: NativeVerdict::Failure,
            failure_kind: NativeFailureKind::Other,
        }
    );
    assert_eq!(
        native.document["harness_selection"]["requested_filters"],
        json!(["g_insufficient_bound", "b_success"])
    );
    assert_eq!(native.document["summary"]["successful"], 0);
    Ok(())
}
