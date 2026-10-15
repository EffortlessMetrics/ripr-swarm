//! Archive-boundary contracts for the hosted Rust dependency cache.
//! Swatinem v2 adds workspace targets unless cache-targets is false, then
//! appends cache-directories. Cleanup still uses workspaces independently.
use std::path::Path;

const WORKFLOW: &str = include_str!("../../../.github/workflows/rust-gates.yml");

fn cache_step(workflow: &str) -> &str {
    workflow
        .split("\n      - ")
        .find(|step| step.starts_with("name: Restore hosted Rust cache\n"))
        .unwrap_or("")
}

fn input<'a>(step: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("          {name}: ");
    step.lines().find_map(|line| line.strip_prefix(&prefix))
}

fn archive_roots(step: &str) -> Vec<&str> {
    let mut roots = Vec::new();
    if input(step, "cache-targets") != Some("false") {
        roots.push("target");
    }
    if let Some((_, tail)) = step.split_once("          cache-directories: |\n") {
        roots.extend(
            tail.lines()
                .take_while(|line| line.starts_with("            "))
                .map(str::trim),
        );
    }
    roots
}

fn boundary_violations(workflow: &str) -> Vec<String> {
    let step = cache_step(workflow);
    let mut errors = Vec::new();
    for (name, expected) in [
        ("shared-key", "ripr-swarm-rust-deps-v1-${{ runner.os }}"),
        ("workspaces", "\". -> target\""),
        ("cache-targets", "false"),
        ("cache-workspace-crates", "false"),
        ("cache-bin", "false"),
        ("save-if", "${{ github.ref == 'refs/heads/main' }}"),
    ] {
        if input(step, name) != Some(expected) {
            errors.push(name.to_owned());
        }
    }
    if archive_roots(step) != ["target/debug", "target/release"] {
        errors.push("archive roots".to_owned());
    }
    errors
}

#[test]
fn hosted_cache_keeps_only_declared_cargo_profiles_and_dependency_policy() {
    assert_eq!(boundary_violations(WORKFLOW), Vec::<String>::new());
    let step = cache_step(WORKFLOW);
    assert!(step.contains("        if: ${{ !inputs.use-scratch }}\n"));
    assert!(step.contains("        uses: Swatinem/rust-cache@v2\n"));
    // Defaults retain compiler/environment/manifest hashing. A hit is never
    // used as permission to skip compilation, tests, or other required proof.
    for forbidden in [
        "add-rust-environment-hash-key:",
        "lookup-only:",
        "cache-hit",
    ] {
        assert!(!step.contains(forbidden), "unexpected {forbidden}");
    }
    assert!(!WORKFLOW.contains("outputs.cache-hit"));
}

#[test]
fn archive_selection_excludes_fixture_report_and_sibling_target_state() {
    let roots = archive_roots(cache_step(WORKFLOW));
    let selected = |path: &str| roots.iter().any(|root| Path::new(path).starts_with(root));
    for path in [
        "target/debug/deps/libdependency-hash.rlib",
        "target/debug/.fingerprint/dependency-hash/lib-dependency.json",
        "target/debug/build/dependency-hash/output",
        "target/release/deps/libdependency-hash.rlib",
    ] {
        assert!(selected(path), "dependency profile excluded: {path}");
    }
    for path in [
        "target/ripr-ts-delta5-extract-scope-foreign/tests/trybuild",
        "target/ripr/reports/policy-preflight.json",
        "target/ripr/cache/analysis.json",
        "target/nextest/ci/junit.xml",
        "target/xtask/impacted-evidence/receipt.json",
        "target/tests/trybuild/Cargo.toml",
        "target/debug-foreign/deps/libdependency.rlib",
    ] {
        assert!(!selected(path), "non-profile state selected: {path}");
    }
}

#[test]
fn broad_target_or_missing_opt_out_is_rejected() {
    for changed in [
        WORKFLOW.replace("cache-targets: false", "cache-targets: true"),
        WORKFLOW.replace("          cache-targets: false\n", ""),
        WORKFLOW.replace("            target/debug\n", "            target\n"),
        WORKFLOW.replace("            target/release\n", "            target/ripr\n"),
    ] {
        assert!(boundary_violations(&changed).contains(&"archive roots".to_owned()));
    }
}

#[test]
fn namespace_change_prevents_fallback_to_old_whole_target_archives() {
    let changed = WORKFLOW.replace(
        "ripr-swarm-rust-deps-v1-${{ runner.os }}",
        "ripr-swarm-rust-small-${{ runner.os }}",
    );
    assert!(boundary_violations(&changed).contains(&"shared-key".to_owned()));
}

#[test]
fn cleanup_mapping_and_main_only_write_authority_are_preserved() {
    for (from, to, finding) in [
        (
            "workspaces: \". -> target\"",
            "workspaces: \". -> target/debug\"",
            "workspaces",
        ),
        (
            "cache-workspace-crates: false",
            "cache-workspace-crates: true",
            "cache-workspace-crates",
        ),
        ("cache-bin: false", "cache-bin: true", "cache-bin"),
        (
            "save-if: ${{ github.ref == 'refs/heads/main' }}",
            "save-if: true",
            "save-if",
        ),
    ] {
        assert!(boundary_violations(&WORKFLOW.replace(from, to)).contains(&finding.to_owned()));
    }
}
