// RIPR-SPEC-0202 / #7736: reach real policy omissions, and keep the opposite
// recovery for a genuinely unknown seam under the same omitting policy.
use super::{
    agent_brief_sample_workspace, assert_success, commit_repair_fixture, init_git_fixture_repo,
    printed_ripr_args, run_git, run_ripr,
};
use std::path::PathBuf;
use std::process::Output;

fn omitted_seam(label: &str) -> Result<(PathBuf, String), Box<dyn std::error::Error>> {
    let (root, diff) = agent_brief_sample_workspace(label)?;
    let root_arg = root.display().to_string();
    let diff_arg = diff.display().to_string();
    let brief = run_ripr(&[
        "agent", "brief", "--root", &root_arg, "--diff", &diff_arg, "--json",
    ]);
    assert_success(&brief);
    let brief: serde_json::Value = serde_json::from_slice(&brief.stdout)?;
    assert_eq!(
        brief["top_seams"][0]["grip_class"], "weakly_gripped",
        "{brief}"
    );
    let seam_id = brief["top_seams"][0]["seam_id"]
        .as_str()
        .ok_or("fixture must expose a real weakly_gripped seam before policy omission")?
        .to_string();
    std::fs::write(
        root.join("ripr.toml"),
        "[severity.seams]\nweakly_gripped = \"off\"\n",
    )?;
    init_git_fixture_repo(&root)?;
    run_git(&root, &["add", "Cargo.toml", "src", "tests", "ripr.toml"])?;
    commit_repair_fixture(&root, &["-qm", "policy omission fixture"])?;
    let omitted = run_ripr(&[
        "agent",
        "brief",
        "--root",
        &root_arg,
        "--seam-id",
        &seam_id,
        "--json",
    ]);
    assert_success(&omitted);
    let omitted: serde_json::Value = serde_json::from_slice(&omitted.stdout)?;
    assert_eq!(omitted["top_seams"], serde_json::json!([]), "{omitted}");
    assert!(
        omitted["warnings"]
            .to_string()
            .contains("configured off for weakly_gripped seams"),
        "{omitted}"
    );
    Ok((root, seam_id))
}

fn card_envelope(output: &Output) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let end = stderr
        .find("\n}\n")
        .ok_or("card refusal envelope must terminate")?
        + 3;
    let envelope: serde_json::Value = serde_json::from_str(&stderr[..end])?;
    assert_eq!(envelope["schema_version"], "0.1", "{envelope}");
    assert_eq!(envelope["kind"], "agent_card_refusal", "{envelope}");
    let message = envelope["error"]["message"]
        .as_str()
        .ok_or("card refusal must carry prose")?;
    assert_eq!(stderr[end..].trim(), format!("ripr: {message}"));
    Ok(envelope)
}

fn policy_omitted_route(
    root_arg: &str,
    seam_id: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let card = run_ripr(&[
        "agent",
        "card",
        "--root",
        root_arg,
        "--seam-id",
        seam_id,
        "--json",
    ]);
    let envelope = card_envelope(&card)?;
    assert_eq!(envelope["error"]["kind"], "policy_omitted", "{envelope}");
    Ok(envelope["error"]["remedy_route"]
        .as_str()
        .ok_or("missing policy remedy route")?
        .to_string())
}

fn assert_policy_guidance(message: &str, route: &str) {
    assert!(
        message.contains("configured off for weakly_gripped seams"),
        "{message}"
    );
    assert!(
        message.contains("Inspect the agent policy with `ripr agent brief --root "),
        "{message}"
    );
    assert!(message.contains(&format!("`{route}`")), "{message}");
    assert!(
        message.contains("re-listing cannot change this policy decision"),
        "{message}"
    );
    assert!(!message.contains("ripr pilot"), "{message}");
    assert!(!message.contains("to list current seam IDs"), "{message}");
}

