//! Replay the installed-hook shell with explicit compiler/runner test doubles.
//! These controls prove orchestration refusals, not wheel or native-test success.

use super::{Fixture, TestResult, WORKFLOW, job, shell_from_job};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Output;

const STAGE: &str = "Stage the exact-source ordinary integration runner";
const RUN: &str = "Run the strict installed ordinary journey";
const SELECTOR: &str =
    "ordinary_daily_loop::ordinary_loop_preserves_production_subject_and_native_outcome";

fn executable(path: &Path, source: &str) -> TestResult {
    fs::create_dir_all(path.parent().ok_or("executable parent")?)?;
    fs::write(path, source)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

fn digest(path: &Path) -> TestResult<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

fn artifact(path: &Path) -> serde_json::Value {
    serde_json::json!({
        "reason":"compiler-artifact", "target":{"name":"boundary_gap_closure","kind":["test"]},
        "profile":{"test":true}, "executable":path,
    })
}

#[test]
fn ordinary_runner_requires_one_successful_exact_source_cargo_artifact() -> TestResult {
    let fixture = Fixture::new()?;
    let head = fixture.git(&["rev-parse", "HEAD"])?;
    let runner = fixture.0.join(".git/compiled-runner");
    executable(&runner, "#!/bin/bash\nexit 0\n")?;
    fixture.write_shim("rustc", "printf 'rustc 1.95.0 (fixture)\n'\n")?;
    fixture.write_shim(
        "cargo",
        r#"if test "$*" = '--version --verbose'; then printf 'cargo 1.95.0 (fixture)\n'; exit 0; fi
test "$*" = 'test -p ripr --locked --test boundary_gap_closure --no-run --message-format=json' || exit 91
cat "$CARGO_MESSAGES"
exit "$CARGO_TEST_EXIT"
"#,
    )?;
    let evidence = fixture.0.join("target/ripr/python-ordinary-runner");
    let output_path = fixture.0.join(".git/stage-output");
    let messages_path = fixture.0.join(".git/cargo-messages");
    let run = |messages: &[serde_json::Value], status: u8, candidate: &str| -> TestResult<Output> {
        if evidence.exists() {
            fs::remove_dir_all(&evidence)?;
        }
        fs::write(&output_path, "")?;
        let mut jsonl = Vec::new();
        for message in messages {
            serde_json::to_writer(&mut jsonl, message)?;
            jsonl.push(b'\n');
        }
        fs::write(&messages_path, jsonl)?;
        Ok(fixture
            .command("bash")?
            .args(["-c", &shell_from_job(WORKFLOW, "source-contract", STAGE)?])
            .env("GITHUB_WORKSPACE", &fixture.0)
            .env("GITHUB_OUTPUT", &output_path)
            .env("CANDIDATE_SHA", candidate)
            .env("CARGO_MESSAGES", &messages_path)
            .env("CARGO_TEST_EXIT", status.to_string())
            .output()?)
    };
    let valid = artifact(&runner);
    let positive = run(std::slice::from_ref(&valid), 0, &head)?;
    assert!(positive.status.success(), "{positive:?}");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(evidence.join("runner-receipt.json"))?)?;
    assert_eq!(receipt["candidate_sha"], head);
    assert_eq!(
        receipt["candidate_tree"],
        fixture.git(&["rev-parse", "HEAD^{tree}"])?
    );
    assert_eq!(receipt["runner_sha256"], digest(&runner)?);
    assert_eq!(receipt["test_selector"], SELECTOR);
    assert_eq!(
        fs::read(evidence.join("boundary_gap_closure"))?,
        fs::read(&runner)?
    );
    assert!(
        fs::read_to_string(&output_path)?
            .contains(&format!("runner_sha256={}\n", digest(&runner)?))
    );

    let mut wrong_kind = valid.clone();
    wrong_kind["target"]["kind"] = serde_json::json!(["bin"]);
    let mut not_a_test = valid.clone();
    not_a_test["profile"]["test"] = false.into();
    for messages in [
        vec![],
        vec![valid.clone(), valid.clone()],
        vec![wrong_kind],
        vec![not_a_test],
    ] {
        let result = run(&messages, 0, &head)?;
        assert!(
            !result.status.success(),
            "accepted ambiguous/wrong artifact: {result:?}"
        );
        assert!(!evidence.join("runner-receipt.json").exists());
        assert!(fs::read_to_string(&output_path)?.is_empty());
    }
    let failed_build = run(std::slice::from_ref(&valid), 17, &head)?;
    assert!(!failed_build.status.success());
    assert_eq!(
        fs::read_to_string(evidence.join("build-exit-status.txt"))?,
        "17\n"
    );
    assert!(!evidence.join("runner-receipt.json").exists());
    assert!(
        !run(std::slice::from_ref(&valid), 0, &"0".repeat(40))?
            .status
            .success()
    );
    assert!(
        !evidence.join("cargo-build.jsonl").exists(),
        "wrong source must refuse before compilation"
    );
    fs::write(fixture.0.join("Cargo.toml"), "# uncommitted source change\n")?;
    assert!(!run(&[valid], 0, &head)?.status.success());
    assert!(!evidence.join("runner-receipt.json").exists());
    assert!(fs::read_to_string(&output_path)?.is_empty());
    Ok(())
}

