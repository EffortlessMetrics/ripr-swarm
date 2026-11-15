//! Product-free docs-gate coverage (issue #14644 Leaf A), in the style of
//! `docs_only_routing.rs`: ordinary Markdown passes the new path, a changed
//! spec runs the spec gates, a hand-edited generated registry block is
//! refused (retained product obligation), and manifest/lockfile input
//! widens instead of silently passing.
use repo_policy::docs_gate::{plan, run_docs_gate};
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
