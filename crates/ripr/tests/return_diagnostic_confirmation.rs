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

#[test]
fn guarded_error_constructor_change_names_its_limit_without_inventing_a_test_gap()
-> Result<(), String> {
    let source = include_str!("../../../fixtures/error_return_unresolved_guard/input/src/lib.rs");
    let test = include_str!("../../../fixtures/error_return_unresolved_guard/input/tests/q.rs");
    let manifest = include_str!("../../../fixtures/error_return_unresolved_guard/input/Cargo.toml");
    let diff = include_str!("../../../fixtures/error_return_unresolved_guard/diff.patch");
    let report = check_report(source, test, manifest, diff)?;
    let findings = report["findings"].as_array().ok_or("findings missing")?;
    assert_eq!(findings.len(), 3);
    for finding in findings {
        assert_eq!(finding["classification"], "infection_unknown");
        assert_eq!(
            finding["static_limit_kind"],
            "rust_value_propagation_unresolved"
        );
        let next = finding["recommended_next_step"]
            .as_str()
            .ok_or("next step missing")?;
        assert!(next.contains("boolean guard"), "{next}");
        assert!(
            !next.starts_with("Add ") && !next.starts_with("Strengthen "),
            "{next}"
        );
        assert!(
            finding["ripr"]["infect"]["summary"]
                .as_str()
                .is_some_and(|s| s.contains("cancelled"))
        );
    }
    let overwritten = test.replace("let err =", "let mut err =").replace(
        "    assert_eq!(err,",
        "    err = cancelled_error();\n    assert_eq!(err,",
    );
    let shadowed = test.replace(
        "    assert_eq!(err,",
        "    let err = cancelled_error();\n    assert_eq!(err,",
    );
    let discarded = test
        .replace("let err =", "let _ =")
        .replace(
            "    assert_eq!(err, cancelled_error());",
            "    assert_eq!(1, 1);",
        )
        .replace("    assert_eq!(err.message, \"operation cancelled\");", "");
    for (label, body) in [
        ("overwritten", overwritten),
        ("shadowed", shadowed),
        ("discarded", discarded),
        (
            "unentered",
            test.replace("run_slow(true)", "run_slow(false)"),
        ),
    ] {
        let report = check_report(source, &body, manifest, diff)?;
        let findings = report["findings"]
            .as_array()
            .ok_or("negative findings missing")?;
        assert!(!findings.is_empty());
        for finding in findings {
            assert_ne!(finding["classification"], "exposed", "{label}: {finding}");
            assert_ne!(
                finding["static_limit_kind"], "rust_value_propagation_unresolved",
                "{label}: {finding}"
            );
        }
    }
    Ok(())
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
fn returned_comparisons_pair_the_boundary_oracle_with_its_owner_and_element() -> Result<(), String>
{
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for (name, discarded, observed, first, second) in [
        (
            "boundary_pairing_sibling_owner",
            "let _ = a::gate(10);",
            "assert_eq!(a::gate(10), true);",
            "assert_eq!(a::gate(11), true);",
            "assert_eq!(b::gate(10), true);",
        ),
        (
            "boundary_pairing_sibling_element",
            "let _ = a::gate(&[0]);",
            "assert_eq!(a::gate(&[0]), [false]);",
            "assert_eq!(a::gate(&[5]), [true]);",
            "assert_eq!(gate(&[0]), [false]);",
        ),
    ] {
        let fixture = fixtures.join(name);
        let source = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
            .map_err(|error| error.to_string())?;
        let manifest = std::fs::read_to_string(fixture.join("input/Cargo.toml"))
            .map_err(|error| error.to_string())?;
        let diff = std::fs::read_to_string(fixture.join("diff.patch"))
            .map_err(|error| error.to_string())?;
        assert!(source.contains(discarded) && source.contains(first) && source.contains(second));
        for reversed in [false, true] {
            for positive in [false, true] {
                let mut source = source.clone();
                if reversed {
                    source = source
                        .replace(first, "__FIRST_ORACLE__")
                        .replace(second, first)
                        .replace("__FIRST_ORACLE__", second);
                }
                if positive {
                    source = source.replace(discarded, observed);
                }
                let report = check_report(&source, "", &manifest, &diff)?;
                let findings = report["findings"].as_array().ok_or("missing findings")?;
                assert_eq!(findings.len(), 2, "{name}/{reversed}/{positive}: {report}");
                for family in ["predicate", "return_value"] {
                    let selected: Vec<_> = findings
                        .iter()
                        .filter(|finding| finding["probe"]["family"] == family)
                        .collect();
                    assert_eq!(selected.len(), 1, "{family}: {report}");
                    assert_eq!(
                        selected[0]["classification"],
                        if positive {
                            "exposed"
                        } else {
                            "weakly_exposed"
                        },
                        "{name}/{family}/reversed={reversed}/positive={positive}: {report}"
                    );
                }
            }
        }
    }
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
        let imports = if source.contains("struct Builder") {
            "Builder"
        } else if source.contains("make_item") {
            "{make_item, Item}"
        } else {
            "tax"
        };
        let test = format!(
            "use rust_catalog_description_structural_oracle::{imports};\n#[test]\nfn observes() {{\n    {body}\n}}\n"
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

#[test]
fn returned_field_constructor_identity_respects_namespace() -> Result<(), String> {
    let source = "pub struct Builder { pub attempts:u32 }\nimpl Builder {\n pub fn new()->Self {\n  Self { attempts:4 }\n }\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -4,1 +4,1 @@\n-  Self { attempts:3 }\n+  Self { attempts:4 }\n";
    let foreign = "mod foreign { pub struct Builder { pub attempts:u32 } impl Builder { pub fn new()->Self { Self { attempts:4 } } } pub fn replace(_discarded: rust_catalog_description_structural_oracle::Builder)->Builder { Builder::new() } }";
    for (constructor, exposed) in [
        (
            "rust_catalog_description_structural_oracle::Builder::new()",
            true,
        ),
        ("foreign::Builder::new()", false),
        (
            "foreign::replace(rust_catalog_description_structural_oracle::Builder::new())",
            false,
        ),
    ] {
        let test = format!(
            "{foreign}\n#[test]\nfn observes() {{\n let _=rust_catalog_description_structural_oracle::Builder::new();\n let item={constructor};\n assert_eq!(item.attempts,4);\n}}\n"
        );
        let report = check_report(source, &test, MANIFEST, diff)?;
        let findings = report["findings"].as_array().ok_or("findings missing")?;
        assert!(!findings.is_empty(), "{report}");
        for finding in findings {
            assert_eq!(finding["classification"] == "exposed", exposed, "{report}");
        }
    }
    Ok(())
}

#[test]
fn returned_field_identity_rejects_mutation_inside_let_initializers() -> Result<(), String> {
    let source = "#[derive(Clone, Copy)] pub struct Builder { pub attempts:u32 }\nimpl Builder {\n pub fn new()->Self {\n  Self { attempts:4 }\n }\n pub fn reset(&mut self) { self.attempts=4; }\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -4,1 +4,1 @@\n-  Self { attempts:3 }\n+  Self { attempts:4 }\n";
    for (body, exposed) in [
        (
            "let mut item=Builder::new(); let _ignored={ item.attempts=4; 0 }; assert_eq!(item.attempts,4);",
            false,
        ),
        (
            "let mut item=Builder::new(); let _ignored=item.reset(); assert_eq!(item.attempts,4);",
            false,
        ),
        (
            "let item=Builder::new(); let _alias=item; assert_eq!(item.attempts,4);",
            true,
        ),
        (
            "let item=Builder::new(); let _snapshot=item.attempts; assert_eq!(item.attempts,4);",
            true,
        ),
    ] {
        let test = format!(
            "use rust_catalog_description_structural_oracle::Builder;\n#[test]\nfn observes() {{\n {body}\n}}\n"
        );
        let report = check_report(source, &test, MANIFEST, diff)?;
        let findings = report["findings"].as_array().ok_or("findings missing")?;
        assert!(!findings.is_empty(), "{report}");
        for finding in findings {
            assert_eq!(finding["classification"] == "exposed", exposed, "{report}");
        }
    }
    Ok(())
}

#[test]
fn persisted_effect_identity_tracks_the_changed_argument_not_written_siblings() -> Result<(), String>
{
    for (
        extra_signature,
        extra_argument,
        status_write,
        before_call,
        expected_status,
        status_exposed,
        sent_exposed,
    ) in [
        (
            "",
            "",
            "self.status=1;",
            "notifier.send(\"static_msg\")",
            1,
            false,
            true,
        ),
        (
            ", status:u32",
            ", status",
            "self.status=status;",
            "notifier.send(\"static_msg\", status)",
            7,
            false,
            true,
        ),
        (
            ", status:u32",
            ", status",
            "self.status=status;",
            "notifier.send(order_id, 0)",
            7,
            true,
            false,
        ),
    ] {
        let source = format!(
            "pub struct Notifier {{ pub sent:Vec<String>, pub status:u32 }}\nimpl Notifier {{\n pub fn new()->Self {{ Self {{ sent:vec![], status:0 }} }}\n pub fn send(&mut self,payload:&str{extra_signature})->bool {{\n  {status_write}\n  self.sent.push(payload.to_string());\n  true\n }}\n}}\npub fn process_order(notifier:&mut Notifier,order_id:&str{extra_signature})->bool {{\n notifier.send(order_id{extra_argument})\n}}\n"
        );
        let after_call = format!("notifier.send(order_id{extra_argument})");
        let diff = format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -11,1 +11,1 @@\n- {before_call}\n+ {after_call}\n"
        );
        let extra_value = if extra_signature.is_empty() {
            ""
        } else {
            ", 7"
        };
        for (assertion, exposed) in [
            (
                "assert_eq!(notifier.sent, vec![\"order-42\".to_string()]);".to_string(),
                sent_exposed,
            ),
            (
                format!("assert_eq!(notifier.status,{expected_status});"),
                status_exposed,
            ),
            ("assert_eq!(notifier.sent.len(),1);".to_string(), false),
        ] {
            let test = format!(
                "use rust_catalog_description_structural_oracle::{{Notifier,process_order}};\n#[test]\nfn observes() {{\n let mut notifier=Notifier::new();\n process_order(&mut notifier,\"order-42\"{extra_value});\n {assertion}\n}}\n"
            );
            let report = check_report(&source, &test, MANIFEST, &diff)?;
            let finding = report["findings"]
                .as_array()
                .ok_or("findings missing")?
                .iter()
                .find(|finding| finding["probe"]["family"] == "side_effect")
                .ok_or("side-effect row missing")?;
            assert_eq!(finding["classification"] == "exposed", exposed, "{report}");
        }
    }
    Ok(())
}

#[test]
fn returned_array_field_identity_tracks_the_changed_element() -> Result<(), String> {
    let source = "pub struct Row { pub total:u32 }\npub fn rows(a:u32,b:u32)->[Row;2] {\n [Row { total:0 }, Row { total:a+b }]\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3,1 +3,1 @@\n- [Row { total:0 }, Row { total:a-b }]\n+ [Row { total:0 }, Row { total:a+b }]\n";
    for (body, exposed) in [
        ("let rows=rows(30,3);\n assert_eq!(rows[1].total,33);", true),
        ("let rows=rows(30,3);\n assert_eq!(rows[0].total,0);", false),
        (
            "let mut rows=rows(30,3);\n rows[1].total=33;\n assert_eq!(rows[1].total,33);",
            false,
        ),
        (
            "let _=rows(30,3);\n let rows=[Row { total:0 }, Row { total:33 }];\n assert_eq!(rows[1].total,33);",
            false,
        ),
    ] {
        let test = format!(
            "use rust_catalog_description_structural_oracle::{{rows,Row}};\n#[test]\nfn observes() {{\n {body}\n}}\n"
        );
        let report = check_report(source, &test, MANIFEST, diff)?;
        for family in ["return_value", "field_construction"] {
            let findings: Vec<_> = report["findings"]
                .as_array()
                .ok_or("findings missing")?
                .iter()
                .filter(|finding| finding["probe"]["family"] == family)
                .collect();
            assert!(!findings.is_empty(), "{family}: {report}");
            for finding in findings {
                assert_eq!(
                    finding["classification"] == "exposed",
                    exposed,
                    "{body}: {report}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn unit_error_variant_unwrap_identity_preserves_current_binding() -> Result<(), String> {
    let source = "#[derive(Debug,PartialEq,Eq)]\npub enum CalcError { Negative, TooLarge }\npub fn compute(x:i32)->Result<i32,CalcError> {\n if x<0 { return Err(CalcError::Negative); }\n Ok(x*2)\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -4,1 +4,1 @@\n- if x<0 { return Err(CalcError::TooLarge); }\n+ if x<0 { return Err(CalcError::Negative); }\n";
    for (body, exposed) in [
        (
            "assert_eq!(compute(-1).unwrap_err(),CalcError::Negative);",
            true,
        ),
        (
            "let err=compute(-1).unwrap_err();\n assert_eq!(err,CalcError::Negative);",
            true,
        ),
        (
            "let err=compute(-1).expect_err(\"must reject\");\n assert_eq!(err,CalcError::Negative);",
            true,
        ),
        (
            "let _=compute(-1);\n let err=CalcError::Negative;\n assert_eq!(err,CalcError::Negative);",
            false,
        ),
        (
            "let mut err=compute(-1).unwrap_err();\n err=CalcError::Negative;\n assert_eq!(err,CalcError::Negative);",
            false,
        ),
    ] {
        let test = format!(
            "use rust_catalog_description_structural_oracle::{{compute,CalcError}};\n#[test]\nfn observes() {{\n {body}\n}}\n"
        );
        let report = check_report(source, &test, MANIFEST, diff)?;
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        for family in ["error_path", "return_value"] {
            let selected: Vec<_> = findings
                .iter()
                .filter(|f| f["probe"]["family"] == family)
                .collect();
            assert_eq!(selected.len(), 1, "{family}: {report}");
            assert_eq!(
                selected[0]["classification"] == "exposed",
                exposed,
                "{body}: {report}"
            );
        }
    }
    Ok(())
}

#[test]
fn bound_literal_matches_keeps_scalar_identity_and_currentness() -> Result<(), String> {
    let source = "pub fn score(value:i32)->i32 {\n value+1\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,1 +2,1 @@\n- value+2\n+ value+1\n";
    for (body, exposed) in [
        ("let value=score(1);\n assert!(matches!(value,2));", true),
        (
            "let value=score(1);\n assert!(matches!(value,_ if value==2));",
            true,
        ),
        (
            "let value=score(1);\n assert!(matches!(value,_ if 2==value));",
            true,
        ),
        ("let value=score(1);\n assert!(matches!(value,5));", false),
        (
            "let value=score(1);\n assert!(matches!(value,_ if value==5));",
            false,
        ),
        (
            "let mut value=score(1);\n value=2;\n assert!(matches!(value,2));",
            false,
        ),
        (
            "let _=score(1);\n let value=2;\n assert!(matches!(value,2));",
            false,
        ),
        (
            "let value={let _=score(1);2};\n assert!(matches!(value,2));",
            false,
        ),
        (
            "let value=score(1);\n let other=2;\n assert!(matches!(value,_ if other==2));",
            false,
        ),
        (
            "let value=score(1);\n assert!(matches!(value,_ if value==2 || true));",
            false,
        ),
    ] {
        let test = format!(
            "use rust_catalog_description_structural_oracle::score;\n#[test]\nfn observes() {{\n {body}\n}}\n"
        );
        let report = check_report(source, &test, MANIFEST, diff)?;
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        let selected: Vec<_> = findings
            .iter()
            .filter(|f| f["probe"]["family"] == "return_value")
            .collect();
        assert_eq!(selected.len(), 1, "{report}");
        assert_eq!(
            selected[0]["classification"] == "exposed",
            exposed,
            "{body}: {report}"
        );
    }
    Ok(())
}

#[test]
fn independent_same_named_method_helper_does_not_look_self_computed() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/owner_name_collision_helper_keeps_credit");
    let source = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
        .map_err(|error| error.to_string())?;
    let manifest = std::fs::read_to_string(fixture.join("input/Cargo.toml"))
        .map_err(|error| error.to_string())?;
    let diff =
        std::fs::read_to_string(fixture.join("diff.patch")).map_err(|error| error.to_string())?;
    for (source, exposed) in [
        (source.clone(), true),
        (source.replace("count_items(&[1, 2, 3, 4])", "4"), true),
        (
            source
                .replace(
                    "values.len()",
                    "Stack::len(&Stack::from_vec(values.to_vec()))",
                )
                .replace("count_items(&[1, 2, 3, 4])", "count_items(&[1, 2, 3])"),
            false,
        ),
    ] {
        let report = check_report(&source, "", &manifest, &diff)?;
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(findings.len(), 1, "{report}");
        assert_eq!(
            findings[0]["classification"] == "exposed",
            exposed,
            "{report}"
        );
    }
    Ok(())
}

#[test]
fn primitive_named_format_effect_carries_only_its_changed_argument() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/observation_verified_call_deletion");
    let source =
        std::fs::read_to_string(fixture.join("input/src/lib.rs")).map_err(|e| e.to_string())?;
    let test = std::fs::read_to_string(fixture.join("input/tests/store_tests.rs"))
        .map_err(|e| e.to_string())?;
    let manifest =
        std::fs::read_to_string(fixture.join("input/Cargo.toml")).map_err(|e| e.to_string())?;
    let diff = std::fs::read_to_string(fixture.join("diff.patch")).map_err(|e| e.to_string())?;
    for (source, test, diff, exposed) in [
        (source.clone(), test.clone(), diff.clone(), true),
        (
            source.clone(),
            test.replace(
                "assert_eq!(cache.inserted, vec![\"result_key=42\".to_string()]);",
                "assert_eq!(cache.inserted.len(), 1);",
            ),
            diff.clone(),
            false,
        ),
        (
            source.replace("format!(\"{key}={value}\")", "format!(\"constant\")"),
            test.clone(),
            diff.clone(),
            false,
        ),
        (
            format!("macro_rules! format {{ ($($t:tt)*) => {{ String::new() }}; }}\n{source}"),
            test.clone(),
            diff.replace("@@ -13,4 +13,4", "@@ -14,4 +14,4"),
            false,
        ),
    ] {
        let report = check_report(&source, &test, &manifest, &diff)?;
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        let selected: Vec<_> = findings
            .iter()
            .filter(|finding| finding["probe"]["family"] == "side_effect")
            .collect();
        assert_eq!(selected.len(), 1, "{report}");
        assert_eq!(
            selected[0]["classification"] == "exposed",
            exposed,
            "{report}"
        );
    }
    Ok(())
}

#[test]
fn tuple_option_projection_keeps_its_receiver_and_changed_slot() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/rust_same_method_owner_type_positive");
    let source =
        std::fs::read_to_string(fixture.join("input/src/lib.rs")).map_err(|e| e.to_string())?;
    let original = std::fs::read_to_string(fixture.join("input/tests/size_hints.rs"))
        .map_err(|e| e.to_string())?;
    let manifest =
        std::fs::read_to_string(fixture.join("input/Cargo.toml")).map_err(|e| e.to_string())?;
    let diff = std::fs::read_to_string(fixture.join("diff.patch")).map_err(|e| e.to_string())?;
    for (test,exposed) in [
        (original.clone(),true),
        (original.replace("assert_eq!(it.size_hint().1, Some(it.remaining));","assert_eq!(it.size_hint().0, 0);"),false),
        (original.replace("assert_eq!(it.size_hint().1, Some(it.remaining));","let hint=it.size_hint();\n    assert_eq!(hint.1, Some(it.remaining));"),true),
        (original.replace("assert_eq!(it.size_hint().1, Some(it.remaining));","let mut hint=it.size_hint();\n    hint=(0,Some(it.remaining));\n    assert_eq!(hint.1, Some(it.remaining));"),false),
        (original.replace("::WhileSome;","::{WhileSome,Combinations};").replace("assert_eq!(it.size_hint().1, Some(it.remaining));","let _=it.size_hint();\n    let other=Combinations { remaining:2 };\n    assert_eq!(other.size_hint().1, Some(other.remaining));"),false),
    ] {
        let report=check_report(&source,&test,&manifest,&diff)?;
        let findings=report["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(findings.len(),1,"{report}");
        assert_eq!(findings[0]["classification"]=="exposed",exposed,"{test}: {report}");
    }
    Ok(())
}

#[test]
fn tuple_iterator_identity_cannot_borrow_an_inherent_override() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/rust_same_method_owner_type_positive");
    let source =
        std::fs::read_to_string(fixture.join("input/src/lib.rs")).map_err(|e| e.to_string())?;
    let test = std::fs::read_to_string(fixture.join("input/tests/size_hints.rs"))
        .map_err(|e| e.to_string())?;
    let manifest =
        std::fs::read_to_string(fixture.join("input/Cargo.toml")).map_err(|e| e.to_string())?;
    let diff = std::fs::read_to_string(fixture.join("diff.patch")).map_err(|e| e.to_string())?;
    let source = format!(
        "{source}\nimpl WhileSome {{ pub fn size_hint(&self)->(usize,Option<usize>) {{(0,Some(self.remaining))}} }}\n"
    );
    let report = check_report(&source, &test, &manifest, &diff)?;
    let findings = report["findings"].as_array().ok_or("missing findings")?;
    assert_eq!(findings.len(), 1, "{report}");
    assert_ne!(findings[0]["classification"], "exposed", "{report}");
    Ok(())
}
