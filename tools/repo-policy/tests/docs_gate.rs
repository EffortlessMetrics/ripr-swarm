//! Product-free docs-gate coverage (issue #14644 Leaf A), in the style of
//! `docs_only_routing.rs`: ordinary Markdown passes the new path, a changed
//! spec runs the spec gates, a hand-edited generated registry block is
//! refused (retained product obligation), and manifest/lockfile input
//! widens instead of silently passing.
use repo_policy::docs_gate::{plan, run_docs_gate, run_docs_gate_at_revision};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

type TestResult = Result<(), Box<dyn std::error::Error>>;
static NEXT: AtomicU64 = AtomicU64::new(0);

fn fixture(files: &[(&str, &str)]) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let id = NEXT.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ripr-docs-gate-{}-{id}", std::process::id()));
    for (rel, text) in files {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, text)?;
    }
    Ok(root)
}

fn changed(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|p| (*p).to_string()).collect()
}

const GOOD_SPEC: &str = "# RIPR-SPEC-0099: Example\n\nStatus: proposed\n\n## Problem\n\nx\n\n## Behavior\n\nx\n\n## Required Evidence\n\nx\n\n## Non-Goals\n\nx\n\n## Acceptance Examples\n\nx\n\n## Test Mapping\n\nx\n\n## Implementation Mapping\n\nx\n\n## Metrics\n\nx\n";

#[test]
fn ordinary_markdown_change_passes_new_path() -> TestResult {
    let root = fixture(&[
        (
            "docs/GUIDE.md",
            "# Guide\n\nSee [spec](specs/RIPR-SPEC-0099-example.md).\n",
        ),
        ("docs/specs/RIPR-SPEC-0099-example.md", GOOD_SPEC),
    ])?;
    let report = run_docs_gate(&root, &changed(&["docs/GUIDE.md"]))?;
    assert!(report.plan.is_docs_eligible(), "{}", report.render());
    assert!(report.violations.is_empty(), "{}", report.render());
    assert!(report.plan.gates.contains(&"markdown-links"));
    assert!(!report.plan.gates.contains(&"check-spec-format"));
    Ok(())
}

#[test]
fn changed_spec_runs_spec_gates() -> TestResult {
    let root = fixture(&[
        ("docs/specs/RIPR-SPEC-0099-example.md", GOOD_SPEC),
        ("docs/spec-index.d/RIPR-SPEC-0099.md", "# index\n"),
    ])?;
    let report = run_docs_gate(&root, &changed(&["docs/specs/RIPR-SPEC-0099-example.md"]))?;
    assert!(report.violations.is_empty(), "{}", report.render());
    assert!(report.plan.gates.contains(&"check-spec-format"));
    assert!(report.plan.gates.contains(&"check-spec-numbering"));

    let bad = fixture(&[("docs/specs/RIPR-SPEC-0099-example.md", "# Wrong\n")])?;
    let report = run_docs_gate(&bad, &changed(&["docs/specs/RIPR-SPEC-0099-example.md"]))?;
    assert!(!report.violations.is_empty(), "bad spec must fail");
    Ok(())
}

#[test]
fn hand_edited_generated_registry_block_is_refused_not_passed() -> TestResult {
    let root = fixture(&[(
        "docs/status/SUPPORT_TIERS.md",
        "# Tiers\n\n<!-- capability-registry:begin (generated) -->\nhand edited\n<!-- capability-registry:end -->\n",
    )])?;
    let report = run_docs_gate(&root, &changed(&["docs/status/SUPPORT_TIERS.md"]))?;
    assert!(
        !report.plan.is_docs_eligible(),
        "registry change must widen: {}",
        report.render()
    );
    assert!(report.render().contains("capability-docs --check"));
    assert!(!plan(&changed(&["docs/status/SUPPORT_TIERS.md"])).is_docs_eligible());
    Ok(())
}

