use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};

const SOURCE: &str = include_str!("data/catalog_wording/lib.rs");
const TEST: &str = include_str!("data/catalog_wording/catalog.rs");
const DIFF: &str = include_str!("data/catalog_wording/diff.patch");
const MANIFEST: &str = include_str!("data/catalog_wording/Cargo.toml");

fn check(test: &str) -> Result<String, String> {
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    impl Scratch {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = Scratch(std::env::temp_dir().join(format!(
        "ripr-catalog-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    std::fs::create_dir(&dir.0).map_err(|e| e.to_string())?;
    std::fs::create_dir(dir.path().join("src")).map_err(|e| e.to_string())?;
    std::fs::create_dir(dir.path().join("tests")).map_err(|e| e.to_string())?;
    std::fs::write(dir.path().join("Cargo.toml"), MANIFEST).map_err(|e| e.to_string())?;
    std::fs::write(dir.path().join("src/lib.rs"), SOURCE).map_err(|e| e.to_string())?;
    std::fs::write(dir.path().join("tests/catalog.rs"), test).map_err(|e| e.to_string())?;
    let diff = dir.path().join("change.diff");
    std::fs::write(&diff, DIFF).map_err(|e| e.to_string())?;
    let report = check_workspace(CheckInput {
        root: dir.path().into(),
        diff_file: Some(diff),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: serde_json::Value =
        serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
            .map_err(|e| e.to_string())?;
    let findings = json["findings"].as_array().ok_or("missing findings")?;
    assert_eq!(findings.len(), 1, "{json}");
    assert_eq!(
        findings[0]["canonical_gap"]["owner"],
        "default_check_description"
    );
    Ok(findings[0]["classification"]
        .as_str()
        .ok_or("missing classification")?
        .into())
}

#[test]
fn structural_observer_does_not_pin_wording() -> Result<(), String> {
    assert_eq!(check(TEST)?, "weakly_exposed");
    Ok(())
}

#[test]
fn direct_wording_observer_remains_exposed() -> Result<(), String> {
    let test = TEST.replace(
        "let _ = default_check_description();",
        "assert_eq!(default_check_description(), \"verifies catalog consistency\");",
    );
    assert_ne!(test, TEST);
    assert_eq!(check(&test)?, "exposed");
    Ok(())
}
