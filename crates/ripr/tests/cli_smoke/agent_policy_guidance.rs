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

/// Like [`omitted_seam`], but the sample ships with no reaching tests, so
/// the top seam is unreached (not weakly gripped): the weak-oracle refusal
/// cannot fire and the stub producer succeeds, isolating the
/// policy-omission disclosure. Returns the root, the seam id, and the
/// brief-reported grip class the config omits.
fn unreached_omitted_seam(
    label: &str,
) -> Result<(PathBuf, String, String), Box<dyn std::error::Error>> {
    let (root, diff) = agent_brief_sample_workspace(label)?;
    // No reaching tests: drop the sample integration tests so the top seam
    // classifies without test evidence.
    std::fs::remove_dir_all(root.join("tests"))?;
    let root_arg = root.display().to_string();
    let diff_arg = diff.display().to_string();
    let brief = run_ripr(&[
        "agent", "brief", "--root", &root_arg, "--diff", &diff_arg, "--json",
    ]);
    assert_success(&brief);
    let brief: serde_json::Value = serde_json::from_slice(&brief.stdout)?;
    let class = brief["top_seams"][0]["grip_class"]
        .as_str()
        .ok_or("fixture must expose a real seam before policy omission")?
        .to_string();
    assert_ne!(
        class, "weakly_gripped",
        "without reaching tests the seam must not be weakly gripped: {brief}"
    );
    let seam_id = brief["top_seams"][0]["seam_id"]
        .as_str()
        .ok_or("fixture must expose a real seam id before policy omission")?
        .to_string();
    std::fs::write(
        root.join("ripr.toml"),
        format!("[severity.seams]\n{class} = \"off\"\n"),
    )?;
    init_git_fixture_repo(&root)?;
    run_git(&root, &["add", "Cargo.toml", "src", "ripr.toml"])?;
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
    Ok((root, seam_id, class))
}

fn card_envelope(output: &Output) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(envelope["schema_version"], "0.1", "{envelope}");
    assert_eq!(envelope["kind"], "agent_card_refusal", "{envelope}");
    let message = envelope["error"]["message"]
        .as_str()
        .ok_or("card refusal must carry prose")?;
    assert_eq!(stderr.trim(), format!("ripr: {message}"));
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
    assert!(!message.contains("to list current repair IDs"), "{message}");
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
    // #10712: a policy-withheld seam is the same typed refusal (exit 3) as
    // `agent card` gives, with its own cause rather than `analysis_failed`.
    assert_eq!(typed.status.code(), Some(3), "{typed:?}");
    let envelope: serde_json::Value = serde_json::from_slice(&typed.stdout)?;
    assert_eq!(envelope["schema_version"], "0.5", "{envelope}");
    assert_eq!(
        envelope["run_limitations"][0]["basis"], "seam_policy_omitted",
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
fn policy_omitted_repair_before_is_a_typed_refusal() -> Result<(), Box<dyn std::error::Error>> {
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
        // #10712 residual: a policy-withheld seam is the same typed refusal
        // (exit 3) as `agent card`/`start`/`packet` give, not exit 2.
        assert_eq!(repair.status.code(), Some(3), "{repair:?}");
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
    for (command, code) in [("card", 3), ("packet", 3), ("repair", 3)] {
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
        assert!(stderr.contains("to list current repair IDs"), "{stderr}");
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

/// #10712: `agent start` gives a policy-withheld seam the same typed refusal
/// (exit 3) and cause as `agent card` and `agent packet`, never "not found".
#[test]
fn policy_omitted_start_is_a_typed_refusal_naming_the_policy()
-> Result<(), Box<dyn std::error::Error>> {
    let (root, seam_id) = omitted_seam("policy omitted start")?;
    let root_arg = root.display().to_string();
    let route = policy_omitted_route(&root_arg, &seam_id)?;
    let start = run_ripr(&["agent", "start", "--root", &root_arg, "--seam-id", &seam_id]);
    assert_eq!(start.status.code(), Some(3), "{start:?}");
    let stderr = String::from_utf8_lossy(&start.stderr);
    assert!(!stderr.contains("not found"), "{stderr}");
    assert!(!stderr.contains("hidden by config"), "{stderr}");
    assert_policy_guidance(&stderr, &route);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

/// #10712 residual: `agent stub` on a policy-omitted seam. A stub is
/// suggestion text, not a repair start, so the stub is still produced
/// (exit 0) with the omission disclosed in `warnings` — the intended split,
/// like the card's informational exit 0 with `repair_ready=false` — while
/// the repair journey it would feed is withheld (`next_repair` stays
/// absent, since the packet would refuse that start).
///
/// The fixture has no reaching tests, so the seam is ungripped: the
/// weak-oracle refusal (which needs a weakly-gripped boundary) cannot fire
/// and the stub producer succeeds, isolating exactly the policy-omission
/// disclosure from every producer refusal.
#[test]
fn policy_omitted_stub_discloses_the_omission_without_a_repair_journey()
-> Result<(), Box<dyn std::error::Error>> {
    let (root, seam_id, class) = unreached_omitted_seam("policy omitted stub")?;
    let root_arg = root.display().to_string();
    // Control: the sibling surfaces refuse this seam as policy-omitted.
    policy_omitted_route(&root_arg, &seam_id)?;
    // Human shape: the stub prints, names the policy, and offers no repair.
    let human = run_ripr(&["agent", "stub", "--root", &root_arg, "--seam-id", &seam_id]);
    assert_eq!(human.status.code(), Some(0), "{human:?}");
    let stdout = String::from_utf8_lossy(&human.stdout).to_string();
    assert!(stdout.contains("Test stub"), "{stdout}");
    assert!(
        stdout.contains(&format!("configured off for {class} seams")),
        "{stdout}"
    );
    assert!(!stdout.contains("Next repair:"), "{stdout}");
    // Machine shape: the ready document carries the omission warning and
    // no repair journey.
    let typed = run_ripr(&[
        "agent",
        "stub",
        "--root",
        &root_arg,
        "--seam-id",
        &seam_id,
        "--json",
    ]);
    assert_eq!(typed.status.code(), Some(0), "{typed:?}");
    let document: serde_json::Value = serde_json::from_slice(&typed.stdout)?;
    assert_eq!(document["state"], "ready", "{document}");
    assert_eq!(document["seam_id"], seam_id, "{document}");
    let warnings = document["warnings"]
        .as_array()
        .ok_or("stub document must carry warnings")?;
    assert!(
        warnings
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|warning| { warning.contains(&format!("configured off for {class} seams")) }),
        "{document}"
    );
    assert!(
        document.get("next_repair").is_none(),
        "a policy-omitted seam must offer no repair journey: {document}"
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}
