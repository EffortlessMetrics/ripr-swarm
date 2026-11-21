//! Gate on the file lists Cargo would ship for the published crates.
//!
//! `cargo package -p ripr --list` / `-p ripr-contract --list` must not carry
//! test-only sources: the `src/testing/**` support tree, out-of-line unit-test
//! modules (`tests.rs`, `tests/**`, `*_tests.rs`), corpus-test modules
//! (`corpus_tests`) or Kani proof modules (`kani_proofs`). Those files inflate
//! the crates.io upload, can push it over the size limit, and ship test
//! scaffolding to downstream consumers. Fail-closed: a package list that
//! cannot be obtained or is empty is a violation, never a silent pass.
//!
//! A bare name heuristic false-positives on production modules whose names
//! merely end in `_tests.rs` (they extract or index *other* code's tests).
//! Those are exempted only through the exact-path, per-package
//! [`PRODUCTION_ALLOWLIST`] below, each entry carrying its reason; the list
//! mirrors the re-inclusions after `!src/**/*_tests.rs` in
//! `crates/ripr/Cargo.toml`. Any other `*_tests.rs` — including a new
//! production module not yet allowlisted — is still flagged, so the gate
//! stays fail-closed and adding an entry is a deliberate, reviewed act.
//! `tests/**` is likewise scoped to the crate root and `src/**`: the
//! `examples/*/tests/**` trees are shipped deliberately (see the `ripr`
//! manifest) as the analyzable sample workspace, not as this crate's tests.

use crate::{FixKind, PolicyReportSpec, finish_policy_report};

const PACKAGES: &[&str] = &["ripr", "ripr-contract"];

/// Production sources exempt from the `*_tests.rs` rule, as
/// `(package, path, reason)`. Exact paths only — no globs — and scoped to
/// the package that actually declares the module without `#[cfg(test)]`.
/// Keep in sync with the re-included paths in `crates/ripr/Cargo.toml`;
/// `production_allowlist_mirrors_manifest_reinclusions` pins that sync.
const PRODUCTION_ALLOWLIST: &[(&str, &str, &str)] = &[
    (
        "ripr",
        "src/analysis/classify/related_tests.rs",
        "unconditional `mod related_tests;` in classify/mod.rs, re-exported for production classification (RelatedTestCandidateIndex)",
    ),
    (
        "ripr",
        "src/analysis/facts/macro_generated_tests.rs",
        "unconditional module in facts/mod.rs; register_macro_generated_tests is called on the production fact path",
    ),
    (
        "ripr",
        "src/analysis/facts/parameterized_tests.rs",
        "unconditional module in facts/mod.rs; promote_explicit_test_case_functions is called on the production fact path",
    ),
    (
        "ripr",
        "src/analysis/language/python/doctest_tests.rs",
        "unconditional module in language/python.rs; extracts Python doctest facts for production analysis",
    ),
    (
        "ripr",
        "src/analysis/language/python/owners_tests.rs",
        "unconditional module in language/python.rs; name means Python owners+tests extraction (extract_owners/extract_tests), used by production source-facts code",
    ),
    (
        "ripr",
        "src/analysis/language/python/related_tests.rs",
        "unconditional module in language/python.rs; production related-test lookup for Python analysis",
    ),
    (
        "ripr",
        "src/analysis/language/typescript/related_tests.rs",
        "unconditional pub(crate) module in typescript/mod.rs; production related-test lookup for TypeScript analysis",
    ),
    (
        "ripr",
        "src/analysis/test_grip_evidence/related_tests.rs",
        "unconditional module in test_grip_evidence.rs; find_related_tests_with_context is production test-grip evidence",
    ),
];

pub(crate) fn check_package_content() -> Result<(), String> {
    let mut violations = Vec::new();
    for package in PACKAGES {
        let command = format!("cargo package -p {package} --list");
        let package_list = crate::run_output("cargo", &["package", "-p", package, "--list"])
            .map_err(|err| {
                format!(
                    "{command} failed; packaged content cannot be judged without the list cargo would ship: {err}"
                )
            })?;
        violations.extend(package_content_violations(package, &package_list));
    }
    finish_policy_report(report_spec(), &violations)
}