const FAKE_RUNNER: &str = r#"#!/bin/bash
set -euo pipefail
selector=ordinary_daily_loop::ordinary_loop_preserves_production_subject_and_native_outcome
test "$#" -eq 3 && test "$1" = "$selector" && test "$2" = --exact
scenario="$(cat "$(dirname "$0")/scenario")"
if test "$3" = --list; then
  if test "$scenario" = empty_inventory; then printf '0 tests, 0 benchmarks\n'; exit 0; fi
  printf '%s: test\n\n1 test, 0 benchmarks\n' "$selector"
  exit 0
fi
test "$3" = --nocapture
printf invoked > "$(dirname "$0")/invoked"
test "${RIPR_ORDINARY_MODE:-}" = installed || exit 83
test -x "${RIPR_ORDINARY_INSTALLED_BINARY:?}"
test "${#RIPR_ORDINARY_EXPECTED_SHA256}" -eq 64
test -n "${RIPR_ORDINARY_EXPECTED_VERSION:?}"
test -n "${RIPR_ORDINARY_EVIDENCE_DIR:?}"
test "${CARGO_NET_OFFLINE:?}" = true
test "${RUSTUP_TOOLCHAIN:?}" = 1.95.0
test -x "${RUSTC:?}"
test -z "${CARGO_BIN_EXE_ripr:-}"
if test "$scenario" = zero_execution; then
  printf 'test result: ok. 0 passed; 0 failed; 0 ignored; 1 filtered out\n'
  exit 0
fi
printf 'test result: ok. 1 passed; 0 failed; 0 ignored; 19 filtered out\n'
mkdir "$RIPR_ORDINARY_EVIDENCE_DIR"
jq -n --arg binary "$(realpath "$RIPR_ORDINARY_INSTALLED_BINARY")" \
  --arg sha "sha256:$RIPR_ORDINARY_EXPECTED_SHA256" --arg version "$RIPR_ORDINARY_EXPECTED_VERSION" \
  '{resolved_binary:$binary,sha256:$sha,full_version:$version}' > "$RIPR_ORDINARY_EVIDENCE_DIR/admitted.json"
if test "$scenario" = missing_completion; then exit 0; fi
printf '%s\n' '{"state":"completed","input_kind":"admitted_installed_executable","disposable_roots_removed":true,"positive_native":{"selected":1,"executed":1,"passed":1,"failed":0,"ignored":0,"native_exit_code":0}}' > "$RIPR_ORDINARY_EVIDENCE_DIR/completed.json"
if test "$scenario" = zero_native_receipt; then
  jq '.positive_native.executed = 0' "$RIPR_ORDINARY_EVIDENCE_DIR/completed.json" > "$RIPR_ORDINARY_EVIDENCE_DIR/changed.json"
  mv "$RIPR_ORDINARY_EVIDENCE_DIR/changed.json" "$RIPR_ORDINARY_EVIDENCE_DIR/completed.json"
fi
mkdir "$RIPR_ORDINARY_EVIDENCE_DIR/command-000"
for name in command result; do printf '{}\n' > "$RIPR_ORDINARY_EVIDENCE_DIR/command-000/$name.json"; done
touch "$RIPR_ORDINARY_EVIDENCE_DIR/command-000/stdout" "$RIPR_ORDINARY_EVIDENCE_DIR/command-000/stderr"
if test "$scenario" = missing_command; then rm "$RIPR_ORDINARY_EVIDENCE_DIR/command-000/command.json"; fi
if test "$scenario" = missing_stream; then rm "$RIPR_ORDINARY_EVIDENCE_DIR/command-000/stdout"; fi
if test "$scenario" = native_failure; then exit 17; fi
"#;