#[test]
fn manifest_and_lockfile_input_widens_rather_than_silently_passing() {
    for path in [
        "Cargo.lock",
        "Cargo.toml",
        "crates/ripr/Cargo.toml",
        "docs/OUTPUT_SCHEMA.md",
        "src/lib.rs",
    ] {
        let p = plan(&changed(&[path, "docs/GUIDE.md"]));
        assert!(!p.is_docs_eligible(), "{path} must widen: {p:?}");
    }
    assert!(!plan(&[]).is_docs_eligible(), "empty diff must widen");
}

#[test]
fn deleted_files_and_directory_readmes_are_skipped() -> TestResult {
    let root = fixture(&[
        ("docs/specs/README.md", "# Specs\n\nIndex prose.\n"),
        ("changelog.d/README.md", "# Fragments\n\nFormat doc.\n"),
    ])?;
    let report = run_docs_gate(
        &root,
        &changed(&[
            "docs/deleted-guide.md",
            "docs/specs/README.md",
            "changelog.d/README.md",
        ]),
    )?;
    assert!(report.violations.is_empty(), "{}", report.render());
    Ok(())
}

#[test]
fn allowlisted_docs_and_reasonless_marker_handling() -> TestResult {
    let root = fixture(&[
        (
            ".ripr/static-language-allowlist.toml",
            "schema_version = 1\n\n[[allow]]\npath = \"AGENTS.md\"\nowner = \"maintainers\"\nreason = \"boundary doc\"\n\n[[allow]]\nglob = \"docs/*.md\"\nowner = \"maintainers\"\nreason = \"docs\"\n\n[[allow]]\nglob = \"docs/**/*.md\"\nowner = \"maintainers\"\nreason = \"docs\"\n",
        ),
        ("AGENTS.md", "# Agents\n\nDo not say killed.\n"),
        ("docs/other/GUIDE.md", "# Guide\n\nThis was killed.\n"),
        (
            "notes/MARKED.md",
            "# Marked\n\nThis was killed. ripr-allow: static-language:\nThis was survived. <!-- ripr-allow: static-language: calibration terms are data here -->\n",
        ),
    ])?;
    let report = run_docs_gate(&root, &changed(&["AGENTS.md"]))?;
    assert!(report.violations.is_empty(), "{}", report.render());
    let report = run_docs_gate(&root, &changed(&["docs/other/GUIDE.md"]))?;
    assert!(
        report.violations.is_empty(),
        "docs glob must allow: {}",
        report.render()
    );
    let report = run_docs_gate(&root, &changed(&["notes/MARKED.md"]))?;
    assert_eq!(report.violations.len(), 1, "{}", report.render());
    Ok(())
}

#[test]
fn retained_gate_inputs_widen_instead_of_passing_scoped() {
    for (path, gate) in [
        ("docs/spec-index.d/RIPR-SPEC-0099.md", "check-doc-index"),
        ("policy/doc-artifacts.d/thing.md", "check-doc-artifacts"),
        (".ripr/traceability.d/thing.toml", "check-traceability"),
    ] {
        let p = plan(&changed(&[path]));
        assert!(!p.is_docs_eligible(), "{path} must widen: {p:?}");
        assert!(
            p.widen_reasons.iter().any(|r| r.contains(gate)),
            "{path} must name {gate}: {p:?}"
        );
        assert!(p.retained_triggers.iter().any(|t| t == path), "{p:?}");
    }
}

#[test]
fn undeclared_docs_glob_does_not_exempt_static_language() -> TestResult {
    // The allowlist declares only an exact path; the docs globs the
    // canonical check honors must be declared, not assumed.
    let root = fixture(&[
        (
            ".ripr/static-language-allowlist.toml",
            "schema_version = 1\n\n[[allow]]\npath = \"AGENTS.md\"\nowner = \"maintainers\"\nreason = \"boundary doc\"\n",
        ),
        ("docs/other/GUIDE.md", "# Guide\n\nThis was killed.\n"),
    ])?;
    let report = run_docs_gate(&root, &changed(&["docs/other/GUIDE.md"]))?;
    assert!(
        report.violations.iter().any(|v| v.contains("killed")),
        "undeclared glob must not exempt: {}",
        report.render()
    );
    Ok(())
}