fn report_spec() -> PolicyReportSpec<'static> {
    PolicyReportSpec {
        report_file: "package-content.md",
        check: "check-package-content",
        why_it_matters: "the `.crate` Cargo ships must contain only production sources. Test-only modules (`src/testing/**`, `tests.rs`, `tests/**`, `*_tests.rs`, `corpus_tests`, `kani_proofs`) inflate the upload toward the crates.io size limit and ship test scaffolding downstream. Production modules named `*_tests.rs` are exempt only via the exact-path allowlist in package_content.rs.",
        fix_kind: FixKind::AuthorDecisionRequired,
        recommended_fixes: &[
            "Exclude test-only paths from the crate `include` list in its Cargo.toml (negated patterns after the production includes), or move the test-only code behind the publish stage.",
            "If a flagged `*_tests.rs` file is production code (declared without #[cfg(test)] and called from production code), add its exact path with a reason to PRODUCTION_ALLOWLIST in xtask/src/policy/package_content.rs and re-include it explicitly in the crate Cargo.toml after the exclusion, as the existing production `*_tests.rs` modules do; otherwise exclude it from the package.",
            "Keep `src/testing/**` helpers out of the packaged `src/**` glob.",
        ],
        rerun_command: "cargo xtask check-package-content",
        exception_template: None,
    }
}

/// Pure classifier over one `cargo package --list` output, so synthetic list
/// fixtures can pin the contract without invoking Cargo.
pub(crate) fn package_content_violations(package: &str, package_list: &str) -> Vec<String> {
    let listed: Vec<String> = package_list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(normalize_slashes)
        .collect();
    if listed.is_empty() {
        return vec![format!(
            "`cargo package -p {package} --list` produced no files; packaged content cannot be judged from an empty list"
        )];
    }
    listed
        .iter()
        .filter(|path| !is_allowlisted_production(package, path))
        .filter_map(|path| {
            forbidden_rule(path).map(|rule| {
                format!(
                    "`cargo package -p {package} --list` includes forbidden test-only path `{path}` ({rule}); test-only sources must not ship in the published crate"
                )
            })
        })
        .collect()
}

fn is_allowlisted_production(package: &str, path: &str) -> bool {
    PRODUCTION_ALLOWLIST
        .iter()
        .any(|(pkg, allowed, _reason)| *pkg == package && *allowed == path)
}

fn normalize_slashes(path: &str) -> String {
    path.replace('\\', "/")
}