#[test]
fn installed_hook_refuses_wrong_identity_empty_execution_and_missing_completion() -> TestResult {
    let fixture = Fixture::new()?;
    let root = fixture.0.join(".git/checkout-free consumer ü");
    let runner_dir = root.join("ordinary-runner");
    let evidence = root.join("ordinary-evidence");
    let temporary = root.join("tmp");
    let installed = root.join("installed/ripr");
    let runner = runner_dir.join("boundary_gap_closure");
    fs::create_dir_all(root.join("evidence"))?;
    fs::create_dir(&temporary)?;
    executable(&installed, "#!/bin/bash\nexit 97\n")?;
    executable(&runner, FAKE_RUNNER)?;
    fixture.write_shim("cargo", "printf 'cargo 1.95.0 (fixture)\n'\n")?;
    fixture.write_shim("rustc", "printf 'rustc 1.95.0 (fixture)\n'\n")?;
    fixture.write_shim(
        "rustup",
        "test \"$1 $2 $3\" = 'which --toolchain 1.95.0' || exit 90\nprintf '%s/%s\n' \"$NATIVE_TOOL_DIR\" \"$4\"\n",
    )?;
    let head = "a".repeat(40);
    let tree = "b".repeat(40);
    let runner_sha = digest(&runner)?;
    let payload_sha = digest(&installed)?;
    let runner_receipt = serde_json::json!({
        "candidate_sha":head, "candidate_tree":tree, "runner_sha256":runner_sha,
        "test_selector":SELECTOR,
    });
    let wheel_receipt = serde_json::json!({
        "candidate_sha":head, "candidate_tree":tree,
        "wheel":{"installed_payload_sha256":payload_sha,"sha256":"c".repeat(64)},
    });
    let invoke = |source: &str,
                  scenario: &str,
                  source_receipt: &serde_json::Value,
                  package_receipt: &serde_json::Value,
                  expected_runner: &str|
     -> TestResult<Output> {
        let native_temporary = temporary.join("ordinary-native-fixtures");
        for path in [&evidence, &native_temporary] {
            if path.exists() {
                fs::remove_dir_all(path)?;
            }
        }
        if runner_dir.join("invoked").exists() {
            fs::remove_file(runner_dir.join("invoked"))?;
        }
        fs::write(runner_dir.join("scenario"), scenario)?;
        fs::write(
            runner_dir.join("runner-receipt.json"),
            source_receipt.to_string(),
        )?;
        fs::write(
            root.join("evidence/wheel-receipt.json"),
            package_receipt.to_string(),
        )?;
        Ok(fixture
            .command("bash")?
            .args(["-c", &shell_from_job(source, "consume", RUN)?])
            .current_dir(&root)
            .env("GITHUB_WORKSPACE", &root)
            .env("RUNNER_TEMP", &temporary)
            .env("CANDIDATE_SHA", &head)
            .env("EXPECTED_ORDINARY_SOURCE_TREE", &tree)
            .env("EXPECTED_ORDINARY_RUNNER_SHA256", expected_runner)
            .env("EXPECTED_PAYLOAD_SHA256", &payload_sha)
            .env("EXPECTED_WHEEL_SHA256", "c".repeat(64))
            .env("RIPR_NATIVE_VERSION", "0.11.0")
            .env("CONSUMER_EXECUTABLE", &installed)
            .env("CONSUMER_PATH", "/usr/bin:/bin")
            .env("PLANTED_MARKER", root.join("planted-executed"))
            .env("NATIVE_TOOL_DIR", fixture.0.join(".git/bin"))
            .env("CARGO_BIN_EXE_ripr", "/wrong/workspace/binary")
            .output()?)
    };
    let positive = invoke(
        WORKFLOW,
        "positive",
        &runner_receipt,
        &wheel_receipt,
        &runner_sha,
    )?;
    assert!(positive.status.success(), "{positive:?}");
    assert_eq!(
        fs::read_to_string(evidence.join("native-exit-status.txt"))?,
        "0\n"
    );
    assert_eq!(
        fs::read_to_string(evidence.join("hook-exit-status.txt"))?,
        "0\n"
    );
    assert!(evidence.join("journey/command-000/result.json").is_file());
    for scenario in [
        "empty_inventory",
        "zero_execution",
        "native_failure",
        "missing_completion",
        "zero_native_receipt",
        "missing_command",
        "missing_stream",
    ] {
        let result = invoke(
            WORKFLOW,
            scenario,
            &runner_receipt,
            &wheel_receipt,
            &runner_sha,
        )?;
        assert!(!result.status.success(), "accepted {scenario}: {result:?}");
        assert_ne!(
            fs::read_to_string(evidence.join("hook-exit-status.txt"))?,
            "0\n"
        );
        if scenario == "empty_inventory" {
            assert!(!runner_dir.join("invoked").exists());
        } else {
            assert!(evidence.join("runner.stdout").is_file());
            assert!(evidence.join("runner.stderr").is_file());
        }
        if scenario == "native_failure" {
            assert_eq!(
                fs::read_to_string(evidence.join("native-exit-status.txt"))?,
                "17\n"
            );
        }
    }
    for field in [
        "candidate_sha",
        "candidate_tree",
        "runner_sha256",
        "test_selector",
    ] {
        let mut wrong = runner_receipt.clone();
        wrong[field] = "wrong".into();
        assert!(
            !invoke(WORKFLOW, "positive", &wrong, &wheel_receipt, &runner_sha)?
                .status
                .success()
        );
        assert!(!runner_dir.join("invoked").exists());
    }
    let mut wrong_wheel = wheel_receipt.clone();
    wrong_wheel["candidate_tree"] = "d".repeat(40).into();
    assert!(
        !invoke(WORKFLOW, "positive", &runner_receipt, &wrong_wheel, &runner_sha)?
            .status
            .success()
    );
    assert!(!runner_dir.join("invoked").exists());
    assert!(
        !invoke(
            WORKFLOW,
            "positive",
            &runner_receipt,
            &wheel_receipt,
            &"0".repeat(64)
        )?
        .status
        .success()
    );
    assert!(!runner_dir.join("invoked").exists());
    let missing_mode = WORKFLOW.replace("RIPR_ORDINARY_MODE=installed ", "");
    assert_ne!(missing_mode, WORKFLOW);
    let partial = invoke(
        &missing_mode,
        "positive",
        &runner_receipt,
        &wheel_receipt,
        &runner_sha,
    )?;
    assert!(
        !partial.status.success(),
        "partial strict input must not use the Cargo binary"
    );
    assert_eq!(
        fs::read_to_string(evidence.join("native-exit-status.txt"))?,
        "83\n"
    );
    let removed_status = WORKFLOW.replace("          test \"${native_status}\" -eq 0\n", "");
    assert_ne!(removed_status, WORKFLOW);
    let false_success = invoke(
        &removed_status,
        "native_failure",
        &runner_receipt,
        &wheel_receipt,
        &runner_sha,
    )?;
    assert!(
        false_success.status.success(),
        "removing native-status admission must expose false success: {false_success:?}"
    );
    assert_eq!(
        fs::read_to_string(evidence.join("native-exit-status.txt"))?,
        "17\n"
    );
    Ok(())
}