#[test]
fn bookkeeping_fragment_entries_require_internal_ci_section() -> TestResult {
    let labeled = fixture(&[(
        "changelog.d/14644-labeled-corpus.md",
        "<!-- section: Added -->\n- Labeled Rust verdict corpus covers the new case (#14644).\n",
    )])?;
    let report = run_docs_gate(&labeled, &changed(&["changelog.d/14644-labeled-corpus.md"]))?;
    assert!(
        report.violations.iter().any(|v| v.contains("bookkeeping")),
        "{}",
        report.render()
    );
    let tooling = fixture(&[(
        "changelog.d/14644-xtask-note.md",
        "<!-- section: Fixed -->\n- `cargo xtask check-fast` scopes Clippy to impacted packages (#14644).\n",
    )])?;
    let report = run_docs_gate(&tooling, &changed(&["changelog.d/14644-xtask-note.md"]))?;
    assert!(
        report.violations.iter().any(|v| v.contains("bookkeeping")),
        "{}",
        report.render()
    );
    let internal = fixture(&[(
        "changelog.d/14644-labeled-corpus.md",
        "<!-- section: Internal / CI -->\n- Labeled Rust verdict corpus covers the new case (#14644).\n",
    )])?;
    let report = run_docs_gate(
        &internal,
        &changed(&["changelog.d/14644-labeled-corpus.md"]),
    )?;
    assert!(report.violations.is_empty(), "{}", report.render());
    Ok(())
}

#[test]
fn disposition_non_array_element_is_a_violation() -> TestResult {
    let root = fixture(&[(
        "policy/ripr-agent-disposition.toml",
        "schema_version = \"1\"\nelement = \"not-an-array\"\n",
    )])?;
    let report = run_docs_gate(&root, &changed(&["policy/ripr-agent-disposition.toml"]))?;
    assert!(
        report
            .violations
            .iter()
            .any(|v| v.contains("element must be an array")),
        "{}",
        report.render()
    );
    Ok(())
}

#[test]
fn disposition_path_is_accepted_and_validated() -> TestResult {
    let root = fixture(&[(
        "policy/ripr-agent-disposition.toml",
        "schema_version = \"1\"\n\n[[element]]\nid = \"x\"\ndisposition = \"retire\"\n",
    )])?;
    let report = run_docs_gate(&root, &changed(&["policy/ripr-agent-disposition.toml"]))?;
    assert!(report.plan.is_docs_eligible(), "{}", report.render());
    assert!(report.plan.gates.contains(&"check-ripr-agent-disposition"));
    assert!(report.violations.is_empty(), "{}", report.render());
    Ok(())
}

#[test]
fn duplicate_spec_ids_and_fragment_rules_fail() -> TestResult {
    let root = fixture(&[
        ("docs/specs/RIPR-SPEC-0099-example.md", GOOD_SPEC),
        (
            "docs/specs/RIPR-SPEC-0099-copy.md",
            "# RIPR-SPEC-0099: Copy\n\nStatus: proposed\n\n## Problem\n\nx\n\n## Behavior\n\nx\n\n## Required Evidence\n\nx\n\n## Non-Goals\n\nx\n\n## Acceptance Examples\n\nx\n\n## Test Mapping\n\nx\n\n## Implementation Mapping\n\nx\n\n## Metrics\n\nx\n",
        ),
        ("docs/spec-index.d/RIPR-SPEC-0099.md", "# index\n"),
        (
            "changelog.d/Bad_Name.md",
            "<!-- section: Improved -->\n- No reference here.\n",
        ),
    ])?;
    let report = run_docs_gate(
        &root,
        &changed(&[
            "docs/specs/RIPR-SPEC-0099-copy.md",
            "changelog.d/Bad_Name.md",
        ]),
    )?;
    assert!(
        report
            .violations
            .iter()
            .any(|v| v.contains("duplicates spec id")),
        "{}",
        report.render()
    );
    assert!(
        report
            .violations
            .iter()
            .any(|v| v.contains("unknown section")),
        "{}",
        report.render()
    );
    assert!(
        report.violations.iter().any(|v| v.contains("kebab")),
        "{}",
        report.render()
    );
    assert!(
        report
            .violations
            .iter()
            .any(|v| v.contains("names no issue or PR")),
        "{}",
        report.render()
    );
    Ok(())
}