fn forbidden_rule(path: &str) -> Option<&'static str> {
    let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
    let basename = components.last().copied().unwrap_or("");
    if components.first().copied() == Some("src") && components.get(1).copied() == Some("testing") {
        return Some("src/testing/**");
    }
    if components.iter().any(|c| c.contains("corpus_tests")) {
        return Some("corpus_tests path");
    }
    if components
        .iter()
        .any(|c| *c == "kani_proofs" || *c == "kani_proofs.rs")
    {
        return Some("kani_proofs path");
    }
    if basename == "tests.rs" {
        return Some("tests.rs module");
    }
    if basename.ends_with("_tests.rs") {
        return Some("*_tests.rs module");
    }
    // Only this crate's own test trees: a crate-root `tests/**` or a
    // `src/**/tests/**`. `examples/*/tests/**` is the deliberately shipped
    // sample workspace, not this crate's tests.
    let parents = &components[..components.len().saturating_sub(1)];
    if components.first().copied() == Some("tests")
        || (components.first().copied() == Some("src") && parents.contains(&"tests"))
    {
        return Some("tests/** path");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLEAN_LIST: &str = "\
Cargo.toml
Cargo.toml.orig
README.md
build.rs
src/lib.rs
src/main.rs
src/analysis/mod.rs
src/analysis/facts/doctests.rs
src/domain/mod.rs
examples/sample/src/lib.rs
LICENSE-MIT
";

    /// Every allowlisted production `*_tests.rs` path, as `cargo package`
    /// would list it for `ripr`. These are the negative control: production
    /// code with a test-shaped name must not be flagged.
    const PRODUCTION_LIST: &str = "\
src/analysis/classify/related_tests.rs
src/analysis/facts/macro_generated_tests.rs
src/analysis/facts/parameterized_tests.rs
src/analysis/language/python/doctest_tests.rs
src/analysis/language/python/owners_tests.rs
src/analysis/language/python/related_tests.rs
src/analysis/language/typescript/related_tests.rs
src/analysis/test_grip_evidence/related_tests.rs
examples/sample/tests/pricing.rs
";

    const VIOLATING_LIST: &str = "\
Cargo.toml
src/lib.rs
src/testing/mod.rs
src/testing/fixture_git.rs
src/domain/tests.rs
src/domain/identity/tests.rs
src/analysis/widget_tests.rs
src/domain/refusal_remedy_corpus_tests.rs
src/analysis/classify/decision/kani_proofs.rs
src\\testing\\cwd_lock.rs
tests/integration.rs
";

    #[test]
    fn clean_list_has_no_violations() {
        assert_eq!(
            package_content_violations("ripr", CLEAN_LIST),
            Vec::<String>::new()
        );
        // `doctests.rs` is production (doc-test facts), not a test module.
        assert!(forbidden_rule("src/analysis/facts/doctests.rs").is_none());
    }

    #[test]
    fn production_tests_named_modules_are_not_flagged() {
        // Negative control: each allowlisted production `*_tests.rs` module
        // (and the deliberately shipped sample-workspace test) passes.
        assert_eq!(
            package_content_violations("ripr", PRODUCTION_LIST),
            Vec::<String>::new(),
        );
        for (_package, path, reason) in PRODUCTION_ALLOWLIST {
            assert!(!reason.is_empty(), "allowlist entry {path} needs a reason");
            assert_eq!(
                package_content_violations("ripr", path),
                Vec::<String>::new(),
                "allowlisted production path {path} must not be flagged"
            );
        }
    }

    #[test]
    fn production_allowlist_is_package_scoped_and_exact() {
        // The same path under the other published crate is not production
        // there, so it stays flagged (fail-closed, no cross-package leak).
        let violations =
            package_content_violations("ripr-contract", "src/analysis/classify/related_tests.rs\n");
        assert_eq!(violations.len(), 1, "{violations:?}");
        // A near-miss name is not the allowlisted path.
        let violations =
            package_content_violations("ripr", "src/analysis/classify/extra_tests.rs\n");
        assert_eq!(violations.len(), 1, "{violations:?}");
    }

    #[test]
    fn production_allowlist_mirrors_manifest_reinclusions() -> Result<(), String> {
        // The allowlist and the manifest re-inclusions after
        // `!src/**/*_tests.rs` are two spellings of one decision; drift
        // between them means the packaged crate either drops production
        // code or ships a module the gate still flags.
        let manifest = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/ripr/Cargo.toml"),
        )
        .map_err(|err| format!("ripr manifest must be readable from the xtask test: {err}"))?;
        for (package, path, _reason) in PRODUCTION_ALLOWLIST {
            assert_eq!(*package, "ripr");
            assert!(
                manifest.contains(&format!("\"{path}\"")),
                "allowlisted {path} is not re-included in crates/ripr/Cargo.toml"
            );
        }
        Ok(())
    }

    #[test]
    fn violating_list_flags_every_category() {
        let violations = package_content_violations("ripr", VIOLATING_LIST);
        // 9 violating paths (all but Cargo.toml and src/lib.rs), one each.
        // `src/analysis/widget_tests.rs` is the true test-only positive: a
        // `*_tests.rs` that is not on the production allowlist.
        assert_eq!(violations.len(), 9, "{violations:?}");
        for needle in [
            "src/testing/mod.rs",
            "src/domain/tests.rs",
            "src/domain/identity/tests.rs",
            "src/analysis/widget_tests.rs",
            "src/domain/refusal_remedy_corpus_tests.rs",
            "src/analysis/classify/decision/kani_proofs.rs",
            "src/testing/cwd_lock.rs",
            "tests/integration.rs",
        ] {
            assert!(
                violations.iter().any(|v| v.contains(needle)),
                "missing violation for {needle}: {violations:?}"
            );
        }
        assert!(
            violations
                .iter()
                .all(|v| v.contains("cargo package -p ripr --list"))
        );
    }

    #[test]
    fn empty_list_fails_closed() {
        let violations = package_content_violations("ripr-contract", "  \n\n");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("no files"));
    }

    #[test]
    fn contract_package_name_is_reported() {
        let violations = package_content_violations("ripr-contract", "src/domain/tests.rs\n");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("ripr-contract"));
    }
}
