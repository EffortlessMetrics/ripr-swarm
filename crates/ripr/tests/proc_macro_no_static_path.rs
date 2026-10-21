//! Public-API controls for `no_static_path` in proc-macro crates (#8999,
//! RIPR-SPEC-0114).
//!
//! Tests reach proc-macro code through `#[derive]` expansion in another crate,
//! so a name scan finding no calling test must not read `no_static_path`.
//! Each control changes one thing about the shape: the manifest, or which
//! target of the package the edit lands in.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace, render_check};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const PROC_MACRO: &str = "[package]\nname = \"derive-demo\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\nproc-macro = true\n\n[workspace]\n";
const ORDINARY: &str =
    "[package]\nname = \"derive-demo\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n";

const SOURCE: &str = "pub fn width(n: u32) -> u32 {\n    if n > 3 { 1 } else { 0 }\n}\n";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(manifest: &str, file: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-proc-macro-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let target = root.join(file);
        let parent = target.parent().ok_or("file has no parent")?;
        std::fs::create_dir_all(parent).map_err(|error| format!("create dir: {error}"))?;
        std::fs::write(root.join("Cargo.toml"), manifest)
            .map_err(|error| format!("write Cargo.toml: {error}"))?;
        std::fs::write(&target, SOURCE).map_err(|error| format!("write source: {error}"))?;
        let diff = format!(
            "diff --git a/{file} b/{file}\nindex 1111111..2222222 100644\n--- a/{file}\n+++ b/{file}\n@@ -2,1 +2,1 @@\n-    if n >= 3 {{ 1 }} else {{ 0 }}\n+    if n > 3 {{ 1 }} else {{ 0 }}\n"
        );
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff: {error}"))?;
        Ok(Self { root })
    }

    fn input(&self, format: OutputFormat) -> CheckInput {
        CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format,
            include_unchanged_tests: true,
            ..CheckInput::default()
        }
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let findings = check_workspace(self.input(OutputFormat::Json))?.findings;
        let on_line: Vec<Finding> = findings
            .into_iter()
            .filter(|finding| finding.probe.location.line == 2)
            .collect();
        if on_line.is_empty() {
            return Err("premise: a finding on the edited line".to_string());
        }
        Ok(on_line)
    }

    fn rendered(&self, format: OutputFormat) -> Result<String, String> {
        let output = check_workspace(self.input(OutputFormat::Json))?;
        render_check(&output, &format)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn proc_macro_library_reads_static_unknown_in_findings_json_and_human() -> Result<(), String> {
    let repo = TempRepo::create(PROC_MACRO, "src/lib.rs")?;
    let findings = repo.findings()?;
    assert!(
        findings
            .iter()
            .all(|finding| finding.class != ExposureClass::NoStaticPath),
        "{:?}",
        findings
            .iter()
            .map(|f| f.class.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.class == ExposureClass::StaticUnknown),
        "{:?}",
        findings
            .iter()
            .map(|f| f.class.as_str())
            .collect::<Vec<_>>()
    );
    for finding in &findings {
        assert_eq!(finding.class.severity(), "note");
    }

    let json = repo.rendered(OutputFormat::Json)?;
    assert!(json.contains("\"no_static_path\":0"), "{json}");
    assert!(
        !json.contains("\"classification\": \"no_static_path\""),
        "{json}"
    );
    assert!(
        json.contains("\"classification\": \"static_unknown\""),
        "{json}"
    );
    assert!(json.contains("proc_macro_expansion:"), "{json}");
    assert!(!json.contains("no test can "), "{json}");

    let human = repo.rendered(OutputFormat::HumanFull)?;
    assert!(human.contains("static_unknown (note"), "{human}");
    assert!(!human.contains("No reachable test oracle"), "{human}");
    assert!(!human.contains("no test can "), "{human}");
    assert!(human.contains("derive or attribute expansion"), "{human}");
    Ok(())
}

#[test]
fn ordinary_library_with_the_same_source_keeps_no_static_path() -> Result<(), String> {
    let repo = TempRepo::create(ORDINARY, "src/lib.rs")?;
    let findings = repo.findings()?;
    assert!(
        findings
            .iter()
            .any(|finding| finding.class == ExposureClass::NoStaticPath),
        "{:?}",
        findings
            .iter()
            .map(|f| f.class.as_str())
            .collect::<Vec<_>>()
    );
    let json = repo.rendered(OutputFormat::Json)?;
    assert!(!json.contains("proc_macro_expansion:"), "{json}");
    Ok(())
}

#[test]
fn build_script_of_a_proc_macro_package_is_not_proc_macro_library_code() -> Result<(), String> {
    let repo = TempRepo::create(PROC_MACRO, "build.rs")?;
    repo.findings()?;
    let json = repo.rendered(OutputFormat::Json)?;
    assert!(!json.contains("proc_macro_expansion:"), "{json}");
    Ok(())
}
