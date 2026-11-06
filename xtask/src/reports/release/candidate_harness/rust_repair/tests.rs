//! Real workspace protocol proof only. No trusted installed handle is invented.
use super::*;
use prefix::tests::{capture, workspace_history};
use std::path::PathBuf;

fn manifest() -> Result<Vec<u8>, String> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/blind_journey_installed_rust/manifest.json"),
    )
    .map_err(|e| e.to_string())
}
fn prepare(label: &str) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf, Value), String> {
    let parent = crate::reports::release::external_cli_fixture_root()?;
    fs::create_dir(&parent).map_err(|e| e.to_string())?;
    let root = parent.join(format!("{label} selected é"));
    let foreign = parent.join("foreign decoy");
    workspace_history(&root)?;
    fs::create_dir(&foreign).map_err(|e| e.to_string())?;
    capture(
        Path::new("git"),
        &["init".into(), "--quiet".into(), "--template=".into()],
        &foreign,
        &[],
    )?;
    fs::write(
        foreign.join("Cargo.toml"),
        "[package]\nname='decoy'\nversion='0.1.0'\n",
    )
    .map_err(|e| e.to_string())?;
    let binary = PathBuf::from(crate::tests::ripr_worktree_binary()?);
    let prepared = prefix::public_steps(&root, &foreign, &parent, |args, _| {
        capture(&binary, args, &foreign, &[])
    })?;
    Ok((parent, root, foreign, binary, prepared))
}
fn project(root: &Path, args: &[String]) -> Result<CommandResult, String> {
    capture(
        Path::new("cargo"),
        args,
        root,
        &[
            ("CARGO_BUILD_JOBS", "1"),
            ("CARGO_INCREMENTAL", "0"),
            ("CARGO_NET_OFFLINE", "true"),
            ("CARGO_TERM_COLOR", "never"),
        ],
    )
}
fn finish(parent: &Path) -> Result<(), String> {
    if std::env::var("RIPR_4510_RETAIN_PROOF_FIXTURES").as_deref() == Ok("1") {
        eprintln!(
            "Rust repair workspace proof retained at {}",
            parent.display()
        );
        Ok(())
    } else {
        fs::remove_dir_all(parent).map_err(|e| e.to_string())
    }
}

