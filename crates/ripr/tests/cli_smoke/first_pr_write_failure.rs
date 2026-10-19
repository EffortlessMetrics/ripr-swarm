use super::{
    assert_success, ignore_remove_dir_all, init_producer_fixture_repo, spawn_command,
    unique_temp_workspace, workspace_root,
};
use std::path::{Path, PathBuf};
use std::process::Output;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Fixture(PathBuf);

impl Fixture {
    fn new(label: &str) -> TestResult<Self> {
        let root = unique_temp_workspace(label);
        std::fs::create_dir_all(root.parent().ok_or("fixture needs parent")?)?;
        std::fs::create_dir(&root)?;
        let fixture = Self(root);
        init_producer_fixture_repo(&fixture.0)?;
        Ok(fixture)
    }

    fn cli(&self, args: &[String]) -> TestResult<Output> {
        let refs: Vec<_> = args.iter().map(String::as_str).collect();
        Ok(spawn_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&self.0),
            &refs,
            &[],
            None,
            Some(b""),
        )?)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        ignore_remove_dir_all(&self.0);
    }
}

fn no_stages(directory: &Path) -> TestResult {
    for entry in std::fs::read_dir(directory)? {
        let name = entry?.file_name();
        assert!(
            !name.to_string_lossy().starts_with(".ripr-atomic-"),
            "failed write stranded staging file {name:?}"
        );
    }
    Ok(())
}

fn failed_write_and_identical_retry(derived_ledger: bool) -> TestResult {
    let fixture = Fixture::new(if derived_ledger {
        "first-pr-ledger-write"
    } else {
        "first-pr-packet-write"
    })?;
    let reports = fixture.0.join("target/ripr/reports");
    // Keep preflight's directory-existence observation identical for the
    // complete setup report, failed write, and repaired retry.
    std::fs::create_dir_all(&reports)?;
    let ledger = reports.join("gap-decision-ledger.json");
    let mut args = vec![
        "first-pr".to_string(),
        "--root".to_string(),
        fixture.0.display().to_string(),
        "--base".to_string(),
        "HEAD".to_string(),
        "--head".to_string(),
        "HEAD".to_string(),
        "--out-dir".to_string(),
        reports.display().to_string(),
        "--gap-ledger".to_string(),
        ledger.display().to_string(),
    ];
    if derived_ledger {
        let check = fixture.cli(&[
            "check".to_string(),
            "--root".to_string(),
            fixture.0.display().to_string(),
            "--diff".to_string(),
            workspace_root()
                .join("fixtures/boundary_gap/diff.patch")
                .display()
                .to_string(),
            "--format".to_string(),
            "json".to_string(),
        ])?;
        assert_success(&check);
        let parsed: serde_json::Value = serde_json::from_slice(&check.stdout)?;
        assert!(
            parsed["findings"]
                .as_array()
                .is_some_and(|findings| !findings.is_empty()),
            "check fixture must contain findings: {parsed}"
        );
        let input = fixture.0.join("check.json");
        std::fs::write(&input, check.stdout)?;
        args.extend(["--check-output".to_string(), input.display().to_string()]);
    }

    // Capture a complete real report before injecting a write fault. This is
    // a publication oracle, not a golden for the renderer's full semantics.
    assert_success(&fixture.cli(&args)?);
    let target = if derived_ledger {
        ledger
    } else {
        reports.join("start-here.json")
    };
    let complete = std::fs::read(&target)?;
    assert!(
        complete.len() > 1024,
        "target must exceed either common shell's ulimit unit"
    );
    let parsed: serde_json::Value = serde_json::from_slice(&complete)?;
    assert_eq!(parsed["schema_version"], "0.1");
    assert_eq!(parsed["tool"], "ripr");
    assert_eq!(
        parsed["kind"],
        if derived_ledger {
            "gap_decision_ledger"
        } else {
            "first_pr_start_here"
        }
    );
    if derived_ledger {
        // This Rust check-output route has no projected gap records. The
        // write subject is still a complete, oversized report.
        assert_eq!(parsed["status"], "no_records");
        assert_eq!(parsed["records"], serde_json::json!([]));
    } else {
        assert_eq!(parsed["posture"], "advisory");
    }

    // Whitespace is valid JSON. Its distinct bytes reject a no-op retry that
    // merely returns success without replacing a previous valid report.
    let mut previous = complete.clone();
    previous.extend_from_slice(b" \n\n");
    serde_json::from_slice::<serde_json::Value>(&previous)?;
    assert_ne!(previous, complete);
    let refs: Vec<_> = args.iter().map(String::as_str).collect();
    let mut limited = vec![
        "-c",
        "trap '' XFSZ; ulimit -f 1 && exec \"$0\" \"$@\"",
        env!("CARGO_BIN_EXE_ripr"),
    ];
    limited.extend(refs);
    for existing in [true, false] {
        if existing {
            std::fs::write(&target, &previous)?;
        } else {
            std::fs::remove_file(&target)?;
        }
        let failed = spawn_command(
            "sh",
            Some(&fixture.0),
            &limited,
            &[("LLVM_PROFILE_FILE", "/dev/null")],
            None,
            Some(b""),
        )?;
        let stderr = String::from_utf8_lossy(&failed.stderr);
        assert_eq!(
            failed.status.code(),
            Some(2),
            "write error must be handled: {failed:?}"
        );
        assert!(
            stderr.contains(&format!("failed to write {}:", target.display())),
            "wrong failure path: {stderr}"
        );
        if existing {
            assert_eq!(
                std::fs::read(&target)?,
                previous,
                "failed write destroyed the previous report"
            );
        } else {
            assert!(
                !target.try_exists()?,
                "failed write published a partial report"
            );
        }
        no_stages(&reports)?;

        // Same product argv and input bytes; only the child write limit is
        // removed. A successful repair must publish all the expected bytes.
        assert_success(&fixture.cli(&args)?);
        assert_eq!(
            std::fs::read(&target)?,
            complete,
            "retry did not publish the complete report"
        );
        no_stages(&reports)?;
    }
    Ok(())
}

#[test]
fn first_pr_packet_failed_write_preserves_destination_and_retries() -> TestResult {
    failed_write_and_identical_retry(false)
}

#[test]
fn first_pr_derived_ledger_failed_write_preserves_destination_and_retries() -> TestResult {
    failed_write_and_identical_retry(true)
}
