//! Runs the stdlib-Python self-tests of the pre-allocation admission action
//! (#5391) so the checked-in decision scripts have a Rust-owned oracle.

use std::path::PathBuf;
use std::process::Command;

fn run_self_test(script: &str, expected: &str) -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let path = root
        .join(".github/actions/pr-candidate-admission")
        .join(script);
    let output = Command::new("python3")
        .arg(&path)
        .arg("--self-test")
        .output()
        .map_err(|error| format!("python3 must be available for {script}: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        return Err(format!(
            "{script} self-test failed: {stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if !stdout.contains(expected) {
        return Err(format!("{script} unexpected output: {stdout}"));
    }
    Ok(())
}

#[test]
fn candidate_admission_self_test_passes() -> Result<(), String> {
    run_self_test("candidate_admission.py", "controls passed")
}

#[test]
fn release_cut_admission_self_test_passes() -> Result<(), String> {
    run_self_test("release_cut_admission.py", "controls passed")
}

/// Executes the real action shell with a stubbed read-only `gh` so the
/// wrapper boundary (live-ref re-read) is exercised, not only the pure scripts.
fn run_wrapper(second_ref_packet_state: &str, initial_absent: bool) -> Result<i32, String> {
    run_scenario(second_ref_packet_state, initial_absent, None).map(|(code, _, _)| code)
}

/// The base carries an `active` packet bound to the product cut `b…b`; the
/// live ref does not move. `base_parent` is the first parent of the base commit.
/// Returns the exit code and the `cut_reason` the action published.
fn run_active_wrapper(files_json: &str, base_parent: &str) -> Result<(i32, String), String> {
    let (code, outputs, _) = run_scenario("active", false, Some((files_json, base_parent)))?;
    let reason = outputs
        .lines()
        .find_map(|line| line.strip_prefix("cut_reason="))
        .unwrap_or("")
        .to_string();
    Ok((code, reason))
}

fn run_active_wrapper_summary(
    files_json: &str,
    base_parent: &str,
) -> Result<(i32, String, String), String> {
    let (code, outputs, summary) = run_scenario("active", false, Some((files_json, base_parent)))?;
    let reason = outputs
        .lines()
        .find_map(|line| line.strip_prefix("cut_reason="))
        .unwrap_or("")
        .to_string();
    Ok((code, reason, summary))
}

static NEXT_SCENARIO: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn run_scenario(
    second_ref_packet_state: &str,
    initial_absent: bool,
    active: Option<(&str, &str)>,
) -> Result<(i32, String, String), String> {
    run_scenario_body(
        second_ref_packet_state,
        initial_absent,
        active,
        false,
        false,
    )
}

