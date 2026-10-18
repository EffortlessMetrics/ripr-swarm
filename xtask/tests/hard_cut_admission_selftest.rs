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
fn run_wrapper(second_ref_packet_state: &str) -> Result<i32, String> {
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
        "ripr-wrapper-{second_ref_packet_state}-{}",
        std::process::id()
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
    std::fs::write(
        tmp.join("moved_packet.b64"),
        packet(second_ref_packet_state),
    )
    .map_err(|e| e.to_string())?;
    let gh = r#"#!/usr/bin/env bash
url=""; for a in "$@"; do case "$a" in repos/*) url="$a";; esac; done
case "$url" in
  */git/ref/*) if [ -e "$CONTROL_TMP/seen" ]; then echo ffffffffffffffffffffffffffffffffffffffff; else touch "$CONTROL_TMP/seen"; echo dddddddddddddddddddddddddddddddddddddddd; fi ;;
  */git/commits/*) echo '{"sha":"dddddddddddddddddddddddddddddddddddddddd","tree":{"sha":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"},"parents":[]}' ;;
  */contents/*ref=ffff*) cat "$CONTROL_TMP/moved_packet.b64" ;;
  */contents/*) base64 -w0 "$CONTROL_TMP/inactive.json" | jq -R '{content: .}' ;;
  */files?*) echo '[{"filename":"src/lib.rs","status":"modified"}]' ;;
  */pulls\?state*) echo '[]' ;;
  *) echo '{"number":10,"state":"open","draft":false,"head":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","repo":{"full_name":"o/r"}},"base":{"ref":"main"},"changed_files":1,"additions":1,"deletions":0,"body":"Governing issue or accepted claim: #5391\nAcceptance delta (one sentence): race control"}' ;;
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
    let _ = std::fs::remove_dir_all(&tmp);
    status
        .code()
        .ok_or_else(|| "wrapper ended by signal".to_string())
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
    let code = run_wrapper("active")?;
    if code == 0 {
        return Err("inactive->active movement during admission must not admit".to_string());
    }
    Ok(())
}

#[test]
fn wrapper_tolerates_main_moving_between_inactive_snapshots() -> Result<(), String> {
    let code = run_wrapper("inactive")?;
    if code != 0 {
        return Err(format!(
            "inactive->inactive movement must admit, got exit {code}"
        ));
    }
    Ok(())
}
