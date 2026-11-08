//! #13736: code in a module gated by an exact `#[cfg(kani)]` is verification
//! source. Only the Kani verifier compiles it, so a changed helper there seeds
//! no production probe and the run discloses the change as
//! `verification_only_change`, inline or out-of-line, at any nesting depth.
//!
//! The gate is read from the parent's `mod` declaration through the module
//! composition owner, never from a module or file name: `cfg(not(kani))`,
//! `cfg(any(kani, ..))` and an ungated module named `kani_proofs` keep their
//! production probes.
use super::RustAdapter;
use crate::analysis::language::LanguageAdapter;
use crate::analysis::{AnalysisMode, AnalysisOptions, diff};
use crate::analysis_outcome::AnalysisLimitationKind;
use crate::config::OraclePolicy;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new(label: &str) -> Result<Self, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-cfg-kani-module-{label}-{stamp}-{}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        fixture.write(
            "Cargo.toml",
            "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[features]\nx = []\n",
        )?;
        Ok(fixture)
    }

    fn write(&self, relative: &str, text: &str) -> Result<(), String> {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(path, text).map_err(|error| error.to_string())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn options(root: &Path) -> AnalysisOptions {
    AnalysisOptions {
        root: root.to_path_buf(),
        base: None,
        diff_file: None,
        mode: AnalysisMode::Draft,
        resolved_subject_identity: None,
        open_rust_index_paths: Default::default(),
        include_unchanged_tests: true,
        resolve_tsconfig_paths: false,
        perl_facts_path: None,
        perl_producer_failure: None,
        git_timeout: None,
        git_candidate: None,
        production_like_targets: Default::default(),
        test_harnesses: Vec::new(),
    }
}

/// Production code plus an out-of-line module declared under `gate`.
fn parent_source(gate: &str, module: &str) -> String {
    format!(
        "pub fn charge(total: u32) -> u32 {{\n    if total > 100 {{ total - 5 }} else {{ total }}\n}}\n\n{gate}\nmod {module};\n"
    )
}

/// A Kani harness and a plain helper `fn` (no proof attribute). Line 5 is the
/// helper's predicate, the only line the diff changes; line 4 opens the hunk.
const PROOF_FILE: &str = "use super::*;

fn any_total() -> u32 {
    let total: u32 = kani::any();
    if total > 7 { total - 1 } else { total + 1 }
}

#[kani::proof]
fn charge_never_grows() {
    let total = any_total();
    assert!(charge(total) <= total);
}
";

/// The helper-only edit: `total > 6` became `total > 7`. `first_line` is the
/// line of the leading `let total` context line.
fn helper_diff(path: &str, indent: &str, first_line: usize) -> String {
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -{first_line},3 +{first_line},3 @@\n {indent}    let total: u32 = kani::any();\n-{indent}    if total > 6 {{ total - 1 }} else {{ total + 1 }}\n+{indent}    if total > 7 {{ total - 1 }} else {{ total + 1 }}\n {indent}}}\n"
    )
}

struct Observed {
    findings_in_path: usize,
    all_findings: usize,
    verification_only: bool,
    kani_module_file: bool,
}

/// Asserts the fixture parsed as intended, then runs the real diff adapter.
fn observe(fixture: &Fixture, path: &str, diff_text: &str) -> Result<Observed, String> {
    let mut files = ["src/lib.rs", path]
        .into_iter()
        .chain((path == "src/proofs/deep.rs").then_some("src/proofs.rs"))
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    let index = crate::analysis::facts::build_index(&fixture.0, &files)?;
    // The changed helper is a plain function, not a proof harness: only its
    // module context can make it verification source.
    let helper = index
        .functions()
        .iter()
        .find(|function| function.file == Path::new(path) && function.name == "any_total")
        .ok_or_else(|| format!("fixture must index the helper in {path}"))?;
    assert_eq!(
        helper.source_role,
        crate::analysis::facts::FunctionSourceRole::Production,
        "the helper carries no proof attribute"
    );
    let changed = diff::parse_unified_diff(diff_text);
    assert_eq!(changed.len(), 1, "fixture diff must parse one file");
    assert_eq!(changed[0].path, Path::new(path));
    assert_eq!(changed[0].added_lines.len(), 1);
    assert_eq!(changed[0].removed_lines.len(), 1);
    assert!(
        changed[0].added_lines[0].text.contains("total > 7"),
        "the changed line is the helper predicate"
    );
    let kani_module_file = index.cfg_kani_module_files.contains(Path::new(path));

    let result =
        RustAdapter.analyze_diff(&options(&fixture.0), &OraclePolicy::default(), &changed)?;
    let findings_in_path = result
        .findings
        .iter()
        // Finding locations are root-joined; match the changed file by its
        // trailing components so the count is never vacuously zero.
        .filter(|finding| finding.probe.location.file.ends_with(path))
        .count();
    let all_findings = result.findings.len();
    let verification_only = result.limitations.iter().any(|limitation| {
        limitation.kind == AnalysisLimitationKind::VerificationOnlyChange
            && limitation
                .bounded_detail
                .as_deref()
                .is_some_and(|detail| detail.contains(path))
    });
    Ok(Observed {
        findings_in_path,
        all_findings,
        verification_only,
        kani_module_file,
    })
}

fn out_of_line(label: &str, gate: &str, module: &str) -> Result<Observed, String> {
    let fixture = Fixture::new(label)?;
    let path = format!("src/{module}.rs");
    fixture.write("src/lib.rs", &parent_source(gate, module))?;
    fixture.write(&path, PROOF_FILE)?;
    let index = crate::analysis::facts::build_index(&fixture.0, &[PathBuf::from("src/lib.rs")])?;
    let declarations = &index
        .files()
        .get(Path::new("src/lib.rs"))
        .ok_or("fixture must index src/lib.rs")?
        .data()
        .module_declarations;
    assert_eq!(declarations.len(), 1, "exactly one `mod {module};`");
    assert_eq!(declarations[0].name, module);
    observe(&fixture, &path, &helper_diff(&path, "", 4))
}

