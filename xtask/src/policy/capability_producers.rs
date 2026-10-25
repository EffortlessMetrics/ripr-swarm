//! `cargo xtask check-capability-producers`: producer inventory against the
//! binary-resident capability registry (#5531).
//!
//! Distinct from `check-capabilities`, which validates the product-status
//! manifest `metrics/capabilities.toml` against `docs/CAPABILITY_MATRIX.md`;
//! this gate does not read that manifest.
//!
//! The registry (`domain::language_capability::CAPABILITY_CELLS`) is read
//! through its existing exported surface, `ripr capabilities --json`. The
//! in-crate tests already own the table's internal facts (cell pins, the
//! docs drift gate, per-framework dispositions); this gate binds only to
//! repository-level inventory those tests cannot see:
//!
//! - every `LanguageId` variant declared in source is exported by the
//!   registry (closes the `LanguageId::ALL` hand-kept-list hole a Rust test
//!   cannot enumerate);
//! - every registered language has at least one cell, and every cell names a
//!   registered language;
//! - the `lang-*` Cargo features, the registry's `required_feature` values and
//!   the adapter directories under `analysis/language/` name the same
//!   producer set, so a new adapter or feature cannot land without a
//!   registered language (and so a cell);
//! - every cell `proof_refs` entry resolves to a spec file;
//! - adapter_compiled matches the selected Cargo language features, including
//!   minimal and individual language builds (JavaScript shares TypeScript).
//!
//! Limits (not enforced here): cell state, tier ceilings and docs text
//! (in-crate tests and `check-support-tiers`); finer producers inside an
//! adapter such as framework detectors (per-language disposition tests); the
//! relation to `metrics/capabilities.toml` (bound by decision, not checked).
//! One selected build is checked per invocation; this is not a matrix runner.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{FixKind, PolicyDisclosure, PolicyReportSpec, read_text_lossy};

mod features;

use features::{BuildSelection, compiled_feature_violations};

#[cfg(test)]
mod tests;

const REGISTRY_KIND: &str = "ripr_language_capabilities";
const REGISTRY_SCHEMA: &str = "0.1";
const LANGUAGE_ID_SOURCE: &str = "crates/ripr/src/domain/language.rs";
const RIPR_CARGO_TOML: &str = "crates/ripr/Cargo.toml";
const ADAPTER_ROOT: &str = "crates/ripr/src/analysis/language";
const SPEC_DIR: &str = "docs/specs";

/// Repository facts the registry is compared against.
pub(super) struct Inventory {
    /// Wire (lowercase) names of the `LanguageId` variants declared in source.
    pub(super) enum_languages: BTreeSet<String>,
    /// `lang-*` feature names declared in `crates/ripr/Cargo.toml`.
    pub(super) lang_features: BTreeSet<String>,
    /// Adapter directory names under `analysis/language/`.
    pub(super) adapter_dirs: BTreeSet<String>,
    /// `RIPR-SPEC-NNNN` ids that have a spec file.
    pub(super) spec_ids: BTreeSet<String>,
}

