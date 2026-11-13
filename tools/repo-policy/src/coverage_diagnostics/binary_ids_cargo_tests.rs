//! Cargo 1.100's finite package/unit/out compatibility boundary.
use super::*;

const UNIT: &str = "debug/build/coverage-fixture/0123456789abcdef/out";

fn unit_executable(root: &Path, relative: &str, bytes: &[u8]) -> Result<PathBuf> {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().ok_or("parent missing")?).map_err(|e| e.to_string())?;
    write_executable(&path, bytes)?;
    Ok(path)
}

#[test]
fn cargo_unit_outputs_match_exact_ids_and_preserve_ambiguity() -> Result<()> {
    let f = fixture()?;
    let id = [0x42; 20];
    let executable = unit_executable(
        &f.0,
        &format!("{UNIT}/coverage_fixture-0123456789abcdef"),
        &elf_bytes(&id, true),
    )?;
    let first = associate_with_limits(&f.0, &ids(&id), LIMITS);
    assert_eq!(first["ids"][0]["status"], "matched");
    assert_eq!(first["ids"][0]["ambiguous"], false);
    assert_eq!(first["candidates"].as_array().map(Vec::len), Some(1));
    assert_eq!(first["files_inspected"], 1);

    fs::hard_link(&executable, f.0.join("debug/coverage-fixture")).map_err(|e| e.to_string())?;
    unit_executable(
        &f.0,
        "release/build/coverage-fixture/fedcba9876543210/out/coverage_fixture-fedcba9876543210",
        &elf_bytes(&id, true),
    )?;
    let duplicated = associate_with_limits(&f.0, &ids(&id), LIMITS);
    assert_eq!(duplicated["ids"][0]["status"], "matched");
    assert_eq!(duplicated["ids"][0]["ambiguous"], true);
    assert_eq!(duplicated["candidates"].as_array().map(Vec::len), Some(2));
    assert_eq!(duplicated["files_inspected"], 3);
    Ok(())
}

#[test]
fn cargo_unit_discovery_never_descends_outside_supported_layout() -> Result<()> {
    let f = fixture()?;
    let id = [0x42; 20];
    for relative in [
        "debug/build/coverage-fixture/not-a-unit/out/invalid-hash",
        "debug/build/coverage-fixture/0123456789abcdef/run/wrong-output-root",
        "debug/build/coverage-fixture/0123456789abcdef/out/child/grandchild",
        "other/build/coverage-fixture/0123456789abcdef/out/outside-profile",
    ] {
        unit_executable(&f.0, relative, &elf_bytes(&id, true))?;
    }
    unit_executable(
        &f.0,
        &format!("{UNIT}/uninstrumented"),
        &elf_bytes(&id, false),
    )?;
    unit_executable(
        &f.0,
        &format!("{UNIT}/wrong-id"),
        &elf_bytes(&[0x41; 20], true),
    )?;
    let report = associate_with_limits(&f.0, &ids(&id), LIMITS);
    assert_eq!(report["candidates"], json!([]));
    assert_eq!(report["status"], "inspected_scope");
    assert_eq!(
        report["ids"][0]["status"],
        "not_found_within_inspected_scope"
    );
    assert_eq!(report["files_inspected"], 2);
    assert_eq!(report["non_candidates"], 1);
    Ok(())
}

#[test]
fn cargo_unit_discovery_refuses_symlinks_at_every_level() -> Result<()> {
    let outside = fixture()?;
    let id = [0x42; 20];
    unit_executable(
        &outside.0,
        &format!("{UNIT}/subject"),
        &elf_bytes(&id, true),
    )?;
    for relative in [
        "debug/build",
        "debug/build/coverage-fixture",
        "debug/build/coverage-fixture/0123456789abcdef",
        UNIT,
        "debug/build/coverage-fixture/0123456789abcdef/out/subject",
    ] {
        let f = fixture()?;
        let link = f.0.join(relative);
        fs::create_dir_all(link.parent().ok_or("parent missing")?).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(outside.0.join(relative), link).map_err(|e| e.to_string())?;
        let report = associate_with_limits(&f.0, &ids(&id), LIMITS);
        assert_eq!(report["candidates"], json!([]), "{relative}: {report}");
        assert_eq!(report["status"], "partial", "{relative}: {report}");
        assert_eq!(
            report["ids"][0]["status"],
            "unavailable_within_partial_scope"
        );
    }
    Ok(())
}

#[test]
fn cargo_unit_discovery_shares_scan_caps() -> Result<()> {
    let f = fixture()?;
    let id = [0x42; 20];
    for n in 0..4 {
        unit_executable(&f.0, &format!("{UNIT}/subject-{n}"), &elf_bytes(&id, true))?;
    }
    for limited in [
        Limits {
            entries: 5,
            ..LIMITS
        },
        Limits { files: 1, ..LIMITS },
        Limits {
            bytes: 63,
            ..LIMITS
        },
        Limits {
            matches: 1,
            ..LIMITS
        },
        Limits {
            elapsed: Duration::ZERO,
            ..LIMITS
        },
    ] {
        let report = associate_with_limits(&f.0, &ids(&id), limited);
        assert_eq!(report["truncated"], true, "{report}");
        assert_ne!(report["status"], "inspected_scope");
        assert!(report["entries_seen"].as_u64().unwrap_or(0) <= limited.entries as u64);
        assert!(report["files_inspected"].as_u64().unwrap_or(0) <= limited.files as u64);
        assert!(report["read_bytes"].as_u64().unwrap_or(0) <= limited.bytes as u64);
        assert!(report["candidates"].as_array().map_or(0, Vec::len) <= limited.matches);
    }
    Ok(())
}

#[test]
fn cargo_unit_discovery_bounds_unavailable_root_samples() -> Result<()> {
    let f = fixture()?;
    for n in 0..12 {
        fs::create_dir_all(f.0.join(format!("debug/build/package-{n}/0123456789abcdef")))
            .map_err(|e| e.to_string())?;
    }
    let report = associate_with_limits(&f.0, &ids(&[0x42; 20]), LIMITS);
    assert_eq!(report["status"], "partial");
    assert_eq!(report["roots_unavailable_count"], 12);
    assert_eq!(
        report["roots_unavailable"].as_array().map(Vec::len),
        Some(8)
    );
    assert_eq!(
        report["ids"][0]["status"],
        "unavailable_within_partial_scope"
    );
    Ok(())
}
