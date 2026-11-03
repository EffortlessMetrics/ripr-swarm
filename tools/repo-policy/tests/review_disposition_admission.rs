//! #8157: shared source-review disposition verifier used by merge-ready admission.
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const SCRIPT: &str = ".github/actions/pr-candidate-admission/review_disposition.py";

fn script() -> Result<PathBuf, String> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .and_then(|p| p.parent())
        .ok_or("repository root not found")?;
    Ok(root.join(SCRIPT))
}

fn check_reviews(reviews: &serde_json::Value, head: &str) -> Result<Output, String> {
    let mut child = Command::new("python3")
        .arg(script()?)
        .args(["--check-reviews", "/dev/stdin", "--head", head])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn python3: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("stdin not piped")?
        .write_all(reviews.to_string().as_bytes())
        .map_err(|e| format!("write reviews: {e}"))?;
    child
        .wait_with_output()
        .map_err(|e| format!("wait for verifier: {e}"))
}

fn review(login: &str, state: &str, head: &str, at: &str, body: &str) -> serde_json::Value {
    serde_json::json!({
        "user": {"login": login},
        "state": state,
        "commit_id": head,
        "submitted_at": at,
        "body": body,
    })
}

#[test]
fn review_disposition_self_test_passes() -> Result<(), String> {
    let output = Command::new("python3")
        .arg(script()?)
        .arg("--self-test")
        .output()
        .map_err(|e| format!("python3 must be available for admission policy tests: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "review disposition self-test failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[test]
fn issue_8157_literal_controls_refuse_narrative_false_positives() -> Result<(), String> {
    let head = "a".repeat(40);
    let cases: [(&str, bool); 8] = [
        ("SOURCE_REVIEW_READY", true),
        ("REVIEW_INCOMPLETE", false),
        (
            "REVIEW_INCOMPLETE. This candidate is not SOURCE_REVIEW_READY; behavioral proof is missing.",
            false,
        ),
        ("REVIEW_INCOMPLETE. Example:\n```\nREVIEW_READY\n```", false),
        (
            "Previous result was REVIEW_READY; current disposition: REPAIR_REQUIRED.",
            false,
        ),
        ("No longer SOURCE_REVIEW_READY. REPAIR_REQUIRED.", false),
        ("disposition: SOURCE_REVIEW_READY", true),
        (
            "disposition: REVIEW_READY\ndisposition: REPAIR_REQUIRED",
            false,
        ),
    ];
    for (body, expected) in cases {
        let reviews = serde_json::json!([review(
            "author",
            "COMMENTED",
            &head,
            "2026-10-15T01:00:01Z",
            body
        )]);
        let output = check_reviews(&reviews, &head)?;
        if output.status.success() != expected {
            return Err(format!(
                "body={body:?} expected={expected} stderr={}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    Ok(())
}

#[test]
fn wrong_head_and_changes_requested_controls_keep_their_results() -> Result<(), String> {
    let head = "a".repeat(40);
    let other = "b".repeat(40);
    let wrong_head = serde_json::json!([review(
        "r",
        "COMMENTED",
        &other,
        "2026-10-15T01:00:01Z",
        "SOURCE_REVIEW_READY"
    )]);
    let blocked = serde_json::json!([
        review("r", "CHANGES_REQUESTED", &head, "2026-10-15T01:00:01Z", ""),
        review(
            "r2",
            "COMMENTED",
            &head,
            "2026-10-15T01:00:02Z",
            "SOURCE_REVIEW_READY"
        ),
    ]);
    let approved = serde_json::json!([review("r", "APPROVED", &head, "2026-10-15T01:00:01Z", "")]);
    for (name, reviews, expected) in [
        ("wrong head", wrong_head, false),
        ("changes requested", blocked, false),
        ("approved on head", approved, true),
    ] {
        let output = check_reviews(&reviews, &head)?;
        if output.status.success() != expected {
            return Err(format!(
                "{name}: expected={expected} stderr={}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    Ok(())
}
