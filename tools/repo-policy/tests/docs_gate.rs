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
fn scoped_docs_gate_checks_unchanged_document_roles_in_the_subject_tree() -> TestResult {
    let good = "# Tree\n## Context\n## Decision\n## Consequences\n## Alternatives Considered\n";
    let rel = "docs/adr/0001-tree.md";
    let note = "docs/releases/note.md";
    let root = fixture(&[(rel, good), (note, "# Note\n")])?;
    let paths = changed(&[note]);
    let passing = run_docs_gate(&root, &paths)?;
    if !passing.violations.is_empty() || !passing.plan.gates.contains(&"check-doc-roles") {
        return Err(passing.render().into());
    }
    git(&root, &["init", "-q"])?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-q", "-m", "valid roles"])?;
    fs::write(root.join(rel), good.replace("## Decision", "## decision"))?;
    let failed = run_docs_gate(&root, &paths)?;
    if failed.violations
        != [format!(
            "{rel} is missing required ADR section `## Decision`"
        )]
    {
        return Err(failed.render().into());
    }
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-q", "-m", "lowercase role section"])?;
    fs::write(root.join(note), "# Note\nUpdated release note.\n")?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-q", "-m", "release note only"])?;
    fs::write(root.join(rel), good)?;
    let revision = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    if revision.violations != failed.violations {
        return Err(revision.render().into());
    }
    fs::write(root.join("docs/adr/README.md"), "# Index\n")?;
    fs::write(
        root.join(".gitattributes"),
        "docs/adr/README.md export-ignore\n",
    )?;
    git(&root, &["add", "docs/adr/README.md", ".gitattributes"])?;
    git(
        &root,
        &["commit", "-q", "-m", "omit the excluded directory index"],
    )?;
    fs::write(root.join(note), "# Note\nFirst index control.\n")?;
    git(&root, &["add", note])?;
    git(
        &root,
        &["commit", "-q", "-m", "release note after index exclusion"],
    )?;
    let index_filtered = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    if !index_filtered.plan.is_docs_eligible() || index_filtered.violations != failed.violations {
        return Err(index_filtered.render().into());
    }
    fs::write(root.join(".gitattributes"), "docs/adr export-ignore\n")?;
    git(&root, &["add", ".gitattributes"])?;
    git(
        &root,
        &["commit", "-q", "-m", "exclude roles from archives"],
    )?;
    fs::write(root.join(note), "# Note\nSecond release note.\n")?;
    git(&root, &["add", note])?;
    git(&root, &["commit", "-q", "-m", "second release note only"])?;
    let filtered = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    if filtered.plan.is_docs_eligible() {
        return Err(filtered.render().into());
    }
    Ok(())
}

