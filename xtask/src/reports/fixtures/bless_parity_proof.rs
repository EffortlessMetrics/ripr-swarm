//! Exact-candidate native proof transport, intentionally separate from batch-cache qualification.
use super::*;
use sha2::{Digest, Sha256};

const SURFACES: [&str; 6] = [
    "check.json",
    "human.txt",
    "human-full.txt",
    "sarif.json",
    "github.txt",
    "CHANGELOG.md",
];
const BASH: &str = "  Verify command (Bash): (cd -P -- <cwd>/fixtures/py/input && python -m pytest tests/test_a.py::test_one) (high)\n";
const MANUAL: &str = "  Suggested command (unverified; inspect and run from the selected root): python -m pytest tests/test_a.py::test_one\n";

fn setup(windows_output: bool) -> Result<(PathBuf, PathBuf, FixtureRun), String> {
    let (root, expected, run) = super::tests::parity_bless_fixture("native-parity")?;
    for surface in ["human.txt", "human-full.txt", "github.txt"] {
        let golden = if surface == "human-full.txt" {
            BASH.replace("Verify command", "verify")
        } else {
            BASH.to_string()
        };
        let actual = if windows_output {
            if surface == "human-full.txt" {
                MANUAL.replace("Suggested command", "suggested command")
            } else {
                MANUAL.to_string()
            }
        } else {
            golden.clone()
        };
        fs::write(expected.join(surface), golden).map_err(|e| e.to_string())?;
        fs::write(run.actual_dir.join(surface), actual).map_err(|e| e.to_string())?;
    }
    Ok((root, expected, run))
}

fn hashes(dir: &Path) -> Result<BTreeMap<String, String>, String> {
    SURFACES
        .iter()
        .map(|name| {
            let bytes = fs::read(dir.join(name)).map_err(|e| e.to_string())?;
            Ok((name.to_string(), format!("{:x}", Sha256::digest(bytes))))
        })
        .collect()
}

fn identity() -> Result<(PathBuf, String), String> {
    let root = std::env::var("RIPR_BLESS_PROOF_DIR").map_err(|e| e.to_string())?;
    let sha = std::env::var("RIPR_BLESS_PROOF_SHA").map_err(|e| e.to_string())?;
    if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("proof requires an exact 40-hex candidate SHA".to_string());
    }
    Ok((PathBuf::from(root), sha))
}

