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
    check_source(SOURCE, test)
}

fn check_report(
    source: &str,
    test: &str,
    manifest: &str,
    diff_text: &str,
) -> Result<serde_json::Value, String> {
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
    std::fs::create_dir_all(std::env::temp_dir()).map_err(|e| e.to_string())?;
    std::fs::create_dir(&dir.0).map_err(|e| e.to_string())?;
    std::fs::create_dir(dir.path().join("src")).map_err(|e| e.to_string())?;
    std::fs::create_dir(dir.path().join("tests")).map_err(|e| e.to_string())?;
    std::fs::write(dir.path().join("Cargo.toml"), manifest).map_err(|e| e.to_string())?;
    std::fs::write(dir.path().join("src/lib.rs"), source).map_err(|e| e.to_string())?;
    std::fs::write(dir.path().join("tests/catalog.rs"), test).map_err(|e| e.to_string())?;
    let diff = dir.path().join("change.diff");
    std::fs::write(&diff, diff_text).map_err(|e| e.to_string())?;
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
    Ok(json)
}

fn check_source(source: &str, test: &str) -> Result<String, String> {
    let json = check_report(source, test, MANIFEST, DIFF)?;
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
    std::fs::create_dir_all(std::env::temp_dir()).map_err(|e| e.to_string())?;
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
        ] {
            let source = SOURCE.replace("verifies catalog consistency", wording);
            let path = root.join("fixture.rs");
            std::fs::write(
                &path,
                format!("{source}\n#[cfg(test)] mod tests {{ {test} }}"),
            )
            .map_err(|e| e.to_string())?;
            let binary = root.join(format!("fixture-bin{}", std::env::consts::EXE_SUFFIX));
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
fn diagnostic_only_token_cannot_confirm_return() -> Result<(), String> {
    let source = SOURCE.replace("entry_description_violations", "check_entries");
    let test = TEST
        .replace("entry_description_violations", "check_entries")
        .replace("let catalog =", "let items =")
        .replace("&catalog", "&items")
        .replace("Vec::<String>::new()", "Vec::<String>::new(), \"verifies\"");
    assert_ne!(source, SOURCE);
    assert!(test.contains("check_entries(&items)"));
    assert!(test.contains("Vec::<String>::new(), \"verifies\""));
    assert!(!test.contains("entry_description_violations"));
    assert_eq!(check_source(&source, &test)?, "weakly_exposed");
    Ok(())
}

/// #12010: a direct call elsewhere establishes reach, but cannot lend its
/// result identity to a separate exact oracle containing the changed wording.
#[test]
fn discarded_owner_result_cannot_lend_identity_to_an_unrelated_exact_oracle() -> Result<(), String>
{
    let prefix = "use rust_catalog_description_structural_oracle::default_check_description;\n#[test] fn wording() {\nlet _ = default_check_description();\nlet other = \"verifies catalog consistency\";\n";
    let unrelated = "assert_eq!(other, \"verifies catalog consistency\");";
    let observed = "assert_eq!(default_check_description(), \"verifies catalog consistency\");";
    for (assertions, expected) in [
        (unrelated.to_string(), "weakly_exposed"),
        (
            "let result = default_check_description(); assert_eq!(other, \"verifies catalog consistency\", \"{result}\");".to_string(),
            "weakly_exposed",
        ),
        (
            "let result = default_check_description(); let result = \"verifies catalog consistency\"; assert_eq!(result, \"verifies catalog consistency\");".to_string(),
            "weakly_exposed",
        ),
        (
            "let result = { let _ = default_check_description(); \"verifies catalog consistency\" }; assert_eq!(result, \"verifies catalog consistency\");".to_string(),
            "weakly_exposed",
        ),
        (
            "assert_eq!({ let _ = default_check_description(); \"verifies catalog consistency\" }, \"verifies catalog consistency\");".to_string(),
            "weakly_exposed",
        ),
        (observed.to_string(), "exposed"),
        (format!("{unrelated}\n{observed}"), "exposed"),
        (format!("{observed}\n{unrelated}"), "exposed"),
        (
            "let result = default_check_description();\nassert_eq!(result, \"verifies catalog consistency\");".to_string(),
            "exposed",
        ),
    ] {
        let test = format!("{prefix}{assertions}\n}}\n");
        assert_eq!(check(&test)?, expected, "{assertions}");
    }
    Ok(())
}

/// Preserve the established persisted-effect observer while refusing an
/// assertion on a different instance after the same owner call.
#[test]
fn persisted_effect_observer_retains_identity_without_lending_it_to_another_instance()
-> Result<(), String> {
    let source =
        include_str!("../../../fixtures/observation_verified_side_effect/input/src/lib.rs")
            .replace(
                "pub sent: RefCell<Vec<String>>,",
                "pub sent: RefCell<Vec<String>>,\n    pub status: u32,",
            )
            .replace(
                "Notifier { sent: RefCell::new(Vec::new()) }",
                "Notifier { sent: RefCell::new(Vec::new()), status: 0 }",
            )
            .replace(
                "self.sent.borrow_mut().push(payload.to_string());",
                "let _ = self.status;\n        self.sent.borrow_mut().push(payload.to_string());",
            );
    let test = include_str!(
        "../../../fixtures/observation_verified_side_effect/input/tests/process_tests.rs"
    );
    let manifest =
        include_str!("../../../fixtures/observation_verified_side_effect/input/Cargo.toml");
    let line = source
        .lines()
        .position(|line| line.trim() == "notifier.send(order_id)")
        .ok_or("effect line missing")?
        + 1;
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-    notifier.send(\"static_msg\")\n+    notifier.send(order_id)\n"
    );
    let unrelated = test.replace(
        "assert_eq!(*notifier.sent.borrow(), vec![\"order-42\".to_string()]);",
        "let other = Notifier::new();\n    assert_eq!(*other.sent.borrow(), Vec::<String>::new());",
    );
    assert_ne!(unrelated, test);
    let renamed = test.replace("notifier", "changed");
    let coincident_name = test
        .replace(
            "process_order(&notifier, \"order-42\");",
            "let changed = Notifier::new();\n    process_order(&changed, \"order-42\");",
        )
        .replace("vec![\"order-42\".to_string()]", "Vec::<String>::new()");
    let cleared = test.replace(
        "assert_eq!(*notifier.sent.borrow(), vec![\"order-42\".to_string()]);",
        "notifier.sent.borrow_mut().clear();\n    assert_eq!(*notifier.sent.borrow(), Vec::<String>::new());",
    );
    let readonly = test.replace(
        "assert_eq!(*notifier.sent.borrow(), vec![\"order-42\".to_string()]);",
        "assert_eq!(notifier.status, 0);",
    );
    assert_ne!(readonly, test);
    assert_ne!(renamed, test);
    assert_ne!(coincident_name, test);
    assert_ne!(cleared, test);
    for (test, expected) in [
        (test, "exposed"),
        (renamed.as_str(), "exposed"),
        (unrelated.as_str(), "weakly_exposed"),
        (coincident_name.as_str(), "weakly_exposed"),
        (cleared.as_str(), "weakly_exposed"),
        (readonly.as_str(), "weakly_exposed"),
    ] {
        let report = check_report(&source, test, manifest, &diff)?;
        let finding = report["findings"]
            .as_array()
            .ok_or("missing findings")?
            .iter()
            .find(|finding| finding["probe"]["family"] == "side_effect")
            .ok_or("side-effect row missing")?;
        assert_eq!(finding["classification"], expected, "{report}");
    }
    Ok(())
}

#[test]
fn established_effect_carrier_binds_the_called_instance_only() -> Result<(), String> {
    let source = "#[derive(Debug, PartialEq)]\npub struct Store { pub written: Vec<u8>, pub unrelated: Vec<u8> }\nimpl Store {\n    pub fn new() -> Self { Self { written: vec![], unrelated: vec![] } }\n    pub fn record(&mut self, value: u8) {\n        self.apply(value);\n    }\n    fn apply(&mut self, value: u8) { self.written.push(value); }\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -5,3 +5,3 @@\n     pub fn record(&mut self, value: u8) {\n-        self.apply(0);\n+        self.apply(value);\n     }\n";
    for (observed, expected) in [("store", "exposed"), ("other", "weakly_exposed")] {
        let value = if observed == "store" {
            "vec![2]"
        } else {
            "vec![]"
        };
        let test = format!(
            "use rust_catalog_description_structural_oracle::Store;\n#[test]\nfn records() {{\n    let mut store = Store::new();\n    let other = Store::new();\n    store.record(2);\n    assert_eq!({observed}, Store {{ written: {value}, unrelated: vec![] }});\n}}\n"
        );
        let report = check_report(source, &test, MANIFEST, diff)?;
        let finding = report["findings"]
            .as_array()
            .ok_or("missing findings")?
            .iter()
            .find(|finding| finding["probe"]["family"] == "call_deletion")
            .ok_or("call-deletion row missing")?;
        assert_eq!(finding["classification"], expected, "{report}");
    }
    Ok(())
}

#[test]
fn returned_fields_and_builders_keep_real_identity_and_reject_replaced_values() -> Result<(), String>
{
    let cases = [
        (
            "pub fn tax(subtotal:i32)->i32 {\n    subtotal*8/1000\n}\n",
            "subtotal*8",
            "subtotal*9",
            "let result=tax(3000);\n    assert_eq!(result,24);",
            true,
        ),
        (
            "pub fn tax(subtotal:i32)->i32 {\n    subtotal*8/1000\n}\n",
            "subtotal*8",
            "subtotal*9",
            "let result=tax(3000);\n    let result=24;\n    assert_eq!(result,24);",
            false,
        ),
        (
            "pub struct Item { pub priority:u32 }\npub fn make_item()->Item {\n    Item { priority:3 }\n}\n",
            "priority:3",
            "priority:4",
            "let item=make_item();\n    assert_eq!(item.priority,3);",
            true,
        ),
        (
            "pub struct Item { pub priority:u32 }\npub fn make_item()->Item {\n    Item { priority:3 }\n}\n",
            "priority:3",
            "priority:4",
            "let _=make_item();\n    let item=Item { priority:3 };\n    assert_eq!(item.priority,3);",
            false,
        ),
        (
            "pub struct Builder { pub attempts:u32 }\nimpl Builder {\n pub fn new()->Self {\n  Self { attempts:4 }\n }\n pub fn attempts(mut self,n:u32)->Self { self.attempts=n; self }\n pub fn build(self)->Self { self }\n}\n",
            "attempts:4",
            "attempts:3",
            "let item=Builder::new().build();\n    assert_eq!(item.attempts,4);",
            true,
        ),
        (
            "pub struct Builder { pub attempts:u32 }\nimpl Builder {\n pub fn new()->Self {\n  Self { attempts:4 }\n }\n pub fn attempts(mut self,n:u32)->Self { self.attempts=n; self }\n pub fn build(self)->Self { self }\n}\n",
            "attempts:4",
            "attempts:3",
            "let item=Builder::new().attempts(7).build();\n    assert_eq!(item.attempts,7);",
            false,
        ),
        (
            "pub struct Builder { pub attempts:u32 }\nimpl Builder {\n pub fn new()->Self {\n  Self { attempts:4 }\n }\n pub fn build(self)->Self { Self { attempts:4 } }\n}\n",
            "attempts:4",
            "attempts:3",
            "let item=Builder::new().build();\n    assert_eq!(item.attempts,4);",
            false,
        ),
    ];
    for (source, after, before, body, exposed) in cases {
        let line = source
            .lines()
            .position(|line| line.contains(after))
            .ok_or("changed field missing")?
            + 1;
        let actual = source.lines().nth(line - 1).ok_or("line missing")?;
        let previous = actual.replace(after, before);
        let diff = format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-{previous}\n+{actual}\n"
        );
        let test = format!(
            "use rust_catalog_description_structural_oracle::*;\n#[test]\nfn observes() {{\n    {body}\n}}\n"
        );
        let report = check_report(source, &test, MANIFEST, &diff)?;
        let findings = report["findings"].as_array().ok_or("findings missing")?;
        assert!(!findings.is_empty(), "{report}");
        for finding in findings {
            if exposed {
                assert_eq!(finding["classification"], "exposed", "{report}");
            } else {
                assert_ne!(finding["classification"], "exposed", "{report}");
            }
        }
    }
    Ok(())
}
