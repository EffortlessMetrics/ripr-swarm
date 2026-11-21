//! Gate on the file lists Cargo would ship for the published crates.
//!
//! `cargo package -p ripr --list` / `-p ripr-contract --list` must not carry
//! test-only sources: the `src/testing/**` support tree, out-of-line unit-test
//! modules (`tests.rs`, `tests/**`, `*_tests.rs`), corpus-test modules
//! (`corpus_tests`) or Kani proof modules (`kani_proofs`). Those files inflate
//! the crates.io upload, can push it over the size limit, and ship test
//! scaffolding to downstream consumers. Fail-closed: a package list that
//! cannot be obtained or is empty is a violation, never a silent pass.

use crate::{FixKind, PolicyReportSpec, finish_policy_report};

const PACKAGES: &[&str] = &["ripr", "ripr-contract"];

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
        why_it_matters: "the `.crate` Cargo ships must contain only production sources. Test-only modules (`src/testing/**`, `tests.rs`, `tests/**`, `*_tests.rs`, `corpus_tests`, `kani_proofs`) inflate the upload toward the crates.io size limit and ship test scaffolding downstream.",
        fix_kind: FixKind::AuthorDecisionRequired,
        recommended_fixes: &[
            "Exclude test-only paths from the crate `include` list in its Cargo.toml (negated patterns after the production includes), or move the test-only code behind the publish stage.",
            "If a flagged `*_tests.rs` file is production code, re-include it explicitly after the exclusion as the existing production `*_tests.rs` modules do, or rename it so it does not read as a test module.",
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
        .filter_map(|path| {
            forbidden_rule(path).map(|rule| {
                format!(
                    "`cargo package -p {package} --list` includes forbidden test-only path `{path}` ({rule}); test-only sources must not ship in the published crate"
                )
            })
        })
        .collect()
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
    if components[..components.len().saturating_sub(1)].contains(&"tests") {
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

    const VIOLATING_LIST: &str = "\
Cargo.toml
src/lib.rs
src/testing/mod.rs
src/testing/fixture_git.rs
src/domain/tests.rs
src/domain/identity/tests.rs
src/analysis/related_tests.rs
src/domain/refusal_remedy_corpus_tests.rs
src/analysis/classify/decision/kani_proofs.rs
src\\testing\\cwd_lock.rs
examples/sample/tests/pricing.rs
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
    fn violating_list_flags_every_category() {
        let violations = package_content_violations("ripr", VIOLATING_LIST);
        // 10 violating paths (all but Cargo.toml and src/lib.rs), one each.
        assert_eq!(violations.len(), 10, "{violations:?}");
        for needle in [
            "src/testing/mod.rs",
            "src/domain/tests.rs",
            "src/domain/identity/tests.rs",
            "src/analysis/related_tests.rs",
            "src/domain/refusal_remedy_corpus_tests.rs",
            "src/analysis/classify/decision/kani_proofs.rs",
            "src/testing/cwd_lock.rs",
            "examples/sample/tests/pricing.rs",
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
