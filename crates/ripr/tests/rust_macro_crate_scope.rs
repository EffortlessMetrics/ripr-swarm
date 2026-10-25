//! #10191: opaque macro arguments must respect established Cargo crate boundaries.
use ripr::{CheckInput, Mode, OutputFormat, ProbeFamily, check_workspace, render_check};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const LIB: &str = "pub fn answer() -> u8 {\n    7\n}\n";
const TEST: &str = "use demo::answer;\n#[test]\nfn pins() { assert_eq!(answer(), 7); }\n";
const FUZZ: &str = "#![no_main]\nuse libfuzzer_sys::fuzz_target;\nfuzz_target!(|bytes: &[u8]| { assert_eq!(bytes, bytes); });\n";
const BIN: &str = "[package]\nname = 'demo-fuzz'\nversion = '0.0.0'\n[workspace]\n[[bin]]\nname = 'parse_version'\npath = 'parse_version.rs'\ntest = false\n";

struct Subject(PathBuf);

impl Subject {
    fn new(files: &[(&str, &str)]) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-macro-crate-scope-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        let subject = Self(path);
        for (name, source) in [
            ("Cargo.toml", "[package]\nname = 'demo'\nversion = '0.0.0'\nedition = '2024'\n"),
            ("src/lib.rs", LIB),
            ("tests/pin.rs", TEST),
            ("diff.patch", "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn answer() -> u8 {\n-    6\n+    7\n }\n"),
        ].into_iter().chain(files.iter().copied()) {
            let target = subject.0.join(name);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(target, source).map_err(|e| e.to_string())?;
        }
        Ok(subject)
    }

    fn finding(&self) -> Result<serde_json::Value, String> {
        let report = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let subjects = report
            .findings
            .iter()
            .filter(|finding| {
                finding.probe.family == ProbeFamily::ReturnValue && finding.probe.expression == "7"
            })
            .collect::<Vec<_>>();
        assert_eq!(subjects.len(), 1, "exactly one changed return subject");
        let json: serde_json::Value =
            serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                .map_err(|e| e.to_string())?;
        json["findings"]
            .as_array()
            .ok_or("missing findings")?
            .iter()
            .find(|finding| {
                finding["probe"]["family"] == "return_value"
                    && finding["probe"]["expression"] == "7"
            })
            .cloned()
            .ok_or_else(|| "missing return finding".into())
    }
}

impl Drop for Subject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn unrelated_fuzz_binary_keeps_bare_and_standard_assertion_credit() -> Result<(), String> {
    for (manifest_file, source_file, manifest) in [
        ("fuzz/Cargo.toml", "fuzz/parse_version.rs", BIN),
        ("fuzz/Cargo.toml", "fuzz/src/main.rs", BIN),
        (
            "Cargo.toml",
            "tools/smoke.rs",
            "[package]\nname='demo'\nversion='0.0.0'\n[[bin]]\nname='smoke'\npath='tools/smoke.rs'\n",
        ),
    ] {
        for assertion in ["assert_eq!", "std::assert_eq!"] {
            let tests = TEST.replace("assert_eq!", assertion);
            let subject = Subject::new(&[
                ("tests/pin.rs", &tests),
                (manifest_file, manifest),
                (source_file, FUZZ),
            ])?;
            let finding = subject.finding()?;
            assert_eq!(
                finding["classification"], "exposed",
                "{assertion}: {finding}"
            );
            assert_eq!(finding["ripr"]["observe"]["state"], "yes");
            assert_eq!(finding["ripr"]["discriminate"]["state"], "yes");
            assert!(!finding.to_string().contains("may define it"));
        }
    }
    Ok(())
}

#[test]
fn visible_macro_rebindings_and_unknown_placement_still_refuse_credit() -> Result<(), String> {
    let shadow = format!("macro_rules! assert_eq {{ ($($t:tt)*) => {{}} }}\n{TEST}");
    let opaque = format!("emit!(assert_eq);\n{TEST}");
    let imported = format!("use other::assert_eq;\n{TEST}");
    let shared = format!("#[path = \"../fuzz/parse_version.rs\"] mod fuzz;\n{TEST}");
    let include = format!("include!(concat!(env!(\"OUT_DIR\"), \"/macros.rs\"));\n{TEST}");
    let both = format!("{BIN}[lib]\npath='parse_version.rs'\n");
    for (label, files) in [
        ("real local shadow", vec![("tests/pin.rs", shadow.as_str())]),
        (
            "same crate expansion",
            vec![("tests/pin.rs", opaque.as_str())],
        ),
        (
            "imported assertion",
            vec![
                ("tests/pin.rs", imported.as_str()),
                (
                    "Cargo.toml",
                    "[package]\nname='demo'\nversion='0.0.0'\n[dev-dependencies]\nother={path='other'}\n",
                ),
                (
                    "other/Cargo.toml",
                    "[package]\nname='other'\nversion='0.0.0'\n",
                ),
                (
                    "other/src/lib.rs",
                    "#[macro_export]\nmacro_rules! assert_eq { ($($t:tt)*) => {} }\n",
                ),
            ],
        ),
        (
            "shared fuzz source",
            vec![
                ("tests/pin.rs", shared.as_str()),
                ("fuzz/Cargo.toml", BIN),
                ("fuzz/parse_version.rs", FUZZ),
            ],
        ),
        (
            "unknown include",
            vec![
                ("tests/pin.rs", include.as_str()),
                ("fuzz/Cargo.toml", BIN),
                ("fuzz/parse_version.rs", FUZZ),
            ],
        ),
        (
            "unplaced fuzz source",
            vec![("fuzz/parse_version.rs", FUZZ)],
        ),
        (
            "library can export macros",
            vec![
                (
                    "fuzz/Cargo.toml",
                    "[package]\nname='demo-fuzz'\nversion='0.0.0'\n[workspace]\n[lib]\npath='parse_version.rs'\n",
                ),
                ("fuzz/parse_version.rs", FUZZ),
            ],
        ),
        (
            "one file is both library and binary",
            vec![
                ("fuzz/Cargo.toml", both.as_str()),
                ("fuzz/parse_version.rs", FUZZ),
            ],
        ),
    ] {
        let finding = Subject::new(&files)?.finding()?;
        assert_ne!(finding["classification"], "exposed", "{label}: {finding}");
        if label == "real local shadow" {
            assert_eq!(finding["classification"], "reachable_unrevealed");
        }
    }
    Ok(())
}