#[test]
fn workspace_literal_rust_repair_separates_pass_failure_not_run_and_zero_subjects()
-> Result<(), String> {
    for (label, behavior, ready) in [
        ("positive", Behavior::positive(), true),
        (
            "failed",
            Behavior {
                use_public_name: true,
                execute: true,
                expected_value: "standard",
                continue_failed_control: true,
            },
            false,
        ),
        (
            "not-run",
            Behavior {
                use_public_name: true,
                execute: false,
                expected_value: "gold",
                continue_failed_control: true,
            },
            false,
        ),
        (
            "wrong-name",
            Behavior {
                use_public_name: false,
                execute: true,
                expected_value: "gold",
                continue_failed_control: true,
            },
            false,
        ),
    ] {
        let (parent, root, foreign, binary, prepared) = prepare(label)?;
        let before = prefix::inventory(&foreign)?;
        let observed = follow(
            &root,
            &parent,
            &prepared,
            &manifest()?,
            behavior,
            |args| capture(&binary, args, &foreign, &[]),
            |args| project(&root, args),
        )
        .map_err(|e| {
            format!(
                "{e}; real workspace control retained at {}",
                parent.display()
            )
        })?;
        if observed["preparation_ready"] != ready
            || observed["static_movement"] != "improved"
            || observed["receipt_status"] != "advisory"
            || observed["authoritative_closure"] != false
            || observed["product_verification"]["status"] != "verification_not_run"
            || prefix::inventory(&foreign)? != before
        {
            return Err(format!(
                "{label}: conflated runtime/static/receipt/decoy axes; retained {}",
                parent.display()
            ));
        }
        match label {
            "positive" if observed["project_verification"]["counts"]["passed"] == 1 => {}
            "failed"
                if observed["project_verification"]["exit"] == 101
                    && observed["project_verification"]["counts"]["failed"] == 1 => {}
            "not-run" if observed["project_verification"]["status"] == "not_run" => {}
            "wrong-name"
                if observed["project_verification"]["exit"] == 0
                    && observed["project_verification"]["counts"]["executed"] == 0 => {}
            _ => {
                return Err(format!(
                    "{label}: control did not exercise its intended runtime outcome"
                ));
            }
        }
        fs::write(
            parent.join("test-observation.json"),
            serde_json::to_vec_pretty(&observed).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        eprintln!("workspace-only Rust literal {label}: {observed}");
        finish(&parent)?;
    }
    Ok(())
}

#[test]
fn workspace_receipt_refuses_missing_corrupt_and_stale_producer_inputs() -> Result<(), String> {
    let (parent, root, foreign, binary, prepared) = prepare("receipt-controls")?;
    let observed = follow(
        &root,
        &parent,
        &prepared,
        &manifest()?,
        Behavior::positive(),
        |args| capture(&binary, args, &foreign, &[]),
        |args| project(&root, args),
    )?;
    let argv = observed["receipt_argv"]
        .as_array()
        .ok_or("receipt args absent")?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or("non-string receipt arg".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let verify = root.join("target/ripr/workflow/agent-verify.json");
    let original = prefix::plain_bytes(&root, &verify)?;
    let mut refusals = Vec::new();
    for label in ["missing", "corrupt"] {
        if label == "missing" {
            fs::remove_file(&verify).map_err(|e| e.to_string())?;
        } else {
            let mut bad = original.clone();
            bad.extend_from_slice(b"corrupt");
            fs::write(&verify, bad).map_err(|e| e.to_string())?;
        }
        let result = capture(&binary, &argv, &foreign, &[]);
        fs::write(&verify, &original).map_err(|e| e.to_string())?;
        let result = result?;
        record(&parent, label, &argv, &result)?;
        let expected_refusal = if label == "missing" {
            result.stderr.contains("produce it with:")
                && result.stderr.contains("ripr agent verify")
        } else {
            result.stderr.contains("agent receipt verify input")
        };
        if result.success || !expected_refusal {
            return Err(format!(
                "{label} input did not block genuine receipt producer; retained {}",
                parent.display()
            ));
        }
        refusals.push(json!({"control":label,"exit":result.status,"stderr":result.stderr}));
    }
    let restored = capture(&binary, &argv, &foreign, &[])?;
    if !restored.success {
        return Err("restored original verify bytes did not recover receipt producer".into());
    }
    capture(
        Path::new("git"),
        &[
            "-C".into(),
            root.to_string_lossy().into_owned(),
            "-c".into(),
            "user.name=RIPR control".into(),
            "-c".into(),
            "user.email=fixture@example.invalid".into(),
            "-c".into(),
            "core.hooksPath=".into(),
            "commit".into(),
            "--quiet".into(),
            "--allow-empty".into(),
            "--no-gpg-sign".into(),
            "-m".into(),
            "stale receipt control".into(),
        ],
        &foreign,
        &[],
    )?;
    let stale = capture(&binary, &argv, &foreign, &[])?;
    record(&parent, "stale", &argv, &stale)?;
    if stale.success || !stale.stderr.contains("agent receipt verify input") {
        return Err(format!(
            "stale producer artifacts did not refuse; retained {}",
            parent.display()
        ));
    }
    refusals.push(json!({"control":"stale_HEAD","exit":stale.status,"stderr":stale.stderr}));
    let packet = json!({"scope":"workspace_protocol_negative_controls","producer_refusals":refusals,
        "original_verify_bytes_restored":true,"fixture_terminal_state":"HEAD moved; previous receipt is not current",
        "authoritative_closure":false});
    fs::write(
        parent.join("test-observation.json"),
        serde_json::to_vec_pretty(&packet).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    eprintln!("workspace-only Rust receipt controls: {packet}");
    finish(&parent)
}

#[test]
fn project_execution_requires_installed_custody_and_receipt_command_is_root_bound()
-> Result<(), String> {
    let execution = CandidateExecution::Legacy(Path::new("planted-workspace-ripr"));
    if !execution
        .rust_project_cargo(
            Path::new("."),
            &["test".into(), "tier_boundary_discriminator".into()],
        )
        .is_err_and(|e| e.contains("requires installed custody"))
    {
        return Err("Legacy mode reached project verification".into());
    }
    if receipt_argv("ripr agent receipt --root decoy --verify-json decoy/verify --seam-id item --json --out target/ripr/reports/agent-receipt.json","selected","item").is_ok() {
        return Err("foreign-root receipt command accepted".into());
    }
    Ok(())
}

#[test]
fn workspace_cage_refuses_tracked_notes_production_and_bound_packet_writes() -> Result<(), String> {
    for control in ["tracked-notes", "production", "bound-packet"] {
        let (parent, root, foreign, binary, prepared) = prepare("cage-control")?;
        let forbidden = match control {
            "tracked-notes" => "target/notes.rs".to_string(),
            "production" => "src/lib.rs".to_string(),
            _ => {
                let manifest_path = Path::new(
                    prepared["attempt_manifest"]["path"]
                        .as_str()
                        .ok_or("manifest path absent")?,
                );
                manifest_path
                    .parent()
                    .ok_or("manifest parent absent")?
                    .join("artifacts/agent-packet.json")
                    .strip_prefix(&root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .into_owned()
            }
        };
        let path = root.join(&forbidden);
        let mut bytes = if path.exists() {
            prefix::plain_bytes(&root, &path)?
        } else {
            Vec::new()
        };
        if control == "bound-packet" {
            bytes.extend_from_slice(b" ");
        } else {
            bytes.extend_from_slice(b"\n// forbidden user-authored control\n");
        }
        fs::write(path, bytes).map_err(|e| e.to_string())?;
        if control == "tracked-notes" {
            let staged = capture(
                Path::new("git"),
                &[
                    "-C".into(),
                    root.to_string_lossy().into_owned(),
                    "add".into(),
                    "--force".into(),
                    "--".into(),
                    forbidden.clone(),
                ],
                &foreign,
                &[],
            )?;
            if !staged.success {
                return Err("could not stage non-build notes control".into());
            }
        }
        let result = follow(
            &root,
            &parent,
            &prepared,
            &manifest()?,
            Behavior::positive(),
            |args| capture(&binary, args, &foreign, &[]),
            |args| project(&root, args),
        );
        let raw: Value = serde_json::from_slice(
            &fs::read(parent.join("repair-continue.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if result.is_ok()
            || raw["exit"] == 0
            || !raw["stderr"].as_str().is_some_and(|text| {
                text.contains(if control == "bound-packet" {
                    "packet"
                } else {
                    forbidden.as_str()
                })
            })
            || root.join("target/ripr/reports/agent-receipt.json").exists()
        {
            return Err(format!(
                "{forbidden} did not fail its actual cage transition; retained {}",
                parent.display()
            ));
        }
        eprintln!("workspace-only forbidden write {forbidden}: {raw}");
        finish(&parent)?;
    }
    Ok(())
}
