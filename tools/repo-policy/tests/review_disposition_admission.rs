//! #8157: shared source-review disposition verifier used by merge-ready admission.
use std::process::Command;

const SCRIPT: &str = ".github/actions/pr-candidate-admission/review_disposition.py";

#[test]
fn review_disposition_self_test_passes() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("repository root");
    let script = root.join(SCRIPT);
    let output = Command::new("python3")
        .arg(script)
        .arg("--self-test")
        .output()
        .expect("python3 must be available for admission policy tests");
    assert!(
        output.status.success(),
        "review disposition self-test failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn issue_8157_literal_controls_refuse_narrative_false_positives() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("repository root");
    let script = root.join(SCRIPT);
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
        let reviews = serde_json::json!([{
            "user": {"login": "author"},
            "state": "COMMENTED",
            "commit_id": head,
            "submitted_at": "2026-10-15T01:00:01Z",
            "body": body,
        }]);
        let mut child = Command::new("python3")
            .arg(&script)
            .arg("--check-reviews")
            .arg("/dev/stdin")
            .arg("--head")
            .arg(&head)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn review disposition check");
        use std::io::Write;
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(reviews.to_string().as_bytes())
            .expect("write reviews");
        let output = child.wait_with_output().expect("wait");
        assert_eq!(
            output.status.success(),
            expected,
            "body={body:?} stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