pub(crate) fn check_capability_producers(args: &[String]) -> Result<(), String> {
    let options = parse_args(args)?;
    let inventory = read_inventory(Path::new("."))?;
    let cargo_text = read_text_lossy(Path::new(RIPR_CARGO_TOML))?;
    let enabled = options.build.enabled_features(&cargo_text)?;
    let (registry_text, source_note) = match &options.registry_file {
        Some(path) => (
            read_text_lossy(path)?,
            format!(
                "registry read from `{}` (offline input, not built from this tree; it can be stale)",
                path.display()
            ),
        ),
        None => (
            registry_from_binary(&options.build)?,
            "registry read from `ripr capabilities --json` of a binary built from this tree"
                .to_string(),
        ),
    };
    let registry: Value = serde_json::from_str(&registry_text)
        .map_err(|err| format!("capability registry output is not JSON: {err}"))?;
    let mut violations = capability_violations(&registry, &inventory);
    violations.extend(compiled_feature_violations(&registry, &enabled));
    let digest = registry
        .get("semantic_digest")
        .and_then(Value::as_str)
        .unwrap_or("<missing>");
    let disclosures = vec![PolicyDisclosure {
        heading: "Scope and limits".to_string(),
        intro: format!("{source_note}. Registry digest: `{digest}`. Expected compiled language features: `{enabled:?}`."),
        items: vec![
            "Enforced: LanguageId variants vs registry languages; cells per language; lang-* features vs required_feature vs adapter directories; proof_refs resolve to spec files; adapter_compiled matches selected Cargo language features.".to_string(),
            "Not enforced here: cell states and tiers (in-crate tests), docs text (check-support-tiers and the drift test), framework-level producers (per-language disposition tests), metrics/capabilities.toml. Only the selected build is checked; other feature combinations require separate invocations.".to_string(),
        ],
    }];
    crate::finish_policy_report_with_disclosures(
        PolicyReportSpec {
            report_file: "capability-producers.md",
            check: "check-capability-producers",
            why_it_matters: "A new language, adapter or Cargo language feature without a capability registration would read as `not_proven` only by accident of absence. It reports an unregistered producer as a failure and is hosted in rust-gates.yml (and the docs-gate for docs/specs changes), so it stops a merge.",
            fix_kind: FixKind::AuthorDecisionRequired,
            recommended_fixes: &[
                "Add the language to `LanguageId::ALL` and register at least one cell in `domain/language_capability.rs` with an explicit state (`not_proven` is a valid disposition).",
                "Keep each `lang-*` feature, its adapter directory and the registry `required_feature` aligned.",
                "Point `proof_refs` at an existing `docs/specs/RIPR-SPEC-NNNN-*.md`.",
            ],
            rerun_command: "cargo xtask check-capability-producers",
            exception_template: None,
        },
        &violations,
        &disclosures,
    )
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    registry_file: Option<PathBuf>,
    build: BuildSelection,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = args.iter();
    let mut features_seen = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--registry-json" if options.registry_file.is_none() => {
                options.registry_file = Some(PathBuf::from(
                    args.next().ok_or("--registry-json needs a path")?,
                ));
            }
            "--no-default-features" if !options.build.no_default_features => {
                options.build.no_default_features = true;
            }
            "--features" if !features_seen => {
                features_seen = true;
                let names = args
                    .next()
                    .ok_or("--features needs comma-separated lang-* names")?;
                for name in names.split(',') {
                    if !name.starts_with("lang-")
                        || !options.build.features.insert(name.to_string())
                    {
                        return Err(format!("invalid or duplicate language feature `{name}`"));
                    }
                }
            }
            _ => {
                return Err(format!(
                    "unknown or repeated argument `{flag}`; usage: cargo xtask check-capability-producers [--registry-json <path>] [--no-default-features] [--features <lang-*,...>]"
                ));
            }
        }
    }
    Ok(options)
}

fn registry_from_binary(build: &BuildSelection) -> Result<String, String> {
    let timeout = crate::tool_build_timeout()?;
    let build_args = build.cargo_args();
    crate::run_output_owned_with_timeout(
        "cargo",
        &build_args,
        timeout,
        "check-capability-producers build of ripr",
    )?;
    let binary = crate::ripr_debug_binary();
    if !binary.exists() {
        return Err(format!(
            "check-capability-producers build reported success but {} is absent",
            binary.display()
        ));
    }
    let program = binary.display().to_string();
    let run_args = ["capabilities", "--json"].map(String::from);
    crate::run_output_owned_with_timeout(
        &program,
        &run_args,
        timeout,
        "check-capability-producers `ripr capabilities --json`",
    )
}