#[test]
fn broken_heading_links_and_fenced_examples() -> TestResult {
    let root = fixture(&[
        (
            "docs/GUIDE.md",
            "# Guide\n\n[details](#does-not-exist)\n\n```md\n[illustrative](missing-example.md)\n```\n\n[spaced](<my guide.md>)\n",
        ),
        ("docs/my guide.md", "# Mine\n"),
    ])?;
    let report = run_docs_gate(&root, &changed(&["docs/GUIDE.md"]))?;
    assert_eq!(report.violations.len(), 1, "{}", report.render());
    assert!(
        report.violations[0].contains("#does-not-exist"),
        "{}",
        report.render()
    );
    Ok(())
}

fn git(root: &std::path::Path, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "docs-gate test")
        .env("GIT_AUTHOR_EMAIL", "docs-gate@example.invalid")
        .env("GIT_COMMITTER_NAME", "docs-gate test")
        .env("GIT_COMMITTER_EMAIL", "docs-gate@example.invalid")
        .status()?;
    assert!(status.success(), "git {args:?} failed");
    Ok(())
}

#[test]
fn revision_mode_reads_committed_tree_not_working_files() -> TestResult {
    let root = fixture(&[
        ("docs/GUIDE.md", "# Guide\n\nClean text.\n"),
        ("docs/OTHER.md", "# Other\n"),
    ])?;
    git(&root, &["init", "-q"])?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-q", "-m", "one"])?;
    fs::write(
        root.join("docs/GUIDE.md"),
        "# Guide\n\nClean text, edited.\n",
    )?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-q", "-m", "two"])?;
    // Dirty the working file with a forbidden term after the head commit.
    fs::write(root.join("docs/GUIDE.md"), "# Guide\n\nThis was killed.\n")?;
    let worktree = run_docs_gate(&root, &changed(&["docs/GUIDE.md"]))?;
    assert!(
        worktree.violations.iter().any(|v| v.contains("killed")),
        "worktree mode must see the dirty bytes: {}",
        worktree.render()
    );
    let revision = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    assert!(revision.plan.is_docs_eligible(), "{}", revision.render());
    assert!(
        revision.violations.is_empty(),
        "revision mode must read committed bytes, not the dirty file: {}",
        revision.render()
    );
    Ok(())
}

#[test]
fn root_relative_link_resolves_from_repo_root() -> TestResult {
    let root = fixture(&[
        (
            "docs/GUIDE.md",
            "# Guide\n\nSee [target](/docs/TARGET.md) and [gone](/docs/GONE.md).\n",
        ),
        ("docs/TARGET.md", "# Target\n"),
    ])?;
    let report = run_docs_gate(&root, &changed(&["docs/GUIDE.md"]))?;
    assert_eq!(report.violations.len(), 1, "{}", report.render());
    assert!(
        report.violations[0].contains("/docs/GONE.md"),
        "{}",
        report.render()
    );
    Ok(())
}

#[test]
fn bom_markdown_reports_one_encoding_violation() -> TestResult {
    let root = fixture(&[("docs/GUIDE.md", "\u{feff}# Guide\n\nPlain text.\n")])?;
    let report = run_docs_gate(&root, &changed(&["docs/GUIDE.md"]))?;
    assert_eq!(report.violations.len(), 1, "{}", report.render());
    assert!(
        report.violations[0].contains("UTF-8 BOM"),
        "{}",
        report.render()
    );
    Ok(())
}

#[test]
fn forbidden_static_language_and_broken_link_fail() -> TestResult {
    let root = fixture(&[(
        "docs/GUIDE.md",
        "# Guide\n\nThis mutant was killed. See [missing](nope.md).\n",
    )])?;
    let report = run_docs_gate(&root, &changed(&["docs/GUIDE.md"]))?;
    assert!(
        report.violations.iter().any(|v| v.contains("killed")),
        "{}",
        report.render()
    );
    assert!(
        report.violations.iter().any(|v| v.contains("nope.md")),
        "{}",
        report.render()
    );
    Ok(())
}