fn run_scenario_body(
    second_ref_packet_state: &str,
    initial_absent: bool,
    active: Option<(&str, &str)>,
    missing_body: bool,
    hostile_head: bool,
) -> Result<(i32, String, String), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let action_dir = root.join(".github/actions/pr-candidate-admission");
    let yaml = std::fs::read_to_string(action_dir.join("action.yml")).map_err(|e| e.to_string())?;
    let body: Vec<&str> = yaml
        .lines()
        .skip_while(|line| *line != "      run: |")
        .skip(1)
        .map(|line| line.strip_prefix("        ").unwrap_or(line))
        .collect();
    let tmp = std::env::temp_dir().join(format!(
        "ripr-wrapper-{second_ref_packet_state}-{initial_absent}-{}-{}",
        std::process::id(),
        NEXT_SCENARIO.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    std::fs::write(tmp.join("action.sh"), body.join("\n")).map_err(|e| e.to_string())?;
    let packet = |state: &str| {
        let value = if state == "inactive" {
            serde_json::json!({"state": state})
        } else {
            serde_json::json!({"state": state, "observed_main": {"sha": "b".repeat(40)}})
        };
        base64_encode(value.to_string().as_bytes())
    };
    let inactive = std::fs::read_to_string(root.join("policy/release-hard-cut-admission.json"))
        .map_err(|e| e.to_string())?;
    std::fs::write(tmp.join("inactive.json"), &inactive).map_err(|e| e.to_string())?;
    let (mode, moved) = match second_ref_packet_state {
        "http403" | "http404" => (second_ref_packet_state, String::new()),
        state => ("packet", packet(state)),
    };
    std::fs::write(tmp.join("second_mode"), mode).map_err(|e| e.to_string())?;
    std::fs::write(tmp.join("moved_packet.b64"), moved).map_err(|e| e.to_string())?;
    if initial_absent {
        std::fs::write(tmp.join("initial_absent"), "1").map_err(|e| e.to_string())?;
    }
    if missing_body {
        std::fs::write(tmp.join("missing_body"), "1").map_err(|e| e.to_string())?;
    }
    if hostile_head {
        std::fs::write(tmp.join("hostile_head"), "1").map_err(|e| e.to_string())?;
    }
    if let Some((files_json, base_parent)) = active {
        let packet = serde_json::json!({
            "schema_version": "1",
            "kind": "ripr_release_cut_admission",
            "release": "0.11.0",
            "state": "active",
            "generation": "cut-1",
            "controller_issue": 5114,
            "selection_issue": 2766,
            "observed_main": {"sha": "b".repeat(40), "tree": "c".repeat(40)},
            "admitted": [{
                "pr": 9,
                "head_sha": "9".repeat(40),
                "admission": "admitted",
                "disposition": "release_required_product",
                "owner_issue": 99,
                "release_effect": "landed before the final cut",
                "proof_refs": ["#99"]
            }],
            "non_claims": ["Admission is not product qualification or merge authority."]
        });
        std::fs::write(tmp.join("active.json"), packet.to_string()).map_err(|e| e.to_string())?;
        std::fs::write(tmp.join("files.json"), files_json).map_err(|e| e.to_string())?;
        std::fs::write(tmp.join("base_parent"), base_parent).map_err(|e| e.to_string())?;
    }
    let gh = r#"#!/usr/bin/env bash
url=""; for a in "$@"; do case "$a" in repos/*) url="$a";; esac; done
case "$url" in
  */git/ref/*) if [ -e "$CONTROL_TMP/active.json" ]; then echo dddddddddddddddddddddddddddddddddddddddd; elif [ -e "$CONTROL_TMP/seen" ]; then echo ffffffffffffffffffffffffffffffffffffffff; else touch "$CONTROL_TMP/seen"; echo dddddddddddddddddddddddddddddddddddddddd; fi ;;
  */git/commits/bbbb*) echo '{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","tree":{"sha":"cccccccccccccccccccccccccccccccccccccccc"},"parents":[]}' ;;
  */git/commits/*) if [ -e "$CONTROL_TMP/base_parent" ]; then jq -n --arg p "$(cat "$CONTROL_TMP/base_parent")" '{sha:"dddddddddddddddddddddddddddddddddddddddd",tree:{sha:"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"},parents:[{sha:$p}]}'; else echo '{"sha":"dddddddddddddddddddddddddddddddddddddddd","tree":{"sha":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"},"parents":[]}'; fi ;;
  */contents/*ref=ffff*)
    case "$(cat "$CONTROL_TMP/second_mode")" in
      http403) echo "gh: Forbidden (HTTP 403)" >&2; exit 1 ;;
      http404) echo "gh: Not Found (HTTP 404)" >&2; exit 1 ;;
      *) jq -n --arg c "$(cat "$CONTROL_TMP/moved_packet.b64")" '{content: $c}' ;;
    esac ;;
  */contents/*)
    if [ -e "$CONTROL_TMP/initial_absent" ]; then echo "gh: Not Found (HTTP 404)" >&2; exit 1; fi
    if [ -e "$CONTROL_TMP/active.json" ]; then base64 "$CONTROL_TMP/active.json" | tr -d '\r\n' | jq -R '{content: .}'; exit 0; fi
    base64 "$CONTROL_TMP/inactive.json" | tr -d '\r\n' | jq -R '{content: .}' ;;
  */files?*) if [ -e "$CONTROL_TMP/files.json" ]; then cat "$CONTROL_TMP/files.json"; else echo '[{"filename":"src/lib.rs","status":"modified"}]'; fi ;;
  */pulls\?state*) echo '[]' ;;
  *) if [ -e "$CONTROL_TMP/hostile_head" ]; then printf '%s\n' '{"number":10,"state":"open","draft":false,"head":{"sha":"aaaaaaaaaaaa\nstatus=admitted","repo":{"full_name":"o/r"}},"base":{"ref":"main"},"changed_files":1,"additions":1,"deletions":0,"body":"Governing issue or accepted claim: #5391\nAcceptance delta (one sentence): race control"}'; elif [ -e "$CONTROL_TMP/missing_body" ]; then echo '{"number":10,"state":"open","draft":false,"head":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","repo":{"full_name":"o/r"}},"base":{"ref":"main"},"changed_files":1,"additions":1,"deletions":0,"body":""}'; else echo '{"number":10,"state":"open","draft":false,"head":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","repo":{"full_name":"o/r"}},"base":{"ref":"main"},"changed_files":1,"additions":1,"deletions":0,"body":"Governing issue or accepted claim: #5391\nAcceptance delta (one sentence): race control"}'; fi ;;
