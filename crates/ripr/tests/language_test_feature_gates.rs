//! A language-named integration test must compile only when its language
//! adapter is built.
//!
//! `cargo test -p ripr --no-default-features --features lang-rust` runs every
//! file under `tests/`. A `typescript_*.rs` file without
//! `#![cfg(feature = "lang-typescript")]` still compiles there and then fails,
//! because the adapter it exercises is absent (#15331's wording test did this
//! after it landed). This scan runs in every feature selection, so a new
//! ungated file fails here in the default build too, before the Rust-only CI
//! step finds it.

use std::path::{Path, PathBuf};

/// File-name prefix and the feature its tests need.
const LANGUAGE_PREFIXES: &[(&str, &str)] = &[
    ("typescript_", "lang-typescript"),
    ("python_", "lang-python"),
    ("perl_", "lang-perl"),
];

/// Language-named files that deliberately carry no language gate, each with
/// the reason. An entry must still exist and still be ungated, so a stale
/// exemption fails instead of silently widening the rule.
const UNGATED: &[(&str, &str)] = &[(
    "python_repair_attempt.rs",
    "drives the language-independent `ripr agent repair` driver; all its tests \
     pass in the Rust-only build (27 on Windows advisory run 45008347596, 31 on Linux)",
)];

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// The file-level `#![cfg(...)]` attributes, read until the first line that
/// is not a comment, blank, or inner attribute.
fn file_level_cfgs(source: &str) -> Vec<String> {
    let mut cfgs = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if trimmed.starts_with("#![") {
            if trimmed.starts_with("#![cfg(") {
                cfgs.push(trimmed.to_string());
            }
            continue;
        }
        break;
    }
    cfgs
}

fn requires_feature(cfgs: &[String], feature: &str) -> bool {
    let needle = format!("feature = \"{feature}\"");
    // `any(...)` would let another feature enable the file, so it does not
    // count as a gate for this language.
    cfgs.iter()
        .any(|cfg| cfg.contains(&needle) && !cfg.contains("any("))
}

#[test]
fn language_named_tests_carry_their_feature_gate() -> Result<(), String> {
    let dir = tests_dir();
    let entries =
        std::fs::read_dir(&dir).map_err(|err| format!("read {}: {err}", dir.display()))?;
    let mut subjects = 0usize;
    let mut violations = Vec::new();
    for entry in entries {
        let path = entry.map_err(|err| format!("read entry: {err}"))?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !path.is_file() || !name.ends_with(".rs") {
            continue;
        }
        let Some((_, feature)) = LANGUAGE_PREFIXES
            .iter()
            .find(|(prefix, _)| name.starts_with(prefix))
        else {
            continue;
        };
        subjects += 1;
        let source = std::fs::read_to_string(&path).map_err(|err| format!("read {name}: {err}"))?;
        let gated = requires_feature(&file_level_cfgs(&source), feature);
        let exempt = UNGATED.iter().any(|(file, _)| *file == name);
        match (gated, exempt) {
            (true, true) => {
                violations.push(format!("{name} is gated now; remove its UNGATED exemption"))
            }
            (false, false) => violations.push(format!(
                "{name} needs `#![cfg(feature = \"{feature}\")]` before its first item \
                 (or an UNGATED entry with the reason it passes without {feature})"
            )),
            (false, true) | (true, false) => {}
        }
    }
    for (file, _) in UNGATED {
        if !dir.join(file).is_file() {
            violations.push(format!("UNGATED names missing file {file}"));
        }
    }
    // The scan must see the language test families it guards; an empty or
    // misdirected scan is not a pass.
    if subjects < 20 {
        violations.push(format!(
            "scanned only {subjects} language-named test files under {}",
            dir.display()
        ));
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations.join("\n"))
    }
}

#[test]
fn gate_reader_rejects_ungated_and_borrowed_gates() -> Result<(), String> {
    let gated = "//! doc\n\n#![cfg(feature = \"lang-typescript\")]\n\nuse std::path::Path;\n";
    let ungated = "//! doc\n\nuse std::path::Path;\n#![cfg(feature = \"lang-typescript\")]\n";
    let combined = "#![cfg(all(feature = \"lang-perl\", unix))]\nfn f() {}\n";
    let either = "#![cfg(any(feature = \"lang-python\", feature = \"lang-typescript\"))]\n";
    let cases = [
        (gated, "lang-typescript", true),
        (gated, "lang-python", false),
        (ungated, "lang-typescript", false),
        (combined, "lang-perl", true),
        (either, "lang-python", false),
    ];
    for (source, feature, expected) in cases {
        let actual = requires_feature(&file_level_cfgs(source), feature);
        if actual != expected {
            return Err(format!(
                "requires_feature({feature}) = {actual}, expected {expected} for:\n{source}"
            ));
        }
    }
    Ok(())
}