#[test]
#[ignore = "requires the native Windows artifact producer job"]
fn native_windows_export() -> Result<(), String> {
    if !cfg!(windows) {
        return Err("must execute on native Windows".to_string());
    }
    let (artifact, sha) = identity()?;
    assert!(!artifact.exists(), "refuse stale artifact directory");
    let (root, expected, run) = setup(true)?;
    let before = hashes(&expected)?;
    for _ in 0..2 {
        for surface in &SURFACES[..5] {
            assert!(
                compare_golden(
                    "py",
                    surface,
                    &expected.join(surface),
                    &run.actual_dir.join(surface)
                )?
                .matches
            );
        }
        let applied = apply_bless_outputs(&expected, &run, "py", "native no-op")?;
        assert!(applied.changed.is_empty());
        assert!(bless_report_body(&root, "native no-op", &run, &applied).contains("No change:"));
        assert_eq!(hashes(&expected)?, before);
    }
    fs::write(&run.check_json, "{\"findings\":[{\"changed\":true}]}").map_err(|e| e.to_string())?;
    assert!(
        !compare_golden(
            "py",
            "check.json",
            &expected.join("check.json"),
            &run.check_json
        )?
        .matches
    );
    let refusal = apply_bless_outputs(&expected, &run, "py", "native change");
    assert!(matches!(refusal, Err(ref e) if e.contains("before writing") && e.contains("Linux")));
    assert_eq!(hashes(&expected)?, before);
    let missing = root.join("missing");
    assert!(apply_bless_outputs(&missing, &run, "py", "native missing",).is_err());
    assert!(!missing.exists());
    fs::create_dir_all(artifact.join("expected")).map_err(|e| e.to_string())?;
    for surface in SURFACES {
        fs::copy(
            expected.join(surface),
            artifact.join("expected").join(surface),
        )
        .map_err(|e| e.to_string())?;
    }
    let receipt = serde_json::json!({"candidate": sha, "producer_os": "windows", "noop_runs": 2, "refusal_checked": true, "hashes": before});
    fs::write(
        artifact.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::remove_dir_all(root).map_err(|e| e.to_string())
}

fn validate_receipt(artifact: &Path, sha: &str, canonical: &Path) -> Result<(), String> {
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(artifact.join("receipt.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if receipt["candidate"] != sha
        || receipt["producer_os"] != "windows"
        || receipt["noop_runs"] != 2
        || receipt["refusal_checked"] != true
    {
        return Err("wrong candidate or incomplete Windows receipt".to_string());
    }
    let actual = hashes(&artifact.join("expected"))?;
    if serde_json::to_value(&actual).map_err(|e| e.to_string())? != receipt["hashes"]
        || actual != hashes(canonical)?
    {
        return Err("transferred bytes differ from receipt or canonical Linux bytes".to_string());
    }
    Ok(())
}

#[test]
#[ignore = "requires the transferred native Windows artifact"]
fn linux_recheck_windows_artifact() -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("must execute on Linux".to_string());
    }
    let (artifact, sha) = identity()?;
    let (root, canonical, run) = setup(false)?;
    validate_receipt(&artifact, &sha, &canonical)?;
    let expected = artifact.join("expected");
    let before = hashes(&expected)?;
    for surface in &SURFACES[..5] {
        assert!(
            compare_golden(
                "py",
                surface,
                &expected.join(surface),
                &run.actual_dir.join(surface)
            )?
            .matches
        );
    }
    assert!(
        apply_bless_outputs(&expected, &run, "py", "Linux recheck")?
            .changed
            .is_empty()
    );
    assert_eq!(hashes(&expected)?, before);
    // Negative controls reject a mismatched subject and changed transferred bytes.
    assert!(validate_receipt(&artifact, &"0".repeat(40), &canonical).is_err());
    fs::write(expected.join("human.txt"), "tampered\n").map_err(|e| e.to_string())?;
    assert!(validate_receipt(&artifact, &sha, &canonical).is_err());
    fs::copy(canonical.join("human.txt"), expected.join("human.txt")).map_err(|e| e.to_string())?;
    validate_receipt(&artifact, &sha, &canonical)?;
    fs::remove_dir_all(root).map_err(|e| e.to_string())
}

#[test]
fn receipt_controls_reject_wrong_identity_and_forged_bytes() -> Result<(), String> {
    // Instrument control only: this synthetic receipt is not Windows execution.
    let (root, canonical, _) = setup(false)?;
    let artifact = root.join("synthetic-transfer");
    fs::create_dir_all(artifact.join("expected")).map_err(|e| e.to_string())?;
    for surface in SURFACES {
        fs::copy(
            canonical.join(surface),
            artifact.join("expected").join(surface),
        )
        .map_err(|e| e.to_string())?;
    }
    let sha = "1".repeat(40);
    let mut receipt = serde_json::json!({"candidate": sha, "producer_os": "windows", "noop_runs": 2, "refusal_checked": true, "hashes": hashes(&canonical)?});
    let write_receipt = |receipt: &serde_json::Value| -> Result<(), String> {
        fs::write(
            artifact.join("receipt.json"),
            serde_json::to_vec(receipt).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    };
    write_receipt(&receipt)?;
    validate_receipt(&artifact, &sha, &canonical)?;
    assert!(validate_receipt(&artifact, &"2".repeat(40), &canonical).is_err());
    fs::write(
        artifact.join("expected/human.txt"),
        "wrong canonical bytes\n",
    )
    .map_err(|e| e.to_string())?;
    assert!(validate_receipt(&artifact, &sha, &canonical).is_err());
    receipt["hashes"] =
        serde_json::to_value(hashes(&artifact.join("expected"))?).map_err(|e| e.to_string())?;
    write_receipt(&receipt)?;
    assert!(
        validate_receipt(&artifact, &sha, &canonical).is_err(),
        "a matching forged receipt cannot authorize changed canonical bytes"
    );
    fs::remove_file(artifact.join("expected/CHANGELOG.md")).map_err(|e| e.to_string())?;
    assert!(validate_receipt(&artifact, &sha, &canonical).is_err());
    fs::remove_dir_all(root).map_err(|e| e.to_string())
}