esac
"#;
    std::fs::write(tmp.join("gh"), gh).map_err(|e| e.to_string())?;
    let _ = Command::new("chmod").arg("+x").arg(tmp.join("gh")).status();
    let path = format!(
        "{}:{}",
        tmp.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let status = Command::new("bash")
        .args(["-e", "-o", "pipefail"])
        .arg(tmp.join("action.sh"))
        .env("PATH", path)
        .env("CONTROL_TMP", &tmp)
        .env("GITHUB_ACTION_PATH", &action_dir)
        .env("GITHUB_WORKSPACE", &root)
        .env("RUNNER_TEMP", &tmp)
        .env("GITHUB_OUTPUT", tmp.join("outputs"))
        .env("GITHUB_STEP_SUMMARY", tmp.join("summary"))
        .env("GH_TOKEN", "local-fixture")
        .env("ADMISSION_REPOSITORY", "o/r")
        .env("ADMISSION_PR_NUMBER", "10")
        .env("ADMISSION_EXPECTED_HEAD", "a".repeat(40))
        .env("ADMISSION_DOCS_ONLY", "false")
        .env("ADMISSION_MAX_CHANGED_FILES", "160")
        .env("ADMISSION_MAX_CHANGED_LINES", "3000")
        .env("ADMISSION_MODE", "admit")
        .status()
        .map_err(|e| e.to_string())?;
    let outputs = std::fs::read_to_string(tmp.join("outputs")).unwrap_or_default();
    let summary = std::fs::read_to_string(tmp.join("summary")).unwrap_or_default();
    if std::env::var_os("KEEP_WRAPPER_TMP").is_none() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    let code = status
        .code()
        .ok_or_else(|| "wrapper ended by signal".to_string())?;
    Ok((code, outputs, summary))
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(TABLE[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[test]
fn wrapper_refuses_activation_observed_after_an_inactive_snapshot() -> Result<(), String> {
    let code = run_wrapper("active", false)?;
    if code == 0 {
        return Err("inactive->active movement during admission must not admit".to_string());
    }
    Ok(())
}

#[test]
fn wrapper_tolerates_main_moving_between_inactive_snapshots() -> Result<(), String> {
    let code = run_wrapper("inactive", false)?;
    if code != 0 {
        return Err(format!(
            "inactive->inactive movement must admit, got exit {code}"
        ));
    }
    Ok(())
}

#[test]
fn wrapper_refuses_unreadable_packet_after_bootstrap_movement() -> Result<(), String> {
    if run_wrapper("http403", true)? == 0 {
        return Err("404 then 403 after base movement must not admit".to_string());
    }
    if run_wrapper("http403", false)? == 0 {
        return Err("a 403 on the moved base must not admit".to_string());
    }
    if run_wrapper("active", true)? == 0 {
        return Err("bootstrap then activation must not admit".to_string());
    }
    Ok(())
}

#[test]
fn wrapper_admits_bootstrap_when_the_moved_base_also_has_no_packet() -> Result<(), String> {
    let code = run_wrapper("http404", true)?;
    if code != 0 {
        return Err(format!(
            "404 then 404 must admit the bootstrap PR, got exit {code}"
        ));
    }
    Ok(())
}

const PACKET_ONLY: &str =
    r#"[{"filename":"policy/release-hard-cut-admission.json","status":"modified"}]"#;
const PRODUCT_ONLY: &str = r#"[{"filename":"src/lib.rs","status":"modified"}]"#;

#[test]
fn wrapper_refuses_ordinary_pr_while_the_packet_is_active() -> Result<(), String> {
    for (parent, expected) in [("b", "cut_frozen"), ("9", "cut_main_moved")] {
        let (code, reason) = run_active_wrapper(PRODUCT_ONLY, &parent.repeat(40))?;
        if code != 1 || reason != expected {
            return Err(format!(
                "ordinary PR (base parent {parent}) must be refused as {expected}, got exit {code} reason {reason:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn wrapper_summary_names_the_missing_template_lines() -> Result<(), String> {
    let (code, outputs, summary) = run_scenario_body("inactive", false, None, true, false)?;
    if code != 1 {
        return Err(format!(
            "a PR body missing the template lines must be refused, got exit {code}: {outputs}"
        ));
    }
    if !outputs.contains("reason=claim_identity_missing")
        || !outputs.contains("Governing issue or accepted claim:")
    {
        return Err(format!(
            "action outputs must carry the concrete refusal detail, got: {outputs}"
        ));
    }
    if !summary.contains("- refusal reason: `claim_identity_missing`")
        || !summary.contains("Governing issue or accepted claim:")
        || !summary.contains("Acceptance delta (one sentence):")
    {
        return Err(format!(
            "step summary must name the missing template lines, got: {summary}"
        ));
    }
    Ok(())
}

#[test]
fn wrapper_summary_names_the_concrete_hard_cut_refusal_reason() -> Result<(), String> {
    let (code, reason, summary) = run_active_wrapper_summary(PRODUCT_ONLY, &"b".repeat(40))?;
    if code != 1 || reason != "cut_frozen" {
        return Err(format!(
            "ordinary PR must be refused as cut_frozen, got exit {code} reason {reason:?}"
        ));
    }
    if !summary.contains("- refusal reason: `cut_frozen`")
        || !summary.contains("hard-cut freeze")
        || summary.contains("- hard-cut refusal reason:")
    {
        return Err(format!(
            "step summary must name the single concrete refusal reason, got: {summary}"
        ));
    }
    if summary.matches("- refusal reason:").count() != 1 {
        return Err(format!(
            "step summary must present exactly one primary refusal reason, got: {summary}"
        ));
    }
    Ok(())
}

#[test]
fn wrapper_summary_names_one_primary_reason_when_both_admissions_refuse() -> Result<(), String> {
    let (code, outputs, summary) = run_scenario_body(
        "active",
        false,
        Some((PRODUCT_ONLY, &"b".repeat(40))),
        true,
        false,
    )?;
    if code != 1
        || !outputs.contains("reason=claim_identity_missing")
        || !outputs.contains("cut_reason=cut_frozen")
    {
        return Err(format!(
            "both-refused fixture must refuse as candidate and hard-cut, got exit {code}: {outputs}"
        ));
    }
    if summary.matches("- refusal reason:").count() != 1
        || !summary.contains("- refusal reason: `claim_identity_missing`")
        || !summary.contains("- also refused (subordinate): hard-cut `cut_frozen`")
        || summary.contains("- hard-cut refusal reason:")
    {
        return Err(format!(
            "step summary must present the candidate refusal as primary and the hard-cut refusal as subordinate, got: {summary}"
        ));
    }
    Ok(())
}

#[test]
fn wrapper_sanitizes_newline_bearing_head_in_outputs() -> Result<(), String> {
    let (code, outputs, summary) = run_scenario_body("inactive", false, None, false, true)?;
    if code != 1 || !outputs.contains("reason=candidate_head_moved") {
        return Err(format!(
            "newline-bearing head must be refused as candidate_head_moved, got exit {code}: {outputs}"
        ));
    }
    if outputs.lines().any(|line| line == "status=admitted") {
        return Err(format!(
            "newline-bearing head must not forge an extra GITHUB_OUTPUT line, got: {outputs}"
        ));
    }
    if outputs.lines().count() != 11
        || !outputs
            .lines()
            .any(|line| line.starts_with("detail=") && line.contains("status=admitted"))
    {
        return Err(format!(
            "sanitized detail must stay on its single detail line, got: {outputs}"
        ));
    }
    if summary.lines().any(|line| line == "status=admitted") {
        return Err(format!(
            "newline-bearing head must not forge a step-summary line, got: {summary}"
        ));
    }
    Ok(())
}

#[test]
fn wrapper_admits_packet_only_pr_while_the_packet_is_active() -> Result<(), String> {
    for parent in ["b", "9"] {
        let (code, reason) = run_active_wrapper(PACKET_ONLY, &parent.repeat(40))?;
        if code != 0 || reason != "cut_packet_control_update" {
            return Err(format!(
                "packet-only PR (base parent {parent}) must admit as cut_packet_control_update, got exit {code} reason {reason:?}"
            ));
        }
    }
    Ok(())
}
