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
    let index = fixture_index(fixture)?;
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

/// Every `.rs` file of the fixture, indexed together as the run would.
fn fixture_index(fixture: &Fixture) -> Result<crate::analysis::facts::RustIndex, String> {
    let mut files = Vec::new();
    let mut pending = vec![PathBuf::from("src")];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(fixture.0.join(&directory)).map_err(|e| e.to_string())? {
            let name = entry.map_err(|e| e.to_string())?.file_name();
            let relative = directory.join(&name);
            if fixture.0.join(&relative).is_dir() {
                pending.push(relative);
            } else if relative.extension().is_some_and(|ext| ext == "rs") {
                files.push(relative);
            }
        }
    }
    files.sort();
    crate::analysis::facts::build_index(&fixture.0, &files)
}

type Guard = Option<(String, bool)>;

/// The captured `(name, simple cfg guard)` of each out-of-line declaration.
fn declarations(fixture: &Fixture, file: &str) -> Result<Vec<(String, Guard)>, String> {
    let index = fixture_index(fixture)?;
    Ok(index
        .files()
        .get(Path::new(file))
        .ok_or_else(|| format!("fixture must index {file}"))?
        .data()
        .module_declarations
        .iter()
        .map(|declaration| (declaration.name.clone(), declaration.cfg_guard.clone()))
        .collect())
}

fn guard(predicate: &str, negated: bool) -> Guard {
    Some((predicate.to_string(), negated))
}

fn out_of_line(label: &str, gate: &str, module: &str, expected: Guard) -> Result<Observed, String> {
    let fixture = Fixture::new(label)?;
    let path = format!("src/{module}.rs");
    fixture.write("src/lib.rs", &parent_source(gate, module))?;
    fixture.write(&path, PROOF_FILE)?;
    assert_eq!(
        declarations(&fixture, "src/lib.rs")?,
        [(module.to_string(), expected)],
        "exactly one `mod {module};` with the intended guard"
    );
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
    let observed = out_of_line("positive", "#[cfg(kani)]", "proofs", guard("kani", false))?;
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
    let observed = out_of_line(
        "not-kani",
        "#[cfg(not(kani))]",
        "proofs",
        guard("kani", true),
    )?;
    assert_production(&observed, "#[cfg(not(kani))] mod proofs;");
    Ok(())
}

#[test]
fn cfg_any_kani_module_keeps_production_probes() -> Result<(), String> {
    let observed = out_of_line(
        "any-kani",
        "#[cfg(any(kani, feature = \"x\"))]",
        "proofs",
        None,
    )?;
    assert_production(&observed, "#[cfg(any(kani, feature = \"x\"))] mod proofs;");
    Ok(())
}

