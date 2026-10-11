use super::{
    init_git_fixture_repo, printed_ripr_args, run_command, run_git, unique_temp_workspace,
};
use std::path::Path;
use std::process::Output;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn replay(command: &str, cwd: &Path) -> Result<Output, Box<dyn std::error::Error>> {
    let args = printed_ripr_args(command)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    Ok(run_command(env!("CARGO_BIN_EXE_ripr"), Some(cwd), &args)?)
}

/// The missing-ledger refusal names real producers. Follow its Rust capture
/// and ledger commands from a foreign CWD, including a quoted root. The
/// fixture is an ordinary pricing crate with independent literal assertions.
#[test]
fn rerun_missing_gap_ledger_recovery_executes_the_rust_producer() -> TestResult {
    let scratch = unique_temp_workspace("rerun-ledger-recovery");
    let root = scratch.join("ordinary shop");
    let foreign = scratch.join("foreign");
    let result: TestResult = (|| {
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::create_dir_all(root.join("tests"))?;
        std::fs::create_dir_all(&foreign)?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"ordinary_shop\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn discounted_total(amount: i32, threshold: i32) -> i32 {\n    if amount > threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n",
        )?;
        std::fs::write(
            root.join("tests/pricing.rs"),
            "use ordinary_shop::discounted_total;\n#[test]\nfn below_threshold_has_no_discount() {\n    assert_eq!(discounted_total(50, 100), 50);\n}\n#[test]\nfn above_threshold_discounts() {\n    assert_eq!(discounted_total(1_000, 100), 990);\n}\n",
        )?;
        init_git_fixture_repo(&root)?;
        run_git(&root, &["add", "Cargo.toml", "src", "tests"])?;
        run_git(
            &root,
            &[
                "-c",
                "user.name=RIPR test",
                "-c",
                "user.email=ripr@example.invalid",
                "commit",
                "-m",
                "pricing crate",
            ],
        )?;
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn discounted_total(amount: i32, threshold: i32) -> i32 {\n    if amount >= threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n",
        )?;
        let root_arg = root.display().to_string();
        let bin = env!("CARGO_BIN_EXE_ripr");
        // Literal identity for this retained repo-inventory predicate. Do not
        // derive the expectation with the production canonical-ID function.
        let gap = "gap:54c07ec8eeadf203";
        let inventory = run_command(
            bin,
            Some(&foreign),
            &[
                "check",
                "--root",
                &root_arg,
                "--format",
                "repo-exposure-json",
            ],
        )?;
        assert!(
            inventory.status.success(),
            "fixture inventory failed: {inventory:?}"
        );
        let inventory_json: serde_json::Value = serde_json::from_slice(&inventory.stdout)?;
        assert_eq!(
            inventory_json["seams"]
                .as_array()
                .ok_or("missing seams")?
                .len(),
            1
        );
        assert_eq!(
            inventory_json["seams"][0]["evidence_record"]["canonical_gap_id"],
            gap
        );

        let refused = run_command(
            bin,
            Some(&foreign),
            &["rerun", "--root", &root_arg, "--gap", gap],
        )?;
        assert_eq!(refused.status.code(), Some(2));
        assert!(refused.stdout.is_empty());
        assert!(
            !foreign.join("gap-ledger.json").exists(),
            "refusal must not create a ledger"
        );
        let stderr = String::from_utf8(refused.stderr)?;
        assert!(stderr.contains("rerun --gap requires --gap-ledger <path>"));
        assert!(
            stderr.contains("save stdout as repo-exposure.json"),
            "missing capture instruction: {stderr}"
        );
        assert!(
            stderr.contains("--check-output check.json"),
            "missing TypeScript producer: {stderr}"
        );
        assert!(
            stderr.contains("the ledger must contain the selected canonical gap ID"),
            "missing identity boundary: {stderr}"
        );
        let capture = stderr
            .split('`')
            .find(|part| part.starts_with("ripr check "))
            .ok_or("missing capture command")?;
        let producer = stderr
            .split('`')
            .find(|part| {
                part.starts_with("ripr reports gap-ledger ") && part.contains("--repo-exposure")
            })
            .ok_or("missing Rust ledger producer")?;
        // Independent literal argv expectations reject a wrong producer,
        // source kind, destination, or root even if it prints plausible text.
        let bound_root = if cfg!(windows) {
            root_arg.replace('\\', "/")
        } else {
            root_arg.clone()
        };
        assert_eq!(
            printed_ripr_args(capture)?,
            [
                "check",
                "--root",
                &bound_root,
                "--format",
                "repo-exposure-json"
            ]
        );
        assert_eq!(
            printed_ripr_args(producer)?,
            [
                "reports",
                "gap-ledger",
                "--root",
                &bound_root,
                "--repo-exposure",
                "repo-exposure.json",
                "--out",
                "gap-ledger.json"
            ]
        );
        let captured = replay(capture, &foreign)?;
        assert!(
            captured.status.success(),
            "printed capture failed: {captured:?}"
        );
        // Saving stdout is the explicit instruction, without shell-specific
        // redirect encoding or an invented check --out flag.
        std::fs::write(foreign.join("repo-exposure.json"), captured.stdout)?;
        let produced = replay(producer, &foreign)?;
        assert!(
            produced.status.success(),
            "printed producer failed: {produced:?}"
        );
        let ledger: serde_json::Value =
            serde_json::from_slice(&std::fs::read(foreign.join("gap-ledger.json"))?)?;
        assert_eq!(
            ledger["records"].as_array().ok_or("missing records")?.len(),
            1
        );
        assert_eq!(ledger["records"][0]["canonical_gap_id"], gap);
        assert!(stderr.contains("retry with `--gap-ledger gap-ledger.json`"));
        let rerun = run_command(
            bin,
            Some(&foreign),
            &[
                "rerun",
                "--root",
                &root_arg,
                "--gap",
                gap,
                "--gap-ledger",
                "gap-ledger.json",
                "--json",
            ],
        )?;
        assert!(rerun.status.success(), "recovered rerun failed: {rerun:?}");
        let selected: serde_json::Value = serde_json::from_slice(&rerun.stdout)?;
        assert_eq!(selected["state"], "current_state_only");
        assert_eq!(selected["selector"]["canonical_gap_id"], gap);
        assert_eq!(selected["selector"]["matched_record_count"], 1);
        assert_eq!(selected["selector"]["recomputed_scope_count"], 1);
        assert_eq!(
            selected["seams"]
                .as_array()
                .ok_or("missing selected seams")?
                .len(),
            1
        );

        // Capturing a repo ledger does not translate diff finding identities.
        let absent =
            "gap:rust:src/lib.rs:discounted_total:predicate_boundary:predicate:amount==threshold";
        let missing = run_command(
            bin,
            Some(&foreign),
            &[
                "rerun",
                "--root",
                &root_arg,
                "--gap",
                absent,
                "--gap-ledger",
                "gap-ledger.json",
                "--json",
            ],
        )?;
        assert!(
            missing.status.success(),
            "missing ID must return a typed limitation: {missing:?}"
        );
        let limited: serde_json::Value = serde_json::from_slice(&missing.stdout)?;
        assert_eq!(limited["state"], "limited");
        assert_eq!(limited["selector"]["matched_record_count"], 0);
        assert_eq!(limited["limitation"]["kind"], "canonical_gap_unresolved");
        assert_eq!(limited["seams"], serde_json::json!([]));
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&scratch);
    result?;
    cleanup?;
    Ok(())
}

#[test]
fn rerun_missing_gap_ledger_help_names_source_specific_capture_steps() -> TestResult {
    let help = run_command(env!("CARGO_BIN_EXE_ripr"), None, &["help", "rerun"])?;
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout)?;
    for expected in [
        "ripr check --format repo-exposure-json",
        "ripr reports gap-ledger --repo-exposure repo-exposure.json --out gap-ledger.json",
        "ripr reports gap-ledger --check-output check.json --out gap-ledger.json",
        "the ledger must contain the selected canonical gap ID",
    ] {
        assert!(
            text.contains(expected),
            "rerun help must contain {expected:?}: {text}"
        );
    }
    Ok(())
}