fn read_inventory(root: &Path) -> Result<Inventory, String> {
    let enum_source = read_text_lossy(&root.join(LANGUAGE_ID_SOURCE))?;
    let enum_languages = language_id_variants(&enum_source)?;

    let cargo_text = read_text_lossy(&root.join(RIPR_CARGO_TOML))?;
    let cargo: toml::Value = toml::from_str(&cargo_text)
        .map_err(|err| format!("{RIPR_CARGO_TOML} is not valid TOML: {err}"))?;
    let lang_features = cargo
        .get("features")
        .and_then(toml::Value::as_table)
        .map(|table| {
            table
                .keys()
                .filter(|name| name.starts_with("lang-"))
                .cloned()
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();

    let mut adapter_dirs = BTreeSet::new();
    let entries = std::fs::read_dir(root.join(ADAPTER_ROOT))
        .map_err(|err| format!("failed to read {ADAPTER_ROOT}: {err}"))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("failed to read {ADAPTER_ROOT} entry: {err}"))?;
        if entry.path().is_dir() {
            adapter_dirs.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }

    let mut spec_ids = BTreeSet::new();
    let specs = std::fs::read_dir(root.join(SPEC_DIR))
        .map_err(|err| format!("failed to read {SPEC_DIR}: {err}"))?;
    for entry in specs {
        let entry = entry.map_err(|err| format!("failed to read {SPEC_DIR} entry: {err}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(id) = spec_id_of_file(&name) {
            spec_ids.insert(id);
        }
    }

    Ok(Inventory {
        enum_languages,
        lang_features,
        adapter_dirs,
        spec_ids,
    })
}

/// `RIPR-SPEC-0026-language-adapter-contract.md` -> `RIPR-SPEC-0026`.
fn spec_id_of_file(name: &str) -> Option<String> {
    let rest = name.strip_prefix("RIPR-SPEC-")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || !name.ends_with(".md") {
        return None;
    }
    Some(format!("RIPR-SPEC-{digits}"))
}

/// Lowercase wire names of the variants in `pub enum LanguageId { ... }`.
pub(super) fn language_id_variants(source: &str) -> Result<BTreeSet<String>, String> {
    let mut inside = false;
    let mut variants = BTreeSet::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if !inside {
            inside = trimmed.starts_with("pub enum LanguageId") && trimmed.ends_with('{');
            continue;
        }
        if trimmed.starts_with('}') {
            break;
        }
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        let name = trimmed.trim_end_matches(',');
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(format!(
                "unrecognized line in `LanguageId` ({LANGUAGE_ID_SOURCE}): `{trimmed}`; update check-capability-producers"
            ));
        }
        variants.insert(name.to_ascii_lowercase());
    }
    if variants.is_empty() {
        return Err(format!(
            "found no `pub enum LanguageId` variants in {LANGUAGE_ID_SOURCE}; the inventory cannot be established"
        ));
    }
    Ok(variants)
}