#[test]
fn ungated_module_named_kani_proofs_keeps_production_probes() -> Result<(), String> {
    let observed = out_of_line("named", "", "kani_proofs", None)?;
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

/// #13736 review: a guard belongs to one declaration, never to a sibling
/// declared on the same line.
#[test]
fn an_ungated_sibling_on_the_gated_line_keeps_production_probes() -> Result<(), String> {
    let fixture = Fixture::new("same-line")?;
    fixture.write(
        "src/lib.rs",
        "pub fn charge(total: u32) -> u32 {\n    if total > 100 { total - 5 } else { total }\n}\n\n#[cfg(kani)] mod proofs; mod helpers;\n",
    )?;
    fixture.write("src/proofs.rs", PROOF_FILE)?;
    fixture.write("src/helpers.rs", PROOF_FILE)?;
    assert_eq!(
        declarations(&fixture, "src/lib.rs")?,
        [
            ("helpers".to_string(), None),
            ("proofs".to_string(), guard("kani", false)),
        ],
        "both declarations share line 5; only `proofs` is gated"
    );
    let index = fixture_index(&fixture)?;
    assert!(
        index
            .cfg_kani_module_files
            .contains(Path::new("src/proofs.rs")),
        "the gated sibling is still Kani-only"
    );
    let path = "src/helpers.rs";
    let observed = observe(&fixture, path, &helper_diff(path, "", 4))?;
    assert_production(
        &observed,
        "mod helpers; after #[cfg(kani)] mod proofs; on one line",
    );
    Ok(())
}

#[test]
fn a_second_cfg_or_cfg_attr_on_the_declaration_keeps_production_probes() -> Result<(), String> {
    for (label, gate) in [
        ("second-cfg", "#[cfg(kani)]\n#[cfg(feature = \"x\")]"),
        (
            "cfg-attr",
            "#[cfg(kani)]\n#[cfg_attr(kani, allow(dead_code))]",
        ),
    ] {
        let observed = out_of_line(label, gate, "proofs", None)?;
        assert_production(&observed, gate);
    }
    Ok(())
}

#[test]
fn a_second_crate_root_declaring_the_module_keeps_production_probes() -> Result<(), String> {
    let fixture = Fixture::new("two-roots")?;
    fixture.write("src/lib.rs", &parent_source("#[cfg(kani)]", "proofs"))?;
    fixture.write(
        "src/main.rs",
        "mod proofs;\nfn charge(total: u32) -> u32 { total }\nfn main() {}\n",
    )?;
    fixture.write("src/proofs.rs", PROOF_FILE)?;
    assert_eq!(
        declarations(&fixture, "src/main.rs")?,
        [("proofs".to_string(), None)],
        "the binary root declares the same module ungated"
    );
    let path = "src/proofs.rs";
    let observed = observe(&fixture, path, &helper_diff(path, "", 4))?;
    assert_production(&observed, "lib #[cfg(kani)] mod proofs; + main mod proofs;");
    Ok(())
}

/// An owner the producer cannot capture (a `mod proofs;` inside a macro token
/// tree) can compile the file in an ordinary build.
#[test]
fn an_invisible_owner_in_a_macro_token_tree_keeps_production_probes() -> Result<(), String> {
    let fixture = Fixture::new("invisible-owner")?;
    fixture.write("src/lib.rs", &parent_source("#[cfg(kani)]", "proofs"))?;
    fixture.write(
        "src/main.rs",
        "macro_rules! declare { ($($item:item)*) => { $($item)* }; }\ndeclare! { mod proofs; }\nfn charge(total: u32) -> u32 { total }\nfn main() {}\n",
    )?;
    fixture.write("src/proofs.rs", PROOF_FILE)?;
    assert!(
        declarations(&fixture, "src/main.rs")?.is_empty(),
        "the producer captures no declaration inside the macro call"
    );
    let path = "src/proofs.rs";
    let observed = observe(&fixture, path, &helper_diff(path, "", 4))?;
    assert_production(
        &observed,
        "declare! { mod proofs; } beside #[cfg(kani)] mod proofs;",
    );
    Ok(())
}

#[test]
fn a_file_also_included_from_production_keeps_production_probes() -> Result<(), String> {
    let fixture = Fixture::new("declared-and-included")?;
    fixture.write(
        "src/lib.rs",
        &format!("{}mod other;\n", parent_source("#[cfg(kani)]", "proofs")),
    )?;
    fixture.write("src/other.rs", "include!(\"proofs.rs\");\n")?;
    fixture.write("src/proofs.rs", PROOF_FILE)?;
    let index = fixture_index(&fixture)?;
    assert!(
        index
            .include_parents
            .contains_key(Path::new("src/proofs.rs")),
        "the fixture includes proofs.rs from the ungated other.rs"
    );
    let path = "src/proofs.rs";
    let observed = observe(&fixture, path, &helper_diff(path, "", 4))?;
    assert_production(
        &observed,
        "#[cfg(kani)] mod proofs; + include!(\"proofs.rs\")",
    );
    Ok(())
}

#[test]
fn a_fragment_included_by_a_cfg_kani_module_is_verification_only() -> Result<(), String> {
    let fixture = Fixture::new("kani-include")?;
    fixture.write("src/lib.rs", &parent_source("#[cfg(kani)]", "proofs"))?;
    fixture.write("src/proofs.rs", "include!(\"frag.rs\");\n")?;
    fixture.write("src/frag.rs", PROOF_FILE)?;
    let index = fixture_index(&fixture)?;
    assert!(
        index.include_parents.contains_key(Path::new("src/frag.rs")),
        "the gated proofs.rs includes frag.rs"
    );
    let path = "src/frag.rs";
    let observed = observe(&fixture, path, &helper_diff(path, "", 4))?;
    assert!(
        observed.kani_module_file,
        "Kani-only context crosses the include"
    );
    assert_verification_only(
        &observed,
        "include!(\"frag.rs\") inside #[cfg(kani)] mod proofs;",
    );
    Ok(())
}

/// An unresolved nested-module `include!` of a package `.rs` file is already
/// in the text scan, so it keeps the verdict unless that file itself could
/// declare the module.
#[test]
fn a_nested_include_blocks_the_verdict_only_when_it_could_declare_the_module() -> Result<(), String>
{
    for (label, body, kani_only) in [
        (
            "nested-include-clean",
            "pub fn extra() -> u32 { 1 }\n",
            true,
        ),
        ("nested-include-declares", "mod proofs;\n", false),
    ] {
        let fixture = Fixture::new(label)?;
        fixture.write(
            "src/lib.rs",
            &format!(
                "{}mod extra {{\n    include!(\"extra_body.rs\");\n}}\n",
                parent_source("#[cfg(kani)]", "proofs")
            ),
        )?;
        fixture.write("src/extra_body.rs", body)?;
        fixture.write("src/proofs.rs", PROOF_FILE)?;
        let index = fixture_index(&fixture)?;
        assert!(
            index.include_limitations.iter().any(|limitation| {
                limitation.reason_code == "rust_include_nested_module_context"
            }),
            "{label}: the inline-module include is an unresolved include"
        );
        let path = "src/proofs.rs";
        let observed = observe(&fixture, path, &helper_diff(path, "", 4))?;
        if kani_only {
            assert!(observed.kani_module_file, "{label}: verdict kept");
            assert_verification_only(&observed, label);
        } else {
            assert_production(&observed, label);
        }
    }
    Ok(())
}

/// A binary root whose only owner of `proofs.rs` sits inside a macro token
/// tree, where the parser producer captures nothing.
const MACRO_OWNER_MAIN: &str = "macro_rules! declare { ($($item:item)*) => { $($item)* }; }\ndeclare! { mod proofs; }\nfn charge(total: u32) -> u32 { total }\nfn main() {}\n";

/// The index the run builds: every fixture file's bytes read through the
/// snapshot authority (`committed_source`), then the loaded-files build.
fn loaded_index(fixture: &Fixture) -> Result<crate::analysis::facts::RustIndex, String> {
    let paths = ["src/lib.rs", "src/main.rs", "src/proofs.rs"];
    let mut loaded = Vec::new();
    for path in paths {
        let bytes =
            crate::analysis::committed_source::read_source_bytes(&fixture.0, Path::new(path))
                .map_err(|error| error.to_string())?
                .ok_or_else(|| format!("{path} has no snapshot bytes"))?;
        loaded.push((PathBuf::from(path), bytes));
    }
    Ok(
        crate::analysis::rust_index::build_analysis_index_from_loaded_files(
            &fixture.0,
            &loaded,
            &[],
        )?
        .index,
    )
}

fn kani_only(index: &crate::analysis::facts::RustIndex, file: &str) -> bool {
    index.cfg_kani_module_files.contains(Path::new(file))
}

/// #13736 review: in committed mode the index reads HEAD bytes, so the owner
/// check must too. A dirty edit that deletes the macro-wrapped owner from the
/// working tree must not make the HEAD file look owner-free.
#[test]
fn committed_mode_judges_owners_from_head_not_the_dirty_worktree() -> Result<(), String> {
    use crate::analysis::committed_source::{CommittedSourceOverlay, with_overlay};
    let fixture = Fixture::new("committed-owner")?;
    fixture.write("src/lib.rs", &parent_source("#[cfg(kani)]", "proofs"))?;
    fixture.write("src/proofs.rs", PROOF_FILE)?;
    // Working tree: the invisible owner was deleted (uncommitted).
    fixture.write(
        "src/main.rs",
        "fn charge(total: u32) -> u32 { total }\nfn main() {}\n",
    )?;
    let worktree = loaded_index(&fixture)?;
    assert!(
        kani_only(&worktree, "src/proofs.rs"),
        "against the working tree alone the module is Kani-only"
    );
    let overlay = CommittedSourceOverlay::from_entries(
        &fixture.0,
        [("src/main.rs", Some(MACRO_OWNER_MAIN.as_bytes()))],
    );
    let committed = with_overlay(Some(std::sync::Arc::new(overlay)), || {
        loaded_index(&fixture)
    })?;
    let main_source = committed
        .files()
        .get(Path::new("src/main.rs"))
        .ok_or("fixture must index src/main.rs")?
        .data()
        .source
        .clone();
    assert!(
        main_source.contains("declare! { mod proofs; }"),
        "the committed index holds the HEAD bytes of main.rs"
    );
    assert!(
        !kani_only(&committed, "src/proofs.rs"),
        "the HEAD owner inside a macro token tree refuses the verdict"
    );
    Ok(())
}

/// #13736 review: counts are compared per file. A captured declaration in
/// a file the discovery walk skips (`src/target/`) must not cancel the
/// uncaptured owner in `lib.rs`.
#[test]
fn a_surplus_in_one_file_cannot_cancel_a_deficit_in_another() -> Result<(), String> {
    let fixture = Fixture::new("offsetting-counts")?;
    fixture.write(
        "src/lib.rs",
        &format!(
            "{}macro_rules! declare {{ ($($item:item)*) => {{ $($item)* }}; }}\ndeclare! {{ mod proofs; }}\n",
            parent_source("#[cfg(kani)]", "proofs")
        ),
    )?;
    fixture.write("src/proofs.rs", PROOF_FILE)?;
    fixture.write("src/target/mod.rs", "mod proofs;\n")?;
    let files = ["src/lib.rs", "src/proofs.rs", "src/target/mod.rs"]
        .map(PathBuf::from)
        .to_vec();
    let index = crate::analysis::facts::build_index(&fixture.0, &files)?;
    let captured = |file: &str| -> Result<Vec<String>, String> {
        Ok(index
            .files()
            .get(Path::new(file))
            .ok_or_else(|| format!("fixture must index {file}"))?
            .data()
            .module_declarations
            .iter()
            .map(|declaration| declaration.name.clone())
            .collect())
    };
    assert_eq!(
        captured("src/lib.rs")?,
        ["proofs"],
        "the macro owner is not captured"
    );
    assert_eq!(captured("src/target/mod.rs")?, ["proofs"]);
    assert!(
        !kani_only(&index, "src/proofs.rs"),
        "lib.rs holds two `mod proofs;` but one captured declaration"
    );
    Ok(())
}

/// A package file the index did not take in may hold an owner; the verdict
/// waits for a complete package rather than reading the file behind the
/// snapshot.
#[test]
fn an_unindexed_package_file_refuses_only_when_it_could_own_the_child() -> Result<(), String> {
    let fixture = Fixture::new("unindexed-file")?;
    fixture.write("src/lib.rs", &parent_source("#[cfg(kani)]", "proofs"))?;
    fixture.write("src/proofs.rs", PROOF_FILE)?;
    fixture.write("src/other.rs", "pub fn other() {}\n")?;
    let partial = |fixture: &Fixture| {
        crate::analysis::facts::build_index(
            &fixture.0,
            &["src/lib.rs", "src/proofs.rs"].map(PathBuf::from),
        )
    };
    let cases = [
        ("src/main.rs", "fn main() {}\n", true),
        ("src/main.rs", "mod proofs;\nfn main() {}\n", false),
        (
            "src/main.rs",
            "mod inner {\n    mod proofs;\n}\nfn main() {}\n",
            false,
        ),
        (
            "build.rs",
            "#[path = \"src/other.rs\"]\nmod other;\nfn main() {}\n",
            true,
        ),
        (
            "build.rs",
            "#[path = \"src/proofs.rs\"]\nmod helper;\nfn main() {}\n",
            false,
        ),
        (
            "build.rs",
            "macro_rules! m { ($n:ident) => { mod $n; } }\nfn main() {}\n",
            false,
        ),
    ];
    for (file, source, grants) in cases {
        fixture.write(file, source)?;
        let index = partial(&fixture)?;
        assert!(
            index.files().get(Path::new(file)).is_none(),
            "the partial index leaves {file} out"
        );
        assert!(
            index.files().get(Path::new("src/proofs.rs")).is_some(),
            "the partial index holds the child"
        );
        assert_eq!(
            kani_only(&index, "src/proofs.rs"),
            grants,
            "unindexed {file} with {source:?}"
        );
        fs::remove_file(fixture.0.join(file)).map_err(|error| error.to_string())?;
    }
    let complete = fixture_index(&fixture)?;
    assert!(
        kani_only(&complete, "src/proofs.rs"),
        "complete index grants"
    );
    Ok(())
}
