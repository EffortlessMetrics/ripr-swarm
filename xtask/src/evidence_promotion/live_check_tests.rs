use super::*;
use crate::tests::{temp_dir, write};

fn subject() -> Value {
    let root = temp_dir("live-check-subject");
    write(
        &root.join("Cargo.toml"),
        "[package]\nname='subject'\nversion='0.0.0'\n",
    );
    write(
        &root.join("diff.patch"),
        "diff --git a/src/lib.rs b/src/lib.rs\n",
    );
    serde_json::json!({"id":"control", "live_check_subject":root,
        "assertions":[{"type":"expected_class","class":"static_unknown"}]})
}

fn selected(case: Value) -> (Vec<LiveCheckCase>, Vec<String>) {
    let mut violations = Vec::new();
    let live = live_check_cases(&serde_json::json!({"cases":[case]}), &mut violations);
    (live, violations)
}

#[test]
fn committed_corpus_selects_a_real_live_subject() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("workspace root")?;
    let mut corpus = read_evidence_promotion_corpus(&root.join(EVIDENCE_PROMOTION_HONESTY_CORPUS))?;
    let cases = corpus
        .get_mut("cases")
        .and_then(Value::as_array_mut)
        .ok_or("cases")?;
    for case in cases {
        for key in ["live_check_subject", "live_check_diff"] {
            if let Some(path) = case.get(key).and_then(Value::as_str) {
                case[key] = serde_json::json!(root.join(path));
            }
        }
    }
    let mut violations = Vec::new();
    let live = live_check_cases(&corpus, &mut violations);
    assert!(!live.is_empty(), "live branch is inert: {violations:?}");
    assert!(violations.is_empty(), "{violations:?}");
    Ok(())
}

#[test]
fn selection_refuses_empty_malformed_missing_diff_and_missing_class() {
    let mut violations = Vec::new();
    assert!(live_check_cases(&serde_json::json!({"cases":[]}), &mut violations).is_empty());
    assert!(!violations.is_empty());
    for mutation in ["subject", "diff", "class", "duplicate_class", "manifest"] {
        let mut case = subject();
        match mutation {
            "subject" => case["live_check_subject"] = serde_json::json!(false),
            "diff" => case["live_check_diff"] = serde_json::json!("missing-live-check-diff.patch"),
            "class" => case["assertions"] = serde_json::json!([]),
            "duplicate_class" => {
                case["assertions"] = serde_json::json!([
                {"type":"expected_class","class":"static_unknown"},
                {"type":"expected_class","class":"exposed"}])
            }
            _ => case["live_check_subject"] = serde_json::json!("missing-live-check-manifest"),
        }
        let (live, violations) = selected(case);
        assert!(live.is_empty(), "{mutation}");
        assert!(!violations.is_empty(), "{mutation}");
    }
}

#[test]
fn selection_uses_the_supplied_snapshot_and_explicit_diff() {
    let mut case = subject();
    let diff = temp_dir("explicit-live-diff").join("selected.patch");
    write(&diff, "diff --git a/src/lib.rs b/src/lib.rs\n");
    case["live_check_diff"] = serde_json::json!(diff);
    let (live, violations) = selected(case);
    assert!(violations.is_empty());
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].diff, diff);
    assert_eq!(live[0].expected, "static_unknown");
}

fn output(stdout: &str, success: bool, timed_out: bool) -> TimedOutput {
    #[cfg(unix)]
    let status = {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(if success { 0 } else { 256 })
    };
    #[cfg(windows)]
    let status = {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(if success { 0 } else { 1 })
    };
    TimedOutput {
        status: Some(status),
        stdout: stdout.to_string(),
        stderr: "diagnostic".to_string(),
        duration: Duration::ZERO,
        timed_out,
    }
}

#[test]
fn comparison_refuses_process_and_report_failures_even_with_a_matching_class() {
    let case = LiveCheckCase {
        id: "control".to_string(),
        subject: PathBuf::new(),
        diff: PathBuf::new(),
        expected: "static_unknown".to_string(),
    };
    let good = r#"{"findings":[{"classification":"static_unknown"}]}"#;
    assert!(live_check_problem(&case, Ok(output(good, true, false))).is_none());
    for result in [
        Err("spawn failed".to_string()),
        Ok(output(good, true, true)),
        Ok(output(good, false, false)),
        Ok(output("not json", true, false)),
        Ok(output("{}", true, false)),
        Ok(output(r#"{"findings":[]}"#, true, false)),
        Ok(output(r#"{"findings":[{},{}]}"#, true, false)),
        Ok(output(
            r#"{"findings":[{"classification":"exposed"}]}"#,
            true,
            false,
        )),
        Ok(output(r#"{"findings":[{}]}"#, true, false)),
        Ok(output(
            r#"{"error":{},"findings":[{"classification":"static_unknown"}]}"#,
            true,
            false,
        )),
        Ok(output(
            r#"{"analysis_scope":{"downstream_consumable":false},"findings":[{"classification":"static_unknown"}]}"#,
            true,
            false,
        )),
        Ok(output(
            r#"{"findings":[],"findings":[{"classification":"static_unknown"}]}"#,
            true,
            false,
        )),
    ] {
        assert!(live_check_problem(&case, result).is_some());
    }
    let mut absent_status = output(good, true, false);
    absent_status.status = None;
    assert!(live_check_problem(&case, Ok(absent_status)).is_some());
}

/// Opt-in producer proof, separate from the synthetic-output unit cases.
#[test]
#[ignore = "builds and runs the real ripr producer three times"]
fn live_producer_accepts_original_rejects_changed_pin_and_accepts_restored() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("workspace root")?;
    let corpus = read_evidence_promotion_corpus(&root.join(EVIDENCE_PROMOTION_HONESTY_CORPUS))?;
    let mut case = corpus
        .get("cases")
        .and_then(Value::as_array)
        .and_then(|cases| {
            cases
                .iter()
                .find(|case| case.get("live_check_subject").is_some())
        })
        .cloned()
        .ok_or("live subject missing")?;
    for key in ["live_check_subject", "live_check_diff"] {
        if let Some(path) = case.get(key).and_then(Value::as_str) {
            case[key] = serde_json::json!(root.join(path));
        }
    }
    let original = serde_json::json!({"cases":[case]});
    let mut violations = Vec::new();
    check_live_check_cases(&original, &mut violations)?;
    assert!(violations.is_empty(), "original: {violations:?}");
    let mut changed = original.clone();
    let assertions = changed["cases"][0]["assertions"]
        .as_array_mut()
        .ok_or("assertions")?;
    let class = assertions
        .iter_mut()
        .find(|a| a["type"] == "expected_class")
        .ok_or("class pin")?;
    class["class"] = serde_json::json!("exposed");
    check_live_check_cases(&changed, &mut violations)?;
    assert_eq!(violations.len(), 1, "negative: {violations:?}");
    assert!(
        violations[0].contains("classifies the finding as `static_unknown`"),
        "{violations:?}"
    );
    println!("real-producer negative control rejected: {}", violations[0]);
    violations.clear();
    check_live_check_cases(&original, &mut violations)?;
    assert!(violations.is_empty(), "restored: {violations:?}");
    Ok(())
}