#[test]
fn policy_omitted_card_prose_agrees_with_typed_remedy() -> Result<(), Box<dyn std::error::Error>> {
    let (root, seam_id) = omitted_seam("policy omitted card")?;
    let root_arg = root.display().to_string();
    let human = run_ripr(&["agent", "card", "--root", &root_arg, "--seam-id", &seam_id]);
    assert_eq!(human.status.code(), Some(3), "{human:?}");
    assert!(human.stdout.is_empty(), "{human:?}");
    let typed = run_ripr(&[
        "agent",
        "card",
        "--root",
        &root_arg,
        "--seam-id",
        &seam_id,
        "--json",
    ]);
    let envelope = card_envelope(&typed)?;
    assert_eq!(envelope["error"]["kind"], "policy_omitted", "{envelope}");
    assert_eq!(envelope["error"]["seam_id"], seam_id, "{envelope}");
    let message = envelope["error"]["message"]
        .as_str()
        .ok_or("missing prose")?;
    assert_eq!(
        String::from_utf8_lossy(&human.stderr).trim(),
        format!("ripr: {message}")
    );
    let route = envelope["error"]["remedy_route"]
        .as_str()
        .ok_or("missing remedy route")?;
    let command_root = root_arg.replace('\\', "/");
    assert_eq!(
        printed_ripr_args(route)?,
        [
            "agent",
            "brief",
            "--root",
            &command_root,
            "--seam-id",
            &seam_id,
            "--json"
        ]
    );
    assert_policy_guidance(message, route);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn policy_omitted_packet_preserves_refusal_channels() -> Result<(), Box<dyn std::error::Error>> {
    let (root, seam_id) = omitted_seam("policy omitted packet")?;
    let root_arg = root.display().to_string();
    let human = run_ripr(&[
        "agent",
        "packet",
        "--root",
        &root_arg,
        "--seam-id",
        &seam_id,
    ]);
    assert_eq!(human.status.code(), Some(2), "{human:?}");
    assert!(human.stdout.is_empty(), "{human:?}");
    // Packet supports JSON only; its human prose is stderr of that call.
    assert!(
        String::from_utf8_lossy(&human.stderr).contains("agent packet requires --json"),
        "{human:?}"
    );
    let typed = run_ripr(&[
        "agent",
        "packet",
        "--root",
        &root_arg,
        "--seam-id",
        &seam_id,
        "--json",
    ]);
    assert_eq!(typed.status.code(), Some(2), "{typed:?}");
    let envelope: serde_json::Value = serde_json::from_slice(&typed.stdout)?;
    assert_eq!(envelope["schema_version"], "0.5", "{envelope}");
    assert_eq!(
        envelope["run_limitations"][0]["basis"], "analysis_failed",
        "{envelope}"
    );
    let message = envelope["run_limitations"][0]["message"]
        .as_str()
        .ok_or("missing packet prose")?;
    assert_eq!(
        String::from_utf8_lossy(&typed.stderr).trim(),
        format!("ripr: {message}")
    );
    let route = policy_omitted_route(&root_arg, &seam_id)?;
    assert_policy_guidance(message, &route);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn policy_omitted_repair_before_preserves_refusal_channels()
-> Result<(), Box<dyn std::error::Error>> {
    let (root, seam_id) = omitted_seam("policy omitted repair")?;
    let root_arg = root.display().to_string();
    let route = policy_omitted_route(&root_arg, &seam_id)?;
    for json in [false, true] {
        let mut args = vec![
            "agent",
            "repair",
            "--root",
            &root_arg,
            "--seam-id",
            &seam_id,
            "--phase",
            "before",
        ];
        if json {
            args.push("--json");
        }
        let repair = run_ripr(&args);
        assert_eq!(repair.status.code(), Some(2), "{repair:?}");
        assert!(repair.stdout.is_empty(), "{repair:?}");
        let attempts = root.join("target/ripr/repair-attempts");
        if attempts.exists() {
            for entry in std::fs::read_dir(attempts)? {
                assert_eq!(
                    entry?.file_name(),
                    ".before.lock",
                    "policy omission must not publish an attempt"
                );
            }
        }
        assert!(
            !root.join("target/ripr/workflow").exists(),
            "policy omission must not prepare a workflow"
        );
        assert_policy_guidance(&String::from_utf8_lossy(&repair.stderr), &route);
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn genuinely_unknown_seam_keeps_pilot_guidance_under_omitting_policy()
-> Result<(), Box<dyn std::error::Error>> {
    let (root, _) = omitted_seam("policy omitted unknown control")?;
    let root_arg = root.display().to_string();
    let unknown = "0000000000000000";
    for (command, code) in [("card", 3), ("packet", 2), ("repair", 2)] {
        let mut args = vec![
            "agent",
            command,
            "--root",
            &root_arg,
            "--seam-id",
            unknown,
            "--json",
        ];
        if command == "repair" {
            args.extend(["--phase", "before"]);
        }
        let output = run_ripr(&args);
        assert_eq!(output.status.code(), Some(code), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("was not found"), "{stderr}");
        assert!(stderr.contains("Run `ripr pilot --root "), "{stderr}");
        assert!(stderr.contains("to list current seam IDs"), "{stderr}");
        assert!(!stderr.contains("ripr agent brief"), "{stderr}");
        if command == "card" {
            let envelope = card_envelope(&output)?;
            assert_eq!(envelope["error"]["kind"], "seam_not_found", "{envelope}");
        } else if command == "packet" {
            let envelope: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            assert_eq!(
                envelope["run_limitations"][0]["basis"], "seam_id_unknown",
                "{envelope}"
            );
        } else {
            assert!(output.stdout.is_empty(), "{output:?}");
        }
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}