fn assert_production(observed: &Observed, shape: &str) {
    assert!(!observed.kani_module_file, "{shape}: not a Kani-only file");
    assert!(
        observed.findings_in_path > 0 && observed.findings_in_path == observed.all_findings,
        "{shape}: the helper edit must keep its production probe"
    );
    assert!(
        !observed.verification_only,
        "{shape}: must not be disclosed as verification-only"
    );
}

fn assert_verification_only(observed: &Observed, shape: &str) {
    assert_eq!(
        observed.all_findings, 0,
        "{shape}: a helper in a cfg(kani) module seeds no production probe"
    );
    assert!(
        observed.verification_only,
        "{shape}: the edit must be disclosed as verification_only_change"
    );
}

#[test]
fn out_of_line_cfg_kani_module_helper_is_verification_only() -> Result<(), String> {
    let observed = out_of_line("positive", "#[cfg(kani)]", "proofs")?;
    assert!(
        observed.kani_module_file,
        "composition marks the file Kani-only"
    );
    assert_verification_only(&observed, "#[cfg(kani)] mod proofs;");
    Ok(())
}

#[test]
fn a_file_below_a_cfg_kani_module_is_verification_only() -> Result<(), String> {
    let fixture = Fixture::new("nested")?;
    fixture.write("src/lib.rs", &parent_source("#[cfg(kani)]", "proofs"))?;
    // The intermediate module carries no gate of its own.
    fixture.write("src/proofs.rs", "mod deep;\n")?;
    fixture.write("src/proofs/deep.rs", PROOF_FILE)?;
    let path = "src/proofs/deep.rs";
    let observed = observe(&fixture, path, &helper_diff(path, "", 4))?;
    assert!(observed.kani_module_file, "Kani-only context is inherited");
    assert_verification_only(&observed, "mod deep; under #[cfg(kani)] mod proofs;");
    Ok(())
}

#[test]
fn inline_cfg_kani_module_helper_is_verification_only() -> Result<(), String> {
    let fixture = Fixture::new("inline")?;
    let indented = PROOF_FILE
        .lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("    {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let source = format!(
        "pub fn charge(total: u32) -> u32 {{\n    if total > 100 {{ total - 5 }} else {{ total }}\n}}\n\n#[cfg(kani)]\nmod proofs {{\n{indented}\n}}\n"
    );
    // The module body starts after line 6, so the helper's `let total` is
    // file line 10 and its predicate line 11.
    assert_eq!(
        source.lines().nth(10),
        Some("        if total > 7 { total - 1 } else { total + 1 }"),
        "fixture places the helper predicate on line 11"
    );
    fixture.write("src/lib.rs", &source)?;
    let path = "src/lib.rs";
    let observed = observe(&fixture, path, &helper_diff(path, "    ", 10))?;
    assert!(
        !observed.kani_module_file,
        "an inline module is recognised from its own text, not composition"
    );
    assert_verification_only(&observed, "inline #[cfg(kani)] mod proofs { .. }");
    Ok(())
}

#[test]
fn cfg_not_kani_module_keeps_production_probes() -> Result<(), String> {
    let observed = out_of_line("not-kani", "#[cfg(not(kani))]", "proofs")?;
    assert_production(&observed, "#[cfg(not(kani))] mod proofs;");
    Ok(())
}

#[test]
fn cfg_any_kani_module_keeps_production_probes() -> Result<(), String> {
    let observed = out_of_line("any-kani", "#[cfg(any(kani, feature = \"x\"))]", "proofs")?;
    assert_production(&observed, "#[cfg(any(kani, feature = \"x\"))] mod proofs;");
    Ok(())
}

#[test]
fn ungated_module_named_kani_proofs_keeps_production_probes() -> Result<(), String> {
    let observed = out_of_line("named", "", "kani_proofs")?;
    assert_production(&observed, "mod kani_proofs; (no cfg)");
    Ok(())
}

#[test]
fn exclusive_cfg_kani_alternatives_keep_production_probes() -> Result<(), String> {
    // `#[cfg(kani)] mod proofs;` and `#[cfg(not(kani))] mod proofs;` name the
    // same file: an ordinary build compiles it through the second one.
    let fixture = Fixture::new("alternatives")?;
    fixture.write(
        "src/lib.rs",
        &format!(
            "{}#[cfg(not(kani))]\nmod proofs;\n",
            parent_source("#[cfg(kani)]", "proofs")
        ),
    )?;
    fixture.write("src/proofs.rs", PROOF_FILE)?;
    let index = crate::analysis::facts::build_index(&fixture.0, &[PathBuf::from("src/lib.rs")])?;
    let guards = index
        .files()
        .get(Path::new("src/lib.rs"))
        .ok_or("fixture must index src/lib.rs")?
        .data()
        .module_declarations
        .iter()
        .map(|declaration| (declaration.name.clone(), declaration.cfg_guard.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        guards,
        [
            ("proofs".to_string(), Some(("kani".to_string(), false))),
            ("proofs".to_string(), Some(("kani".to_string(), true))),
        ],
        "fixture declares both exclusive alternatives"
    );
    let path = "src/proofs.rs";
    let observed = observe(&fixture, path, &helper_diff(path, "", 4))?;
    assert_production(&observed, "#[cfg(kani)] + #[cfg(not(kani))] mod proofs;");
    Ok(())
}
