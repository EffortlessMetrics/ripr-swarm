use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

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
    let test = format!(
        "{}assert_eq!(default_check_description(), \"verifies catalog consistency\");\n}}\n",
        TEST.split_once("    let _ = default_check_description();")
            .ok_or("missing test body")?
            .0
    );
    assert_ne!(test, TEST);
    assert_eq!(check(&test)?, "exposed");
    Ok(())
}

#[test]
fn native_mutants_separate_structural_and_value_observers() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!("ripr-catalog-native-{}", std::process::id()));
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let import = "use rust_catalog_description_structural_oracle::{\n    build_catalog, default_check_description, entry_description_violations,\n};";
    assert!(TEST.contains(import));
    let body = TEST.replace(import, "use super::*;");
    let direct = body.replace(
        "let _ = default_check_description();",
        "assert_eq!(default_check_description(), \"verifies catalog consistency\");",
    );
    let length = body.replace(
        "let _ = default_check_description();",
        "assert_eq!(default_check_description().len(), 28);",
    );
    let unrelated = body.replace(
        "let _ = default_check_description();",
        "let _ = default_check_description();\n assert_eq!(\"verifies\", \"verifies\");",
    );
    let diagnostic = body.replace("Vec::<String>::new()", "Vec::<String>::new(), \"verifies\"");
    assert_ne!(unrelated, body);
    assert_ne!(diagnostic, body);
    assert_ne!(direct, body);
    assert_ne!(length, body);
    for (label, test, catches) in [
        ("structural", &body, false),
        ("unrelated", &unrelated, false),
        ("diagnostic", &diagnostic, false),
        ("direct", &direct, true),
        ("length", &length, true),
    ] {
        for (wording, fails) in [
            ("verifies catalog consistency", false),
            ("checks catalog consistency", catches),
            ("xxxxxxxxxxxxxxxxxxxxxxxxxxxx", label == "direct"),
            ("", true),
        ] {
            let source = SOURCE.replace("verifies catalog consistency", wording);
            let path = root.join("fixture.rs");
            std::fs::write(
                &path,
                format!("{source}\n#[cfg(test)] mod tests {{ {test} }}"),
            )
            .map_err(|e| e.to_string())?;
            let binary = root.join("fixture-bin");
            let mut compile = std::process::Command::new("rustc");
            compile
                .args(["--edition=2024", "--test"])
                .arg(&path)
                .arg("-o")
                .arg(&binary);
            let status = run_bounded(compile, &root, "compile", Duration::from_mins(2))?.status;
            assert!(status.success(), "{label}: compilation");
            let mut execute = Command::new(&binary);
            execute.args([
                "--exact",
                "tests::catalog_with_single_consistent_entry_has_no_violations",
            ]);
            let output = run_bounded(execute, &root, "execute", Duration::from_secs(10))?;
            assert!(
                String::from_utf8_lossy(&output.stdout).contains("running 1 test"),
                "{label}: missing test"
            );
            assert_eq!(
                !output.status.success(),
                fails,
                "{label}, {wording}: {}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
    }
    Ok(())
}

#[test]
fn length_observer_remains_exposed() -> Result<(), String> {
    let test = format!(
        "{}assert_eq!(default_check_description().len(), 28);\n}}\n",
        TEST.split_once("    let _ = default_check_description();")
            .ok_or("missing test body")?
            .0
    );
    assert_ne!(test, TEST);
    assert_eq!(check(&test)?, "exposed");
    Ok(())
}

const STREAM_LIMIT: u64 = 64 * 1024;

fn read_bounded_stream(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(STREAM_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > STREAM_LIMIT {
        return Err(format!(
            "stream {} exceeds {STREAM_LIMIT} bytes",
            path.display()
        ));
    }
    Ok(bytes)
}

// File-backed streams avoid pipe backpressure. The shared process owner
// terminates and reaps on timeout before Scratch removes its owned directory.
fn run_bounded(
    mut command: Command,
    root: &Path,
    label: &str,
    budget: Duration,
) -> Result<Output, String> {
    let stdout = root.join(format!("{label}.stdout"));
    let stderr = root.join(format!("{label}.stderr"));
    command
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&stdout).map_err(|error| error.to_string())?)
        .stderr(std::fs::File::create(&stderr).map_err(|error| error.to_string())?);
    let mut child =
        ripr::process_owner::OwnedProcess::spawn(command).map_err(|error| error.to_string())?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() >= budget {
            child.terminate_tree()?;
            return Err(format!(
                "{label} exceeded its {budget:?} budget; owned process terminated and reaped"
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Ok(Output {
        status,
        stdout: read_bounded_stream(&stdout)?,
        stderr: read_bounded_stream(&stderr)?,
    })
}

#[test]
fn changed_word_in_diagnostic_does_not_confirm_structural_observer() -> Result<(), String> {
    let test = TEST.replace("Vec::<String>::new()", "Vec::<String>::new(), \"verifies\"");
    assert_ne!(test, TEST);
    assert_eq!(check(&test)?, "weakly_exposed");
    Ok(())
}

#[test]
fn unrelated_changed_word_does_not_confirm_structural_observer() -> Result<(), String> {
    let test = TEST.replace(
        "let _ = default_check_description();",
        "let _ = default_check_description();\n    assert_eq!(\"verifies\", \"verifies\");",
    );
    assert_ne!(test, TEST);
    assert_eq!(check(&test)?, "weakly_exposed");
    Ok(())
}
