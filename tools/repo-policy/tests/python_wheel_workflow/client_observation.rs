//! Replays the workflow's actual terminal shell. Fixtures are observations, not provider proof.
use super::*;
use sha2::{Digest, Sha256};

const RECORD: &str = "Record completed Python wheel client execution";
const RETAIN: &str = "Retain completed Python wheel client observation";
const GUARD: &str = "        if: ${{ success() && github.event_name == 'merge_group' && inputs.proof_profile == 'merge-group' }}";
const COMMON: &[&str] = &[
    "deadline_download",
    "deadline_enter",
    "wheel_download",
    "client_install",
    "python_journey",
    "client_reinstall",
];
const PIP: &[&str] = &[
    "ordinary_download",
    "ordinary_rust",
    "ordinary",
    "ordinary_upload",
];

fn statuses() -> serde_json::Value {
    let mut value = serde_json::Map::new();
    for id in COMMON.iter().chain(PIP) {
        value.insert(
            (*id).into(),
            serde_json::json!({"outcome":"success", "conclusion":"success"}),
        );
    }
    value.into()
}

fn source_order(source: &str) -> TestResult<bool> {
    let consume = job(source, "consume")?;
    let deadline = consume
        .find("      - name: Complete within native qualification deadline\n")
        .ok_or("deadline")?;
    let record = consume
        .find(&format!("      - name: {RECORD}\n"))
        .ok_or("record")?;
    let retain = consume
        .find(&format!("      - name: {RETAIN}\n"))
        .ok_or("retain")?;
    let record_body = &consume[record..retain];
    let retain_body = &consume[retain..];
    let ids = [
        ("Download native-run deadline verifier", "deadline_download"),
        (
            "Enter within native qualification deadline",
            "deadline_enter",
        ),
        (
            "Install from the local wheelhouse without Rust or source checkout",
            "client_install",
        ),
        (
            "Run no-config Python preview, fresh-shell follow-up, and disable control",
            "python_journey",
        ),
        (
            "Download the ordinary test runner for the installed pip consumer",
            "ordinary_download",
        ),
        (
            "Provision Rust only for the ordinary fixture native test",
            "ordinary_rust",
        ),
        ("Run the strict installed ordinary journey", "ordinary"),
        (
            "Retain installed ordinary identities, logs, and native outcomes",
            "ordinary_upload",
        ),
        ("Reinstall and uninstall cleanly", "client_reinstall"),
        (
            "Complete within native qualification deadline",
            "deadline_complete",
        ),
    ];
    Ok(record < retain && retain < deadline
        && record_body.lines().filter(|line| *line == GUARD).count() == 1
        && retain_body.lines().filter(|line| *line == GUARD).count() == 1
        && ids.iter().all(|(name, id)| {
            let marker = format!("      - name: {name}\n        id: {id}\n");
            consume.matches(&marker).count() == 1
                && (name == &"Complete within native qualification deadline" || consume.find(&marker).is_some_and(|position| position < record))
        })
        && consume.contains("        id: wheel_download\n        with:\n          name: ripr-rs-python-wheel-${{ env.CANDIDATE_SHA }}")
        && retain_body.contains("          if-no-files-found: error"))
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn terminal_shell_rejects_incomplete_execution_and_stale_or_swapped_inputs() -> TestResult {
    let root = test_support::scratch("wheel-client-observation")?;
    fs::create_dir_all(root.join("evidence/wheelhouse"))?;
    fs::create_dir_all(root.join("ordinary-runner"))?;
    fs::create_dir_all(root.join("tmp"))?;
    let wheel = b"one wheel fixture";
    let runner = b"one runner fixture";
    fs::write(root.join("evidence/wheelhouse/a.whl"), wheel)?;
    fs::write(root.join("ordinary-runner/boundary_gap_closure"), runner)?;
    let subject = "c".repeat(40);
    let tree = "d".repeat(40);
    let generation = "e".repeat(40);
    let wheel_sha = hash(wheel);
    let runner_sha = hash(runner);
    fs::write(root.join("evidence/wheel-receipt.json"), serde_json::json!({"candidate_sha":subject,"candidate_tree":tree,"wheel":{"sha256":wheel_sha}}).to_string())?;
    fs::write(root.join("ordinary-runner/runner-receipt.json"), serde_json::json!({"candidate_sha":subject,"candidate_tree":tree,"runner_sha256":runner_sha}).to_string())?;
    let shell = shell_from_job(WORKFLOW, "consume", RECORD)?;
    let record = root.join("tmp/wheel-client-record/record.json");
    let run = |script: &str,
               client: &str,
               steps: &serde_json::Value,
               overrides: &[(&str, &str)]|
     -> TestResult<Output> {
        if record.exists() {
            fs::remove_file(&record)?;
        }
        let mut command = Command::new("timeout");
        command
            .args(["--kill-after=1s", "10s", "bash", "-c", script])
            .current_dir(&root)
            .env("GITHUB_REPOSITORY", "EffortlessMetrics/ripr-swarm")
            .env("GITHUB_EVENT_NAME", "merge_group")
            .env("CLIENT", client)
            .env("OBSERVED_STEPS", steps.to_string())
            .env("ADMITTED_HEAD", "a".repeat(40))
            .env("ADMITTED_BASE", "b".repeat(40))
            .env("ADMITTED_SUBJECT", &subject)
            .env("ADMITTED_TREE", &tree)
            .env("ADMITTED_GENERATION", &generation)
            .env("GITHUB_WORKFLOW_SHA", &generation)
            .env("GITHUB_RUN_ID", "123")
            .env("GITHUB_RUN_ATTEMPT", "2")
            .env("ADMITTED_REQUEST", "123-2")
            .env("CANDIDATE_SHA", &subject)
            .env("EXPECTED_WHEEL_SHA256", &wheel_sha)
            .env("EXPECTED_ORDINARY_RUNNER_SHA256", &runner_sha)
            .env("RUNNER_TEMP", root.join("tmp"));
        for (key, value) in overrides {
            command.env(key, value);
        }
        Ok(command.output()?)
    };
    for client in ["pip", "uv"] {
        let positive = run(&shell, client, &statuses(), &[])?;
        assert!(positive.status.success(), "{positive:?}");
        let result: serde_json::Value = serde_json::from_slice(&fs::read(&record)?)?;
        assert_eq!(result["client"], client);
        assert_eq!(result["deadline_acceptance"], "pending");
        assert_eq!(result["repository"], "EffortlessMetrics/ripr-swarm");
        assert_eq!(result["event"], "merge_group");
        assert_eq!(result["input"]["provider_artifact_identity"], "unavailable");
        assert_eq!(result["subject_sha"], subject);
        assert_eq!(result["tree_sha"], tree);
        assert_eq!(result["run_id"], "123");
        assert_eq!(result["run_attempt"], "2");
        assert_eq!(result["input"]["wheel_sha256"], wheel_sha);
        assert!(result["input"]["provider_artifact_id"].is_null());
        assert_eq!(
            result["input"]["ordinary_runner_sha256"],
            if client == "pip" {
                serde_json::json!(runner_sha)
            } else {
                serde_json::Value::Null
            }
        );
        let required: Vec<_> = COMMON
            .iter()
            .chain(if client == "pip" {
                PIP.iter()
            } else {
                [].iter()
            })
            .collect();
        for id in required {
            for state in ["failure", "skipped", "cancelled"] {
                let mut bad = statuses();
                bad[*id]["outcome"] = state.into();
                let output = run(&shell, client, &bad, &[])?;
                assert!(!output.status.success(), "{client} {id} {state}");
                assert!(!record.exists());
            }
            let mut partial = statuses();
            partial.as_object_mut().ok_or("steps object")?.remove(*id);
            assert!(
                !run(&shell, client, &partial, &[])?.status.success(),
                "{client} missing {id}"
            );
            assert!(!record.exists());
            let mut masked = statuses();
            masked[*id]["conclusion"] = "failure".into();
            assert!(
                !run(&shell, client, &masked, &[])?.status.success(),
                "{client} conclusion {id}"
            );
        }
        for (key, value) in [
            ("ADMITTED_REQUEST", "123-1"),
            ("GITHUB_EVENT_NAME", "workflow_dispatch"),
            ("GITHUB_RUN_ATTEMPT", "1"),
            (
                "ADMITTED_SUBJECT",
                "0000000000000000000000000000000000000000",
            ),
            ("ADMITTED_TREE", "0000000000000000000000000000000000000000"),
            (
                "GITHUB_WORKFLOW_SHA",
                "0000000000000000000000000000000000000000",
            ),
            (
                "EXPECTED_WHEEL_SHA256",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
        ] {
            assert!(
                !run(&shell, client, &statuses(), &[(key, value)])?
                    .status
                    .success(),
                "{client} stale {key}"
            );
            assert!(!record.exists());
        }
        fs::write(root.join("evidence/wheelhouse/a.whl"), b"swapped wheel")?;
        assert!(!run(&shell, client, &statuses(), &[])?.status.success());
        fs::write(root.join("evidence/wheelhouse/a.whl"), wheel)?;
        fs::remove_file(root.join("evidence/wheelhouse/a.whl"))?;
        assert!(!run(&shell, client, &statuses(), &[])?.status.success());
        fs::write(root.join("evidence/wheelhouse/a.whl"), wheel)?;
        fs::write(root.join("evidence/wheelhouse/extra.whl"), wheel)?;
        assert!(!run(&shell, client, &statuses(), &[])?.status.success());
        fs::remove_file(root.join("evidence/wheelhouse/extra.whl"))?;
        let receipt_path = root.join("evidence/wheel-receipt.json");
        let saved_receipt = fs::read(&receipt_path)?;
        fs::write(&receipt_path, serde_json::json!({"candidate_sha":"0".repeat(40),"candidate_tree":tree,"wheel":{"sha256":wheel_sha}}).to_string())?;
        assert!(!run(&shell, client, &statuses(), &[])?.status.success());
        fs::write(&receipt_path, saved_receipt)?;
    }
    fs::write(
        root.join("ordinary-runner/boundary_gap_closure"),
        b"swapped runner",
    )?;
    assert!(!run(&shell, "pip", &statuses(), &[])?.status.success());
    fs::remove_file(root.join("ordinary-runner/boundary_gap_closure"))?;
    assert!(!run(&shell, "pip", &statuses(), &[])?.status.success());
    fs::write(root.join("ordinary-runner/boundary_gap_closure"), runner)?;
    let wrong = shell.replace("all($required[]; . as $id | $observed[$id] | ok)", "true");
    assert_ne!(wrong, shell);
    let mut early = statuses();
    early
        .as_object_mut()
        .ok_or("steps object")?
        .remove("client_reinstall");
    assert!(
        run(&wrong, "pip", &early, &[])?.status.success(),
        "control must expose a premature record"
    );
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn terminal_record_follows_client_steps_and_precedes_required_deadline() -> TestResult {
    assert!(source_order(WORKFLOW)?);
    let weakened = WORKFLOW.replace(GUARD, "        if: ${{ always() }}");
    assert!(!source_order(&weakened)?);
    let early = WORKFLOW.replace("      - name: Complete within native qualification deadline\n        id: deadline_complete", "      - name: Complete within native qualification deadline\n        id: changed_deadline");
    assert!(!source_order(&early)?);
    let consume = job(WORKFLOW, "consume")?;
    let deadline = consume
        .find("      - name: Complete within native qualification deadline\n")
        .ok_or("deadline")?;
    let record = consume
        .find(&format!("      - name: {RECORD}\n"))
        .ok_or("record")?;
    let retain = consume
        .find(&format!("      - name: {RETAIN}\n"))
        .ok_or("retain")?;
    let reinstall = consume
        .find("      - name: Reinstall and uninstall cleanly\n")
        .ok_or("reinstall")?;
    let premature = WORKFLOW.replace(
        &consume,
        &format!(
            "{}{}{}{}",
            &consume[..reinstall],
            &consume[record..deadline],
            &consume[reinstall..record],
            &consume[deadline..]
        ),
    );
    assert_ne!(premature, WORKFLOW);
    assert!(!source_order(&premature)?);
    let swapped = WORKFLOW.replace(
        &consume,
        &format!(
            "{}{}{}{}",
            &consume[..record],
            &consume[deadline..],
            &consume[record..retain],
            &consume[retain..deadline]
        ),
    );
    assert_ne!(swapped, WORKFLOW);
    assert!(!source_order(&swapped)?);
    Ok(())
}
