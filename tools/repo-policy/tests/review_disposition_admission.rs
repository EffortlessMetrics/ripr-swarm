//! #8157: shared source-review disposition verifier used by merge-ready admission.
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const SCRIPT: &str = ".github/actions/pr-candidate-admission/review_disposition.py";
const REFUSED: &str = "source review attestation refused";

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
        .arg("-I")
        .arg(script()?)
        .args(["--check-reviews", "-", "--head", head])
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

/// Admit means exit 0; refuse means exit 1 with the refusal line and no crash.
fn expect_outcome(output: &Output, admit: bool, what: &str) -> Result<(), String> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let ok = if admit {
        output.status.code() == Some(0)
    } else {
        output.status.code() == Some(1) && stderr.contains(REFUSED) && !stderr.contains("Traceback")
    };
    if ok {
        Ok(())
    } else {
        Err(format!(
            "{what}: expected {} got {:?} stderr={stderr}",
            if admit { "admit" } else { "refusal" },
            output.status.code()
        ))
    }
}

fn review(login: &str, state: &str, head: &str, at: &str, body: &str) -> serde_json::Value {
    serde_json::json!({
        "id": 1,
        "user": {"login": login},
        "author_association": "MEMBER",
        "state": state,
        "commit_id": head,
        "submitted_at": at,
        "body": body,
    })
}