/// #16105: `core.autocrlf=true` is the Git for Windows and hosted Windows
/// runner default. The archive the revision gate reads must keep the
/// committed bytes under it, or every role input mismatches its blob and the
/// gate widens on every Windows checkout. The repository-local setting makes
/// this control host-independent. An in-tree `eol=crlf` attribute still
/// converts the archive, and the gate must then widen rather than check bytes
/// that are not the commit's.
#[test]
fn revision_docs_gate_reads_committed_role_bytes_under_autocrlf() -> TestResult {
    let good = "# Tree\n## Context\n## Decision\n## Consequences\n## Alternatives Considered\n";
    let rel = "docs/adr/0001-tree.md";
    let note = "docs/releases/note.md";
    let root = fixture(&[
        (rel, &good.replace("## Decision", "## decision")),
        (note, "# Note\n"),
    ])?;
    git(&root, &["init", "-q"])?;
    git(&root, &["config", "core.autocrlf", "true"])?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-q", "-m", "lowercase role section"])?;
    fs::write(root.join(note), "# Note\nUpdated release note.\n")?;
    git(&root, &["add", note])?;
    git(&root, &["commit", "-q", "-m", "release note only"])?;
    let checked = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    if !checked.plan.is_docs_eligible()
        || checked.violations
            != [format!(
                "{rel} is missing required ADR section `## Decision`"
            )]
    {
        return Err(checked.render().into());
    }
    fs::write(root.join(".gitattributes"), "*.md text eol=crlf\n")?;
    git(&root, &["add", ".gitattributes"])?;
    git(&root, &["commit", "-q", "-m", "convert markdown on export"])?;
    fs::write(root.join(note), "# Note\nSecond release note.\n")?;
    git(&root, &["add", note])?;
    git(&root, &["commit", "-q", "-m", "second release note only"])?;
    let converted = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    if converted.plan.is_docs_eligible()
        || !converted
            .plan
            .widen_reasons
            .iter()
            .any(|reason| reason.contains("archived role inputs differ"))
    {
        return Err(converted.render().into());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn unrelated_non_utf8_doc_name_keeps_the_scoped_role_verdict() -> TestResult {
    use std::os::unix::ffi::OsStrExt;
    let note = "docs/releases/note.md";
    let root = fixture(&[(note, "# Note\n")])?;
    fs::write(
        root.join("docs")
            .join(std::ffi::OsStr::from_bytes(b"other-\xff.txt")),
        "ordinary bytes",
    )?;
    git(&root, &["init", "-q"])?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-q", "-m", "unrelated document"])?;
    fs::write(root.join(note), "# Note\nUpdated note.\n")?;
    git(&root, &["add", note])?;
    git(&root, &["commit", "-q", "-m", "release notes only"])?;
    let report = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    if !report.plan.is_docs_eligible() || !report.violations.is_empty() {
        return Err(report.render().into());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn scoped_roles_refuse_symlinks_even_for_an_unchanged_role_directory() -> TestResult {
    use std::os::unix::fs::symlink;
    let root = fixture(&[("docs/releases/note.md", "# Note\n")])?;
    symlink(root.join("missing"), root.join("docs/adr"))?;
    let error = run_docs_gate(&root, &changed(&["docs/releases/note.md"])).err();
    if !error
        .as_deref()
        .is_some_and(|error| error.contains("expected a regular file or directory"))
    {
        return Err(format!("unexpected symlink result: {error:?}").into());
    }
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

fn git_output(
    root: &std::path::Path,
    args: &[&str],
) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "docs-gate test")
        .env("GIT_AUTHOR_EMAIL", "docs-gate@example.invalid")
        .env("GIT_COMMITTER_NAME", "docs-gate test")
        .env("GIT_COMMITTER_EMAIL", "docs-gate@example.invalid")
        .output()?;
    assert!(output.status.success(), "git {args:?} failed");
    Ok(output)
}

fn git(root: &std::path::Path, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    git_output(root, args)?;
    Ok(())
}

fn rev_parse(root: &std::path::Path, rev: &str) -> Result<String, Box<dyn std::error::Error>> {
    let output = git_output(root, &["rev-parse", rev])?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
    // Revision mode stages the committed tree under the temp directory and owns
    // that path for the duration of the run only: it must leave nothing behind.
    let head_id = rev_parse(&root, "HEAD")?;
    let leftovers: Vec<_> = fs::read_dir(std::env::temp_dir())?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("ripr-docs-gate-tree-"))
        .filter(|name| name.contains(&head_id[..12]))
        .collect();
    assert!(
        leftovers.is_empty(),
        "revision mode left staging directories behind: {leftovers:?}"
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
#[test]
fn release_note_command_deltas_require_candidate_cli_proof() -> TestResult {
    let note = "docs/releases/0.11.0-release-notes.md";
    let old = "# Release\nUse `ripr check --diff \"old file\" --json`.\n";
    let root = fixture(&[(note, old)])?;
    git(&root, &["init", "-q"])?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-qm", "original commands"])?;
    fs::write(
        root.join(note),
        "# Release\nNew prose.\n\nUse `ripr check --diff \"old file\" --json`.\n",
    )?;
    git(&root, &["commit", "-qam", "reword surrounding prose"])?;
    let safe = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    if !safe.plan.is_docs_eligible() || !safe.violations.is_empty() {
        return Err(safe.render().into());
    }
    for (label, command) in [
        ("new flag", "ripr check --diff path --not-a-ripr-flag"),
        ("different command", "ripr gate check --diff path --json"),
        ("command removed", "regular prose"),
    ] {
        fs::write(root.join(note), format!("# Release\n`{command}`\n"))?;
        git(&root, &["commit", "-qam", label])?;
        // A repaired worktree must not mask a change in the selected Git blobs.
        fs::write(root.join(note), old)?;
        let report = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
        if !report
            .plan
            .widen_reasons
            .iter()
            .any(|r| r.contains("candidate CLI proof required"))
        {
            return Err(format!("{label}: {}", report.render()).into());
        }
    }
    for (before, after) in [
        ("ripr check", "ripr bogus"),
        ("ripr check [--json]", "ripr check [--bad-flag]"),
        ("ripr check \\\n  [--json]", "ripr check \\\n  [--bad-flag]"),
    ] {
        fs::write(root.join(note), before)?;
        git(&root, &["commit", "-qam", "original opaque example"])?;
        fs::write(root.join(note), after)?;
        git(&root, &["commit", "-qam", "changed opaque example"])?;
        let report = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
        if !report
            .plan
            .widen_reasons
            .iter()
            .any(|r| r.contains("cargo test -p xtask --test ripr_public_guide_contract"))
        {
            return Err(format!("opaque example {after:?}: {}", report.render()).into());
        }
    }
    for (name, body, requires_cli) in [
        ("prose-release-notes.md", "# Release\nJust prose.\n", false),
        (
            "cli-release-notes.md",
            "# Release\n`ripr check --new-flag`\n",
            true,
        ),
    ] {
        let path = format!("docs/releases/{name}");
        fs::write(root.join(&path), body)?;
        git(&root, &["add", "--", &path])?;
        git(&root, &["commit", "-qm", name])?;
        let report = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
        if report.plan.is_docs_eligible() == requires_cli || !report.violations.is_empty() {
            return Err(format!("added {name}: {}", report.render()).into());
        }
    }
    let unbound = run_docs_gate(&root, &changed(&[note]))?;
    if !unbound
        .plan
        .widen_reasons
        .iter()
        .any(|r| r.contains("before/after"))
    {
        return Err(unbound.render().into());
    }
    Ok(())
}

#[test]
fn revision_mode_widens_when_unchanged_markdown_can_lose_its_destination() -> TestResult {
    let note = "docs/releases/0.11.0-release-notes.md";
    let root = fixture(&[
        (
            "README.md",
            "[Upgrade](docs/releases/0.11.0-release-notes.md#upgrade-1)\n",
        ),
        (note, "# Upgrade\n## Upgrade ##\n"),
    ])?;
    git(&root, &["init", "-q"])?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-qm", "retain duplicate heading"])?;
    fs::write(root.join(note), "# Upgrade\n## Upgrade ##\n\nNew prose.\n")?;
    git(&root, &["commit", "-qam", "ordinary release note"])?;
    let safe = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    if !safe.plan.is_docs_eligible() || !safe.violations.is_empty() {
        return Err(safe.render().into());
    }
    fs::write(root.join(note), "# Upgrade\n~~~\n## Upgrade ##\n~~~\n")?;
    git(&root, &["commit", "-qam", "turn second heading into code"])?;
    let renamed = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    if !renamed
        .plan
        .widen_reasons
        .iter()
        .any(|reason| reason.contains("#upgrade-1"))
    {
        return Err(format!("lost incoming anchor was eligible: {}", renamed.render()).into());
    }
    git(&root, &["rm", note])?;
    git(&root, &["commit", "-qm", "remove linked page"])?;
    let deleted = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    if deleted.plan.is_docs_eligible() {
        return Err(format!("deleted destination was eligible: {}", deleted.render()).into());
    }
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn revision_mode_preserves_non_markdown_link_destinations() -> TestResult {
    let data = "fixtures/rust-verdict-corpus/spec-scopes/targets.json";
    let root = fixture(&[
        (
            "README.md",
            "[scope](fixtures/rust-verdict-corpus/spec-scopes/targets.json#L1)\n",
        ),
        (data, "{}\n"),
    ])?;
    git(&root, &["init", "-q"])?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-qm", "linked JSON"])?;
    fs::write(root.join(data), "{ \"paths\": [] }\n")?;
    git(&root, &["commit", "-qam", "ordinary JSON content"])?;
    let content = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    if !content.plan.is_docs_eligible() || !content.violations.is_empty() {
        return Err(content.render().into());
    }
    git(&root, &["update-index", "--chmod=+x", data])?;
    git(&root, &["commit", "-qm", "mode change"])?;
    let mode = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    if mode.plan.is_docs_eligible() {
        return Err(format!("mode transition was eligible: {}", mode.render()).into());
    }
    git(
        &root,
        &[
            "mv",
            data,
            "fixtures/rust-verdict-corpus/spec-scopes/renamed.json",
        ],
    )?;
    git(&root, &["commit", "-qm", "rename linked JSON"])?;
    let removed = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    if removed.plan.is_docs_eligible() {
        return Err(format!("renamed destination was eligible: {}", removed.render()).into());
    }
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn outgoing_links_keep_the_scoped_empty_heading_rule() -> TestResult {
    let root = fixture(&[("docs/GUIDE.md", "#\n##\n[empty duplicate](#-1)\n")])?;
    let report = run_docs_gate(&root, &changed(&["docs/GUIDE.md"]))?;
    if !report.violations.iter().any(|v| v.contains("#-1")) {
        return Err(format!(
            "empty headings supplied an outgoing anchor: {}",
            report.render()
        )
        .into());
    }
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn missing_promised_heading_blob_widens_without_hydrating_it() -> TestResult {
    let origin = fixture(&[("note.md", "# Release\nOld prose.\n")])?;
    git(&origin, &["init", "-q"])?;
    git(&origin, &["config", "uploadpack.allowFilter", "true"])?;
    git(&origin, &["add", "."])?;
    git(&origin, &["commit", "-qm", "old note"])?;
    let old = rev_parse(&origin, "HEAD:note.md")?;
    fs::write(origin.join("note.md"), "# Release\nNew prose.\n")?;
    git(&origin, &["commit", "-qam", "new note"])?;
    let clone = origin.with_extension("partial");
    let clone_path = clone.to_str().ok_or("non-UTF8 test directory")?;
    git(
        &origin,
        &[
            "clone",
            "--no-local",
            "--filter=blob:none",
            "--no-checkout",
            ".",
            clone_path,
        ],
    )?;
    git(&clone, &["checkout", "-q", "--detach", "HEAD"])?;
    let missing = || -> Result<bool, Box<dyn std::error::Error>> {
        let out = git_output(
            &clone,
            &["rev-list", "--objects", "--all", "--missing=print"],
        )?;
        Ok(String::from_utf8(out.stdout)?
            .lines()
            .any(|line| line.strip_prefix('?') == Some(old.as_str())))
    };
    if !missing()? {
        return Err("historical blob already present before gate".into());
    }
    let report = run_docs_gate_at_revision(&clone, "HEAD~1", "HEAD")?;
    if !missing()?
        || !report
            .plan
            .widen_reasons
            .iter()
            .any(|reason| reason.contains(&old))
    {
        return Err(format!(
            "promised blob must stay absent and require full proof: {}",
            report.render()
        )
        .into());
    }
    fs::remove_dir_all(clone)?;
    fs::remove_dir_all(origin)?;
    Ok(())
}

#[test]
fn revision_encoding_repair_requires_full_proof_for_the_unknown_old_anchor_set() -> TestResult {
    let root = fixture(&[("note.md", "")])?;
    fs::write(root.join("note.md"), b"# Title\n\xff")?;
    git(&root, &["init", "-q"])?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-qm", "legacy bytes"])?;
    fs::write(root.join("note.md"), "# Title\n")?;
    git(&root, &["commit", "-qam", "repair encoding"])?;
    let report = run_docs_gate_at_revision(&root, "HEAD~1", "HEAD")?;
    if !report
        .plan
        .widen_reasons
        .iter()
        .any(|reason| reason.contains("not UTF-8"))
    {
        return Err(format!(
            "unreadable old heading set was unaccounted for: {}",
            report.render()
        )
        .into());
    }
    fs::remove_dir_all(root)?;
    Ok(())
}