#[test]
fn ordinary_hook_preserves_rust_free_install_and_existing_consumer_guards() -> TestResult {
    let consume = job(WORKFLOW, "consume")?;
    let positions = [
        "Install from the local wheelhouse without Rust or source checkout",
        "Run no-config Python preview, fresh-shell follow-up, and disable control",
        "Download the ordinary test runner for the installed pip consumer",
        "Provision Rust only for the ordinary fixture native test",
        RUN,
        "Retain installed ordinary identities, logs, and native outcomes",
        "Reinstall and uninstall cleanly",
    ]
    .map(|name| consume.find(name).ok_or("consumer phase exists"));
    let positions = positions.into_iter().collect::<Result<Vec<_>, _>>()?;
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(!consume.contains("actions/checkout"));
    assert_eq!(consume.matches("dtolnay/rust-toolchain@").count(), 1);
    assert!(
        consume.contains("if: ${{ matrix.client == 'pip' }}\n        uses: dtolnay/rust-toolchain@")
    );
    assert!(consume.contains("id: ordinary\n        if: ${{ matrix.client == 'pip' }}"));
    assert!(consume.contains(
        "always() && matrix.client == 'pip' && steps.ordinary.outcome != 'skipped'"
    ));
    assert!(consume.contains(
        "needs.build-wheel.result == 'success' && needs.source-contract.result == 'success'"
    ));
    assert!(consume.contains("needs: [build-wheel, source-contract]"));
    assert!(consume.contains("client: [pip, uv]"));
    let install = shell_from_job(
        WORKFLOW,
        "consume",
        "Install from the local wheelhouse without Rust or source checkout",
    )?;
    assert!(install.contains(
        "command -v cargo >/dev/null || command -v rustc >/dev/null || command -v ripr >/dev/null"
    ));
    assert!(!install.contains("rustup"));
    let source = job(WORKFLOW, "source-contract")?;
    assert!(source.contains("steps.ordinary_runner.outputs.runner_sha256"));
    assert!(source.contains("always() && steps.ordinary_runner.outcome != 'skipped'"));
    assert!(!source.contains("continue-on-error"));
    Ok(())
}