#[test]
fn review_disposition_self_test_passes() -> Result<(), String> {
    let output = Command::new("python3")
        .arg("-I")
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
fn issue_8157_narrative_false_positive_controls() -> Result<(), String> {
    let head = "a".repeat(40);
    let cases: [(&str, bool); 37] = [
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
        // Review-of-record regressions on the first head of this PR.
        ("REVIEW_READY was withdrawn", false),
        ("REVIEW_READY: no", false),
        ("SOURCE_REVIEW_READY, but repairs required", false),
        ("REVIEW_READY\nREPAIR_REQUIRED", false),
        ("REVIEW_READY\nREVIEW_INCOMPLETE", false),
        ("REVIEW_READY\n  disposition: REPAIR_REQUIRED", false),
        ("REVIEW_READY\n- disposition: REPAIR_REQUIRED", false),
        ("REVIEW_READY\n> disposition: REPAIR_REQUIRED", false),
        ("REVIEW_READY\n**disposition: REPAIR_REQUIRED**", false),
        ("REVIEW_READY\n```\ndisposition: REPAIR_REQUIRED", false),
        ("<!--\ndisposition: REVIEW_READY\n-->", false),
        ("<!-- unclosed\nREVIEW_READY", false),
        ("REVIEW_READY\n\nChecked the diff and the oracle.", true),
        // Deferred in #12814 and fixed in the follow-up: negating prose after the marker.
        ("REVIEW_READY\nnot ready; repairs are required", false),
        ("REVIEW_READY\nThis is no longer ready", false),
        ("REVIEW_READY\nRepairs are required before merge", false),
        // The real first-line shape of merged review-of-record comments.
        (
            "REVIEW_READY \u{2014} head 7c50f64fa93c3e40533240fbe4fd0e9ac7398eab",
            true,
        ),
        (
            "REVIEW_READY \u{2014} head 7c50f64fa\n\nAngles: correctness. Findings: nothing blocking; hosted CI not run; fail-closed on missing data.",
            true,
        ),
        ("REVIEW_READY - head 7c50f64\nNo findings.", true),
        ("REVIEW_READY \u{2014} head xyz", true),
        // Trimmed real first lines and summary lines from merged reviews of record.
        (
            "REVIEW_READY for the current candidate.\n\ndisposition: REVIEW_READY, subject to final unchanged-head refresh.",
            true,
        ),
        (
            "REVIEW_READY \u{2014} native qualification and integration review complete.\ndisposition: REVIEW_READY for the installed candidate.",
            true,
        ),
        (
            "REVIEW_READY \u{2014} exact-head source, native-proof, current-findings and integration review complete.",
            true,
        ),
        (
            "REVIEW_READY \u{2014} head b9a71b72\n**The blocker is fixed.** The case now adds `unsafe { x.unreachable_unchecked() }`.",
            true,
        ),
        ("REVIEW_READY \u{2014} not ready", false),
        ("REVIEW_READY \u{2014} REPAIR_REQUIRED", false),
        (
            "REVIEW_READY for the candidate, but repairs required",
            false,
        ),
        (
            "REVIEW_READY\n```\nnot ready\n```\nChecked the oracle.",
            true,
        ),
        ("", false),
    ];
    for (body, expected) in cases {
        let reviews = serde_json::json!([review(
            "author",
            "COMMENTED",
            &head,
            "2026-10-15T01:00:01Z",
            body
        )]);
        expect_outcome(
            &check_reviews(&reviews, &head)?,
            expected,
            &format!("{body:?}"),
        )?;
    }
    Ok(())
}

#[test]
fn wrong_head_changes_requested_and_identity_controls() -> Result<(), String> {
    let head = "a".repeat(40);
    let other = "b".repeat(40);
    let at = "2026-10-15T01:00:01Z";
    let mut untrusted = review("r", "COMMENTED", &head, at, "SOURCE_REVIEW_READY");
    untrusted["author_association"] = serde_json::json!("NONE");
    let mut untrusted_approval = review("r", "APPROVED", &head, at, "");
    untrusted_approval["author_association"] = serde_json::json!("CONTRIBUTOR");
    let mut deleted_user_block = review("x", "CHANGES_REQUESTED", &head, at, "");
    deleted_user_block["user"] = serde_json::Value::Null;
    deleted_user_block["id"] = serde_json::json!(2);
    let mut other_negative = review(
        "other",
        "COMMENTED",
        &head,
        "2026-10-15T01:00:02Z",
        "REPAIR_REQUIRED",
    );
    other_negative["id"] = serde_json::json!(2);
    let mut chatter = review(
        "other",
        "COMMENTED",
        &head,
        "2026-10-15T01:00:02Z",
        "Looks fine to me",
    );
    chatter["id"] = serde_json::json!(2);
    let ready = review("r", "COMMENTED", &head, at, "REVIEW_READY");
    let mut no_time = review("r", "APPROVED", &head, at, "");
    no_time["submitted_at"] = serde_json::Value::Null;
    let mut negative_no_time = review("other", "COMMENTED", &head, at, "REPAIR_REQUIRED");
    negative_no_time["submitted_at"] = serde_json::Value::Null;
    negative_no_time["id"] = serde_json::json!(3);
    let controls = [
        (
            "another reviewer's negative comment",
            serde_json::json!([ready.clone(), other_negative]),
            false,
        ),
        (
            "another reviewer's neutral comment",
            serde_json::json!([ready, chatter]),
            true,
        ),
        (
            "negative comment without submitted_at",
            serde_json::json!([
                review("r", "COMMENTED", &head, at, "REVIEW_READY"),
                negative_no_time
            ]),
            false,
        ),
        (
            "decisive review without submitted_at",
            serde_json::json!([no_time]),
            false,
        ),
        (
            "changes requested from a deleted account",
            serde_json::json!([review("r", "APPROVED", &head, at, ""), deleted_user_block]),
            false,
        ),
        (
            "wrong head",
            serde_json::json!([review("r", "COMMENTED", &other, at, "SOURCE_REVIEW_READY")]),
            false,
        ),
        (
            "changes requested",
            serde_json::json!([
                review("r", "CHANGES_REQUESTED", &head, at, ""),
                review(
                    "r2",
                    "COMMENTED",
                    &head,
                    "2026-10-15T01:00:02Z",
                    "SOURCE_REVIEW_READY"
                ),
            ]),
            false,
        ),
        (
            "approved on head",
            serde_json::json!([review("r", "APPROVED", &head, at, "")]),
            true,
        ),
        ("untrusted comment", serde_json::json!([untrusted]), false),
        (
            "untrusted approval",
            serde_json::json!([untrusted_approval]),
            false,
        ),
    ];
    for (name, reviews, expected) in controls {
        expect_outcome(&check_reviews(&reviews, &head)?, expected, name)?;
    }
    Ok(())
}

#[test]
fn malformed_review_pages_refuse_instead_of_crashing() -> Result<(), String> {
    let head = "a".repeat(40);
    for page in [
        serde_json::json!({"message": "Bad credentials"}),
        serde_json::json!([1]),
    ] {
        expect_outcome(&check_reviews(&page, &head)?, false, &format!("{page}"))?;
    }
    Ok(())
}

/// The `--repository/--pr` path reads every page through `gh` and fails closed on error.
#[cfg(unix)]
#[test]
fn live_read_merges_pages_and_fails_closed_on_gh_error() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("review-disposition-gh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
    let head = "a".repeat(40);
    let page1 = serde_json::json!([review(
        "r",
        "CHANGES_REQUESTED",
        &head,
        "2026-10-15T01:00:01Z",
        ""
    )]);
    let page2 = serde_json::json!([review("q", "APPROVED", &head, "2026-10-15T01:00:02Z", "")]);
    let run = |gh_body: String| -> Result<Output, String> {
        let gh = dir.join("gh");
        std::fs::write(&gh, gh_body).map_err(|e| format!("write gh: {e}"))?;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("chmod gh: {e}"))?;
        let path = format!(
            "{}:{}",
            dir.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new("python3")
            .arg("-I")
            .arg(script()?)
            .args(["--repository", "o/r", "--pr", "1", "--head", &head])
            .env("PATH", path)
            .output()
            .map_err(|e| format!("spawn verifier: {e}"))
    };
    // A CHANGES_REQUESTED on page one must still refuse when page two approves.
    let two_pages = format!("#!/bin/sh\nprintf '%s\\n%s\\n' '{page1}' '{page2}'\n");
    expect_outcome(&run(two_pages)?, false, "changes requested on page one")?;
    // An API error after a valid page must not admit.
    let failing = format!("#!/bin/sh\nprintf '%s\\n' '{page2}'\nexit 1\n");
    let output = run(failing);
    let result = output.and_then(|o| expect_outcome(&o, false, "gh failure after a valid page"));
    let _ = std::fs::remove_dir_all(&dir);
    result
}
