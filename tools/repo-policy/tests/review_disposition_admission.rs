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
    check_review_bytes(reviews.to_string().as_bytes(), head, false)
}

fn check_review_bytes(bytes: &[u8], head: &str, legacy_stdin: bool) -> Result<Output, String> {
    check_review_bytes_with_pull(bytes, head, legacy_stdin, None)
}

/// #4984: reviews on stdin plus the pull request object read from `pull`.
fn check_reviews_with_pull(
    reviews: &serde_json::Value,
    pull: &serde_json::Value,
    head: &str,
    tag: &str,
) -> Result<Output, String> {
    let path = std::env::temp_dir().join(format!(
        "review-disposition-pull-{}-{tag}.json",
        std::process::id()
    ));
    std::fs::write(&path, pull.to_string()).map_err(|e| format!("write pull: {e}"))?;
    let output =
        check_review_bytes_with_pull(reviews.to_string().as_bytes(), head, false, Some(&path));
    let _ = std::fs::remove_file(&path);
    output
}

fn check_review_bytes_with_pull(
    bytes: &[u8],
    head: &str,
    legacy_stdin: bool,
    pull: Option<&std::path::Path>,
) -> Result<Output, String> {
    let mut command = Command::new("python3");
    command.arg("-I");
    if legacy_stdin {
        // Force the Windows legacy decoding boundary even on UTF-8 hosts.
        command.args(["-c", "import runpy,sys; sys.stdin.reconfigure(encoding='cp1252'); path=sys.argv.pop(1); runpy.run_path(path, run_name='__main__')"]);
    }
    command
        .arg(script()?)
        .args(["--check-reviews", "-", "--head", head]);
    if let Some(pull) = pull {
        child_args_pull(&mut command, pull);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn python3: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("stdin not piped")?
        .write_all(bytes)
        .map_err(|e| format!("write reviews: {e}"))?;
    child
        .wait_with_output()
        .map_err(|e| format!("wait for verifier: {e}"))
}

#[test]
fn utf8_review_input_ignores_legacy_stdin_encoding() -> Result<(), String> {
    let head = "a".repeat(40);
    let reviews = serde_json::json!([review(
        "author",
        "COMMENTED",
        &head,
        "2026-10-15T01:00:01Z",
        "REVIEW_READY \u{2014} head 7c50f64fa93c3e40533240fbe4fd0e9ac7398eab"
    )]);
    expect_outcome(
        &check_review_bytes(reviews.to_string().as_bytes(), &head, true)?,
        true,
        "UTF-8 review through legacy text stdin",
    )?;
    expect_outcome(
        &check_review_bytes(b"[\"\xff\"]", &head, true)?,
        false,
        "malformed UTF-8 review",
    )
}

fn child_args_pull(command: &mut Command, pull: &std::path::Path) {
    command.arg("--check-pull").arg(pull);
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
    let cases: [(&str, bool); 91] = [
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
        ("REVIEW_READY \u{2014} head xyz", false),
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
        ("REVIEW_READY \u{2014} pending fix", false),
        ("REVIEW_READY, blocked on CI", false),
        ("REVIEW_READY \u{2014} blocking issue found", false),
        ("REVIEW_READY \u{2014} do not merge", false),
        ("REVIEW_READY \u{2014} changes requested", false),
        ("REVIEW_READY \u{2014} failing tests remain", false),
        ("REVIEW_READY \u{2014} stale", false),
        ("REVIEW_READY, if CI passes", false),
        ("REVIEW_READY \u{2014} only after fix", false),
        ("REVIEW_READY \u{2014} awaiting CI", false),
        ("REVIEW_READY\nDo not merge", false),
        ("REVIEW_READY\nI take it back", false),
        ("REVIEW_READY\nThe change is blocked on CI", false),
        ("REVIEW_READY \u{2014} no findings", false),
        (
            "REVIEW_READY for the candidate, but repairs required",
            false,
        ),
        (
            "REVIEW_READY\n```\nnot ready\n```\nChecked the oracle.",
            false,
        ),
        ("REVIEW_READY\n~~~\nRetracting this review\n~~~", false),
        ("REVIEW_READY \u{2014} needs a fix", false),
        ("REVIEW_READY \u{2014} cannot merge", false),
        ("REVIEW_READY \u{2014} CI red", false),
        ("REVIEW_READY \u{2014} wait for CI", false),
        ("REVIEW_READY \u{2014} unverified", false),
        ("REVIEW_READY \u{2014} WIP", false),
        ("REVIEW_READY \u{2014} contingent on CI", false),
        ("REVIEW_READY \u{2014} subject to change", false),
        ("REVIEW_READY\nDont merge", false),
        ("REVIEW_READY\nDon\u{2019}t merge", false),
        ("REVIEW_READY\nWait", false),
        ("REVIEW_READY\nRetracting", false),
        ("REVIEW_READY\nBlocker found", false),
        ("REVIEW_READY\nTests fail", false),
        ("REVIEW_READY\nNeeds work", false),
        ("REVIEW_READY\nBlocked.", false),
        ("REVIEW_READY\nStale", false),
        ("REVIEW_READY\nI do not approve", false),
        ("REVIEW_READY \u{2014} not-ready change", false),
        ("REVIEW_READY \u{2014} on-hold change", false),
        ("REVIEW_READY \u{2014} do-not change", false),
        ("REVIEW_READY \u{2014} pending-fix change", false),
        ("REVIEW_READY \u{2014} \u{4fdd}\u{7559}", false),
        ("REVIEW_READY \u{2014} \u{043d}\u{043e}", false),
        ("REVIEW_READY \u{2014} \u{274c}", false),
        ("REVIEW_READY \u{2014} ?", false),
        ("REVIEW_READY \u{2014} merge after final review", false),
        ("REVIEW_READY \u{2014} then merge", false),
        ("REVIEW_READY \u{2014} admin bypass", false),
        ("REVIEW_READY \u{2014} bypass the gate", false),
        ("REVIEW_READY \u{2014} deadbeef1", false),
        ("REVIEW_READY\nDo **not** merge", false),
        ("REVIEW_READY\nDo\u{a0}not merge", false),
        ("REVIEW_READY\nNever merge", false),
        ("REVIEW_READY\nWon't merge", false),
        ("REVIEW_READY\nReject", false),
        ("REVIEW_READY\nStatus: blocked", false),
        ("REVIEW_READY\nreview incomplete", false),
        ("REVIEW_READY\nDo&nbsp;not merge", false),
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

/// #14355: a negated change/repair phrase is resolved work, not a blocker.
/// The witness is the #14117 body; the controls are the genuine blocker forms.
#[test]
fn issue_14355_negated_change_phrase_is_not_a_blocker() -> Result<(), String> {
    let head = "a".repeat(40);
    let at = "2026-11-11T11:00:28Z";
    let cases: [(&str, bool); 67] = [
        // The strip-exercising control: this admits only because the strip ran.
        ("REVIEW_READY\nResolved with no code change needed.", true),
        (
            "REVIEW_READY\nThree Devin threads resolved, no code changes needed.",
            true,
        ),
        // Control that does not exercise the strip: no "needed"/"required".
        ("REVIEW_READY\nResolved without a code change.", true),
        // A comma resets the clause, so a benign cue earlier in the sentence
        // does not suppress the strip.
        (
            "REVIEW_READY\nThe Devin thread was a false positive, no code change needed.",
            true,
        ),
        (
            "REVIEW_READY\nThe finding is not applicable, no changes needed.",
            true,
        ),
        (
            "REVIEW_READY\nGemini isn't available, no changes needed.",
            true,
        ),
        // A hyphenated build/ci idiom is not a blocker.
        (
            "REVIEW_READY\nHosted CI fail-closed on this change; no code change needed.",
            true,
        ),
        (
            "REVIEW_READY\nBuild fail-fast behaviour verified; no changes needed.",
            true,
        ),
        ("REVIEW_READY\nCode change needed.", false),
        ("REVIEW_READY\nA change is needed.", false),
        ("REVIEW_READY\nChanges required.", false),
        ("REVIEW_READY\nRepairs required.", false),
        (
            "REVIEW_READY\nNo code change needed, but repairs required.",
            false,
        ),
        // The real #14117 line 6: mid-line phrase followed by other sentences.
        (
            "REVIEW_READY\n- Review input: three Devin threads, each answered in the PR \
             conversation and resolved with no code change needed. Gemini had no findings. \
             CodeRabbit was rate limited and ub-review was cancelled, so neither supplied a \
             review; the owning session's inline source review stands in for them.",
            true,
        ),
        (
            "REVIEW_READY\nNo code change needed. All threads resolved.",
            true,
        ),
        (
            "REVIEW_READY\nNo code change needed. Gemini had no findings.",
            true,
        ),
        (
            "REVIEW_READY\nNo changes are needed. The build is red.",
            false,
        ),
        ("REVIEW_READY\nNo code change needed. Tests fail.", false),
        (
            "REVIEW_READY\nNo code change needed. Repairs required.",
            false,
        ),
        (
            "REVIEW_READY\nNo changes are needed. The build fails.",
            false,
        ),
        (
            "REVIEW_READY\nNo changes are needed. The build is broken.",
            false,
        ),
        (
            "REVIEW_READY\nNo changes are needed. The build is failing.",
            false,
        ),
        (
            "REVIEW_READY\nNo code change needed. A fix is needed.",
            false,
        ),
        // A refutation after the phrase, in the same sentence, refuses.
        ("REVIEW_READY\nNo changes are needed. That is false.", false),
        ("REVIEW_READY\nNo changes are needed; that is false.", false),
        (
            "REVIEW_READY\nNo changes are needed. That is not true.",
            false,
        ),
        ("REVIEW_READY\nNo changes are needed. I deny it.", false),
        ("REVIEW_READY\nNo changes are needed. That is a lie.", false),
        ("REVIEW_READY\nNo changes are needed. Incorrect.", false),
        ("REVIEW_READY\nNo changes are needed. Wrong.", false),
        ("REVIEW_READY\nNo code change needed. Not so.", false),
        ("REVIEW_READY\nNo changes are needed. Not true.", false),
        (
            "REVIEW_READY\nNo changes are needed. That isn't true.",
            false,
        ),
        (
            "REVIEW_READY\nNo changes are needed. This isn't correct.",
            false,
        ),
        (
            "REVIEW_READY\nNo changes are needed. That claim is false.",
            false,
        ),
        (
            "REVIEW_READY\nNo changes are needed. The above is false.",
            false,
        ),
        (
            "REVIEW_READY\nNo changes are needed. That assertion is false.",
            false,
        ),
        (
            "REVIEW_READY\nNo changes are needed. This claim is false.",
            false,
        ),
        (
            "REVIEW_READY\nNo changes are needed. The statement is false.",
            false,
        ),
        (
            "REVIEW_READY\nNo changes are needed. The claim is false.",
            false,
        ),
        (
            "REVIEW_READY\nNo changes are needed. The assertion is false.",
            false,
        ),
        (
            "REVIEW_READY\nNo changes are needed. The finding is false.",
            false,
        ),
        (
            "REVIEW_READY\nNo changes are needed. The report is false.",
            false,
        ),
        (
            "REVIEW_READY\nNo changes are needed. This statement is untrue.",
            false,
        ),
        ("REVIEW_READY\nNo changes are needed. All green.", true),
        // Ordinary prose with a cue word is not a refutation.
        (
            "REVIEW_READY\nNo changes are needed. Nothing is wrong.",
            true,
        ),
        (
            "REVIEW_READY\nNo changes are needed. No incorrect claims.",
            true,
        ),
        (
            "REVIEW_READY\nNo code change needed. The wrong file was not touched.",
            true,
        ),
        (
            "REVIEW_READY\nNo changes are needed. All good, false alarm earlier.",
            true,
        ),
        // An unlisted refutation ahead of the phrase is not resolved work.
        (
            "REVIEW_READY\nIt's not accurate that no changes are needed.",
            false,
        ),
        (
            "REVIEW_READY\nIt's not correct that no changes are needed.",
            false,
        ),
        (
            "REVIEW_READY\nIt would be wrong to claim no changes are needed.",
            false,
        ),
        (
            "REVIEW_READY\nI disagree that no changes are needed.",
            false,
        ),
        (
            "REVIEW_READY\nI don't believe no changes are needed.",
            false,
        ),
        ("REVIEW_READY\nI doubt no changes are needed.", false),
        // Graphite thread examples.
        (
            "REVIEW_READY\nNo code change needed. The Devin thread was a false positive.",
            true,
        ),
        (
            "REVIEW_READY\nNo code change needed. Gemini isn't available.",
            true,
        ),
        // A refuted phrase is not resolved work.
        (
            "REVIEW_READY\nIt is not true that no changes are needed.",
            false,
        ),
        ("REVIEW_READY\nIt is false that no change is needed.", false),
        ("REVIEW_READY\nNo changes are needed is false.", false),
        ("REVIEW_READY\nNo code change needed is not correct.", false),
        (
            "REVIEW_READY\nNo changes are needed -- the build is red.",
            false,
        ),
        (
            "REVIEW_READY\nThere is no truth to the claim that no changes are needed.",
            false,
        ),
        (
            "REVIEW_READY\nIt is a lie that no changes are needed.",
            false,
        ),
        (
            "REVIEW_READY\nIt is wrong to say that no changes are needed.",
            false,
        ),
        (
            "REVIEW_READY\nNobody could say that no changes are needed.",
            false,
        ),
        ("REVIEW_READY\nno findings, code change needed", false),
    ];
    for (body, admit) in cases {
        let reviews = serde_json::json!([review("author", "COMMENTED", &head, at, body)]);
        expect_outcome(&check_reviews(&reviews, &head)?, admit, body)?;
    }
    Ok(())
}

/// Full, untrimmed bodies of real review-of-record comments: every `ready-*`
/// body must admit and every `refuse-*` body (REVIEW_INCOMPLETE shapes) must refuse.
#[test]
fn full_real_review_bodies_admit_or_refuse_as_recorded() -> Result<(), String> {
    let head = "a".repeat(40);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/review_bodies");
    let mut ready = 0;
    let mut refused = 0;
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("non-utf8 fixture name")?
            .to_string();
        let body = std::fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
        let admit = name.starts_with("ready-");
        if admit {
            ready += 1;
        } else {
            refused += 1;
        }
        let reviews = serde_json::json!([review(
            "author",
            "COMMENTED",
            &head,
            "2026-10-15T01:00:01Z",
            &body
        )]);
        expect_outcome(&check_reviews(&reviews, &head)?, admit, &name)?;
    }
    if ready != 12 || refused != 7 {
        return Err(format!(
            "expected 12 ready and 7 refuse fixtures, saw {ready}/{refused}"
        ));
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
    let result = output
        .and_then(|o| expect_outcome(&o, false, "gh failure after a valid page"))
        .and_then(|()| {
            // #4984: with no review at all, the live read of the pull request
            // description supplies the exact-head attestation.
            let pull = pull_object(
                "MEMBER",
                "o/r",
                &head,
                &format!("Source review: REVIEW_READY {head}"),
            );
            let described = format!(
                "#!/bin/sh\ncase \"$*\" in *reviews*) printf '[]\\n' ;; *) printf '%s\\n' '{pull}' ;; esac\n"
            );
            expect_outcome(&run(described)?, true, "live description attestation")
        })
        .and_then(|()| {
            // A failed description read leaves only the (empty) reviews: refuse.
            let unreadable =
                "#!/bin/sh\ncase \"$*\" in *reviews*) printf '[]\\n' ;; *) exit 1 ;; esac\n"
                    .to_string();
            expect_outcome(&run(unreadable)?, false, "description read failure")
        });
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn pull_object(
    association: &str,
    head_repo: &str,
    head_sha: &str,
    body: &str,
) -> serde_json::Value {
    serde_json::json!({
        "author_association": association,
        "head": {"sha": head_sha, "repo": {"full_name": head_repo}},
        "base": {"repo": {"full_name": "o/r"}},
        "body": body,
    })
}

/// #4984: the pull request description may carry the exact-head attestation when
/// review writes are throttled. It is a carrier only; every review refusal stands.
#[test]
fn pull_description_carrier_binds_the_exact_head_and_adds_no_authority() -> Result<(), String> {
    let head = "a".repeat(40);
    let other = "b".repeat(40);
    let line = format!("Source review: REVIEW_READY {head}");
    let none = serde_json::json!([]);
    let negative = serde_json::json!([review(
        "r",
        "COMMENTED",
        &head,
        "2026-10-15T01:00:01Z",
        "REPAIR_REQUIRED"
    )]);
    let changes = serde_json::json!([review(
        "r",
        "CHANGES_REQUESTED",
        &head,
        "2026-10-15T01:00:01Z",
        ""
    )]);
    let empty_reply =
        serde_json::json!([review("r", "COMMENTED", &head, "2026-10-15T01:00:01Z", "")]);
    let stale_ready = serde_json::json!([review(
        "r",
        "COMMENTED",
        &other,
        "2026-10-15T01:00:01Z",
        "REVIEW_READY"
    )]);
    let embedded = format!("## Summary\n\n- one change\n\n{line}\n\n## Proof\n");
    let abbreviated = format!("Source review: REVIEW_READY {}", &head[..12]);
    let other_head = format!("Source review: REVIEW_READY {other}");
    let fenced = format!("```\n{line}\n```");
    let commented = format!("<!-- {line} -->");
    let contradicted = format!("{line}\nSource review: REVIEW_INCOMPLETE");
    let hedged = format!("{line} pending CI");
    let quoted = format!("> {line}");
    let cases: [(&str, &serde_json::Value, serde_json::Value, bool); 17] = [
        (
            "exact line",
            &none,
            pull_object("MEMBER", "o/r", &head, &line),
            true,
        ),
        (
            "line inside a filled template",
            &none,
            pull_object("OWNER", "o/r", &head, &embedded),
            true,
        ),
        (
            "empty reply review does not hide it",
            &empty_reply,
            pull_object("COLLABORATOR", "o/r", &head, &line),
            true,
        ),
        (
            "unfilled template",
            &none,
            pull_object("MEMBER", "o/r", &head, "Source review:"),
            false,
        ),
        (
            "no line",
            &none,
            pull_object("MEMBER", "o/r", &head, "## Summary"),
            false,
        ),
        (
            "abbreviated sha",
            &none,
            pull_object("MEMBER", "o/r", &head, &abbreviated),
            false,
        ),
        (
            "line names another head",
            &none,
            pull_object("MEMBER", "o/r", &head, &other_head),
            false,
        ),
        (
            "live head moved",
            &none,
            pull_object("MEMBER", "o/r", &other, &line),
            false,
        ),
        (
            "fenced example",
            &none,
            pull_object("MEMBER", "o/r", &head, &fenced),
            false,
        ),
        (
            "html comment",
            &none,
            pull_object("MEMBER", "o/r", &head, &commented),
            false,
        ),
        (
            "contradicting second line",
            &none,
            pull_object("MEMBER", "o/r", &head, &contradicted),
            false,
        ),
        (
            "hedged value",
            &none,
            pull_object("MEMBER", "o/r", &head, &hedged),
            false,
        ),
        (
            "quoted line",
            &none,
            pull_object("MEMBER", "o/r", &head, &quoted),
            false,
        ),
        (
            "untrusted author",
            &none,
            pull_object("CONTRIBUTOR", "o/r", &head, &line),
            false,
        ),
        (
            "fork head",
            &none,
            pull_object("MEMBER", "fork/r", &head, &line),
            false,
        ),
        (
            "trusted negative review still refuses",
            &negative,
            pull_object("MEMBER", "o/r", &head, &line),
            false,
        ),
        (
            "changes requested still refuses",
            &changes,
            pull_object("MEMBER", "o/r", &head, &line),
            false,
        ),
    ];
    for (index, (name, reviews, pull, admit)) in cases.iter().enumerate() {
        expect_outcome(
            &check_reviews_with_pull(reviews, pull, &head, &index.to_string())?,
            *admit,
            name,
        )?;
    }
    // #11566: a competing field inside the description itself refuses whatever list
    // markup leads it, with no reviews at all; prose and a clean line still admit.
    for (index, (name, extra, admit)) in [
        (
            "numbered competing field",
            "1. disposition: REPAIR_REQUIRED",
            false,
        ),
        (
            "paren-numbered competing field",
            "1) disposition: REPAIR_REQUIRED",
            false,
        ),
        (
            "task-list competing field",
            "- [ ] disposition: REPAIR_REQUIRED",
            false,
        ),
        (
            "bulleted competing field",
            "- disposition: REPAIR_REQUIRED",
            false,
        ),
        ("final field", "Final disposition: REPAIR_REQUIRED", false),
        (
            "current field",
            "Current disposition: REVIEW_INCOMPLETE",
            false,
        ),
        ("table field", "| Disposition | REPAIR_REQUIRED |", false),
        (
            "first-cell state",
            "| REPAIR_REQUIRED | fix oracle |",
            false,
        ),
        ("sole-cell state", "| REPAIR_REQUIRED |", false),
        ("ordinary table", "| fmt | pass |", true),
        (
            "numbered prose",
            "1. Recorded the earlier disposition in the issue.",
            true,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        expect_outcome(
            &check_reviews_with_pull(
                &none,
                &pull_object("OWNER", "o/r", &head, &format!("{line}\n{extra}")),
                &head,
                &format!("list-{index}"),
            )?,
            admit,
            name,
        )?;
    }
    // An older-head ready review plus an unfilled description admits nothing.
    expect_outcome(
        &check_reviews_with_pull(
            &stale_ready,
            &pull_object("MEMBER", "o/r", &head, "Source review:"),
            &head,
            "stale",
        )?,
        false,
        "older-head review with unfilled description",
    )?;
    // Without the pull object the same reviews keep the review-only outcome.
    expect_outcome(
        &check_reviews(&none, &head)?,
        false,
        "no reviews and no pull object",
    )?;
    // A pull object that is not an object refuses instead of crashing.
    expect_outcome(
        &check_reviews_with_pull(&none, &serde_json::json!([1]), &head, "malformed")?,
        false,
        "malformed pull object",
    )?;
    // A competing state field in the description makes it ambiguous, quoted, listed,
    // emphasised or fenced; the template's prose use of the word stays inert.
    let competing = [
        (format!("{line}\ndisposition: REPAIR_REQUIRED"), false),
        (
            format!("{line}\n- **Disposition:** REVIEW_INCOMPLETE"),
            false,
        ),
        (format!("{line}\n> disposition: REVIEW_READY"), false),
        (
            format!("{line}\n```\ndisposition: REPAIR_REQUIRED\n```"),
            false,
        ),
        (
            format!("{line}\nLosing candidate disposition (when decided): none"),
            true,
        ),
    ];
    for (index, (body, admit)) in competing.iter().enumerate() {
        expect_outcome(
            &check_reviews_with_pull(
                &none,
                &pull_object("MEMBER", "o/r", &head, body),
                &head,
                &format!("competing-{index}"),
            )?,
            *admit,
            body,
        )?;
    }
    // A reviewer's negative statement outlives their own later neutral reply (a thread
    // reply creates one); only their own later ready statement replaces it, and a
    // negative statement on an earlier head never applied to this head.
    let at = |second: u8| format!("2026-10-15T01:00:0{second}Z");
    let negative_then_reply = serde_json::json!([
        review("r", "COMMENTED", &head, &at(1), "REPAIR_REQUIRED"),
        review("r", "COMMENTED", &head, &at(2), ""),
    ]);
    let negative_then_ready = serde_json::json!([
        review("r", "COMMENTED", &head, &at(1), "REPAIR_REQUIRED"),
        review("r", "COMMENTED", &head, &at(2), ""),
        review("r", "COMMENTED", &head, &at(3), "REVIEW_READY"),
        review("r", "COMMENTED", &head, &at(4), ""),
    ]);
    let earlier_head_negative = serde_json::json!([
        review("r", "COMMENTED", &other, &at(1), "REPAIR_REQUIRED"),
        review("r", "COMMENTED", &head, &at(2), ""),
    ]);
    let ready_pull = pull_object("MEMBER", "o/r", &head, &line);
    for (name, reviews, admit) in [
        ("negative then neutral reply", &negative_then_reply, false),
        ("negative, reply, ready, reply", &negative_then_ready, true),
        ("negative on an earlier head", &earlier_head_negative, true),
    ] {
        expect_outcome(
            &check_reviews_with_pull(reviews, &ready_pull, &head, name)?,
            admit,
            name,
        )?;
    }
    expect_outcome(
        &check_reviews(&negative_then_reply, &head)?,
        false,
        "negative then neutral reply without a description line",
    )
}

#[test]
fn verdict_fields_use_a_closed_positive_grammar() -> Result<(), String> {
    let head = "1cc13d3de7941b83f2488853e120fbdfd3d9cee5";
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/review_verdicts/refuse-pr13896-5892179853.md");
    let recorded = std::fs::read_to_string(&fixture).map_err(|e| format!("{fixture:?}: {e}"))?;
    let cases = [
        (recorded.as_str(), false),
        (
            "SOURCE_REVIEW_READY\nVERDICT: BLOCK 1cc13d3de7941b83f2488853e120fbdfd3d9cee5",
            false,
        ),
        ("SOURCE_REVIEW_READY\nVERDICT: ABSTAIN", false),
        ("SOURCE_REVIEW_READY\nVERDICT: MAYBE", false),
        ("SOURCE_REVIEW_READY\nVERDICT: APPROVE / BLOCK", false),
        ("SOURCE_REVIEW_READY\nVERDICT: APPROVE", true),
        (
            "SOURCE_REVIEW_READY\nVERDICT: APPROVE 1cc13d3de7941b83f2488853e120fbdfd3d9cee5",
            true,
        ),
        ("SOURCE_REVIEW_READY", true),
        ("REVIEW_READY\nREPAIR_REQUIRED", false),
    ];
    for (body, admit) in cases {
        let reviews = serde_json::json!([review(
            "reviewer",
            "COMMENTED",
            head,
            "2026-11-13T15:00:00Z",
            body,
        )]);
        expect_outcome(&check_reviews(&reviews, head)?, admit, body)?;
    }
    Ok(())
}

#[test]
fn table_verdict_rows_use_the_same_closed_grammar() -> Result<(), String> {
    let head = "1cc13d3de7941b83f2488853e120fbdfd3d9cee5";
    let exact_block =
        "SOURCE_REVIEW_READY\n| Field | Value |\n| --- | --- |\n| Verdict | BLOCK |";
    let controls = [
        (exact_block, false),
        ("SOURCE_REVIEW_READY\n| Verdict | ABSTAIN |", false),
        ("SOURCE_REVIEW_READY\n| Verdict | MAYBE |", false),
        ("SOURCE_REVIEW_READY\n| Verdict |  |", false),
        ("SOURCE_REVIEW_READY\n| Verdict | APPROVE / BLOCK |", false),
        (
            "SOURCE_REVIEW_READY\n| Verdict | APPROVE 1cc13d3de7941b83f2488853e120fbdfd3d9cee5 |",
            true,
        ),
    ];
    for (body, admit) in controls {
        let reviews = serde_json::json!([review(
            "reviewer",
            "COMMENTED",
            head,
            "2026-11-14T07:51:00Z",
            body,
        )]);
        expect_outcome(&check_reviews(&reviews, head)?, admit, body)?;
    }

    let described = pull_object(
        "MEMBER",
        "o/r",
        head,
        &format!("Source review: REVIEW_READY {head}\n| Verdict | BLOCK |"),
    );
    expect_outcome(
        &check_reviews_with_pull(&serde_json::json!([]), &described, head, "table-verdict")?,
        false,
        "table verdict is a competing PR-description field",
    )
}