/// Pure comparison of the exported registry with the repository inventory.
pub(super) fn capability_violations(registry: &Value, inventory: &Inventory) -> Vec<String> {
    let mut violations = Vec::new();
    if registry.get("kind").and_then(Value::as_str) != Some(REGISTRY_KIND) {
        violations.push(format!("registry `kind` is not `{REGISTRY_KIND}`"));
    }
    if registry.get("schema_version").and_then(Value::as_str) != Some(REGISTRY_SCHEMA) {
        violations.push(format!(
            "registry `schema_version` is not `{REGISTRY_SCHEMA}`; update check-capability-producers with the schema"
        ));
    }
    if registry
        .get("semantic_digest")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        violations.push("registry `semantic_digest` is missing or empty".to_string());
    }
    let (Some(languages), Some(cells)) = (
        registry.get("languages").and_then(Value::as_array),
        registry.get("cells").and_then(Value::as_array),
    ) else {
        violations.push("registry is missing the `languages` or `cells` array".to_string());
        return violations;
    };

    // language wire name -> required feature
    let mut registered: BTreeMap<String, String> = BTreeMap::new();
    for entry in languages {
        let language = entry.get("language").and_then(Value::as_str);
        let feature = entry.get("required_feature").and_then(Value::as_str);
        match (language, feature) {
            (Some(language), Some(feature)) => {
                if registered
                    .insert(language.to_string(), feature.to_string())
                    .is_some()
                {
                    violations.push(format!(
                        "registry lists language `{language}` more than once"
                    ));
                }
            }
            _ => violations.push(format!(
                "registry language entry lacks `language` or `required_feature`: {entry}"
            )),
        }
    }
    if registered.is_empty() {
        violations.push("registry exports no languages; the inventory is not established".into());
    }

    let registered_names: BTreeSet<String> = registered.keys().cloned().collect();
    for missing in inventory.enum_languages.difference(&registered_names) {
        violations.push(format!(
            "`LanguageId::{missing}` is declared in {LANGUAGE_ID_SOURCE} but not exported by the capability registry; add it to `LanguageId::ALL` and register a cell"
        ));
    }
    for extra in registered_names.difference(&inventory.enum_languages) {
        violations.push(format!(
            "registry exports language `{extra}` that is not a `LanguageId` variant in {LANGUAGE_ID_SOURCE}"
        ));
    }

    let mut cells_per_language: BTreeMap<&str, usize> = BTreeMap::new();
    for cell in cells {
        match cell.get("language").and_then(Value::as_str) {
            Some(language) => *cells_per_language.entry(language).or_default() += 1,
            None => violations.push(format!("registry cell lacks `language`: {cell}")),
        }
        let id = cell.get("id").and_then(Value::as_str).unwrap_or("<no id>");
        let references = cell.get("proof_refs").and_then(Value::as_array);
        match references {
            None => violations.push(format!("cell `{id}` lacks a `proof_refs` array")),
            Some(references) if references.is_empty() => {
                violations.push(format!("cell `{id}` has empty `proof_refs`"));
            }
            Some(references) => {
                for reference in references {
                    match reference.as_str() {
                        None => violations.push(format!(
                            "cell `{id}` has a non-string `proof_refs` entry: {reference}"
                        )),
                        Some(reference) if !inventory.spec_ids.contains(reference) => {
                            violations.push(format!(
                                "cell `{id}` proof ref `{reference}` has no {SPEC_DIR}/{reference}-*.md"
                            ));
                        }
                        Some(_) => {}
                    }
                }
            }
        }
    }
    for language in &registered_names {
        if !cells_per_language.contains_key(language.as_str()) {
            violations.push(format!(
                "registered language `{language}` has no capability cell; an absent adapter disposition must be an explicit `not_proven` cell"
            ));
        }
    }
    for language in cells_per_language.keys() {
        if !registered_names.contains(*language) {
            violations.push(format!(
                "cells name language `{language}` that is not a registered language"
            ));
        }
    }

    let required_features: BTreeSet<String> = registered.values().cloned().collect();
    for feature in inventory.lang_features.difference(&required_features) {
        violations.push(format!(
            "Cargo feature `{feature}` ({RIPR_CARGO_TOML}) is not the `required_feature` of any registered language"
        ));
    }
    for feature in required_features.difference(&inventory.lang_features) {
        violations.push(format!(
            "registry `required_feature` `{feature}` is not a Cargo feature in {RIPR_CARGO_TOML}"
        ));
    }
    for dir in &inventory.adapter_dirs {
        let feature = format!("lang-{dir}");
        if !required_features.contains(&feature) {
            violations.push(format!(
                "adapter directory `{ADAPTER_ROOT}/{dir}` has no registered language with `required_feature` `{feature}`"
            ));
        }
    }
    for feature in &required_features {
        let dir = feature.strip_prefix("lang-").unwrap_or(feature);
        if !inventory.adapter_dirs.contains(dir) {
            violations.push(format!(
                "registered `required_feature` `{feature}` has no adapter directory `{ADAPTER_ROOT}/{dir}`"
            ));
        }
    }
    violations
}
