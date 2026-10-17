//! #7795: a mixed-quote Git rename header (`a/src/lib.rs "b/src/日本語.rs"`)
//! must keep its rename disclosure, and an adjacent parseable file must not
//! mask the missing scope.
//!
//! Git quotes each `diff --git` path independently under `core.quotePath=true`
//! (the loader's pin): only the destination of an ASCII -> Unicode rename is
//! C-quoted. Every expected filename below is pinned against git's own NUL
//! (`-z`) name-status inventory, never derived from ripr's parser.

use super::common::fixture_git::fixture_git_output;
use super::{run_command, run_git, unique_external_workspace};
use serde_json::Value;
use std::path::{Path, PathBuf};

struct FixtureCleanup<'a>(&'a Path);

impl Drop for FixtureCleanup<'_> {
    fn drop(&mut self) {
        // Normal completion checks cleanup errors below; unwinding remains bounded.
        let _ = std::fs::remove_dir_all(self.0);
    }
}

const LIB_BEFORE: &str =
    "pub fn qualifies(amount: u32, threshold: u32) -> bool {\n    amount >= threshold\n}\n";
const LIB_AFTER: &str =
    "pub fn qualifies(amount: u32, threshold: u32) -> bool {\n    amount > threshold\n}\n";

const PURE_RENAME_DISCLOSURE: &str =
    "ripr: detected 1 pure rename(s) with no changed lines; rename-only changes produce no probes.";
const EDITED_RENAME_DISCLOSURE: &str = "ripr: detected 1 rename(s); changed lines are analyzed \
under the new path, but old-path test associations are not carried forward.";

fn cargo_toml(lib_path: &str) -> String {
    format!(
        "[package]\nname=\"mixed_quote_rename\"\nversion=\"0.0.0\"\nedition=\"2024\"\n\n[lib]\npath = \"{lib_path}\"\n"
    )
}

/// One literal old/new path layout. Git's independent per-path quoting makes
/// each layout exercise a different `diff --git` header quoting combination;
/// the header byte shapes are asserted through the produced diff itself.
struct RenameCase {
    label: &'static str,
    old_name: &'static str,
    new_name: &'static str,
}

/// All four old/new quoting combinations, plus the unquoted-with-spaces
/// control: `both unquoted`, `old quoted only`, `new quoted only` (the #7795
/// witness), `both quoted`.
const LAYOUTS: &[RenameCase] = &[
    RenameCase {
        label: "both-unquoted",
        old_name: "src/lib.rs",
        new_name: "src/renamed.rs",
    },
    RenameCase {
        label: "old-quoted-only",
        old_name: "src/日本語.rs",
        new_name: "src/lib.rs",
    },
    RenameCase {
        label: "new-quoted-only",
        old_name: "src/lib.rs",
        new_name: "src/日本語.rs",
    },
    RenameCase {
        label: "both-quoted",
        old_name: "src/日本語.rs",
        new_name: "src/日本語新.rs",
    },
    RenameCase {
        label: "unquoted-spaces",
        old_name: "src/old name.rs",
        new_name: "src/new name.rs",
    },
];

struct BuiltCase {
    root: PathBuf,
    /// New path exactly as git's NUL inventory recorded it — the identity
    /// pin every downstream assertion compares against.
    inventory_new: String,
    inventory_old: String,
}

/// Serializes fixture-workspace allocation: the shared harness probes a
/// fixed pid-derived directory, so concurrent allocations in this module
/// would trip over each other.
static FIXTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Build one real-git fixture: base commit with the old name, then a
/// worktree rename (plus the `[lib] path` edit that keeps the crate valid,
/// and the predicate flip for the edited variant).
fn build_case(
    case: &RenameCase,
    edited: bool,
    sequence: u64,
) -> Result<BuiltCase, Box<dyn std::error::Error>> {
    let _fixture_permit = FIXTURE_LOCK
        .lock()
        .map_err(|error| format!("fixture workspace lock poisoned: {error}"))?;
    let root = unique_external_workspace(&format!("mixed-quote-{}-{sequence}", case.label))?;
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("Cargo.toml"), cargo_toml(case.old_name))?;
    std::fs::write(root.join(case.old_name), LIB_BEFORE)?;
    run_git(&root, &["init", "-q"])?;
    run_git(&root, &["config", "core.autocrlf", "false"])?;
    // Rename detection is git-config-dependent; pin it so the fixture cannot
    // degrade to a delete+add pair on a host with `diff.renames=false`.
    run_git(&root, &["config", "diff.renames", "true"])?;
    run_git(&root, &["add", "."])?;
    run_git(
        &root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "base",
        ],
    )?;

    std::fs::rename(root.join(case.old_name), root.join(case.new_name))?;
    if edited {
        std::fs::write(root.join(case.new_name), LIB_AFTER)?;
    }
    std::fs::write(root.join("Cargo.toml"), cargo_toml(case.new_name))?;
    // Stage the rename so `git diff HEAD` (the loader's worktree origin) sees
    // the R-record instead of a tracked deletion plus an untracked file.
    run_git(&root, &["add", "-A"])?;

    // Native NUL inventory: variable-length records (`M\0path\0`,
    // `R<sim>\0old\0new\0`). Scan for the rename record; the expected names
    // come from these bytes, not from ripr's parser.
    let inventory = fixture_git_output(
        root.as_path(),
        &["diff", "--name-status", "-z", "HEAD", "--"],
    )?;
    let fields: Vec<&str> = inventory.split('\0').collect();
    let mut index = 0;
    let mut rename: Option<(&str, &str, &str)> = None;
    while index < fields.len() {
        let status = fields[index];
        if status.is_empty() {
            break;
        }
        let width = if status.starts_with('R') { 3 } else { 2 };
        if status.starts_with('R') {
            rename = Some((
                status,
                fields.get(index + 1).copied().unwrap_or_default(),
                fields.get(index + 2).copied().unwrap_or_default(),
            ));
            break;
        }
        index += width;
    }
    let Some((_status, inventory_old, inventory_new)) = rename else {
        return Err(format!("fixture produced no rename record: {inventory:?}").into());
    };
    if inventory_old.is_empty() || inventory_new.is_empty() {
        return Err(format!("inventory rename record is incomplete: {inventory:?}").into());
    }
    let inventory_old = inventory_old.to_string();
    let inventory_new = inventory_new.to_string();
    Ok(BuiltCase {
        root,
        inventory_new,
        inventory_old,
    })
}

fn check_json(
    root: &Path,
    extra_args: &[&str],
) -> Result<(std::process::Output, Value), Box<dyn std::error::Error>> {
    let mut args = vec!["check"];
    args.extend_from_slice(extra_args);
    args.push("--json");
    let output = run_command(env!("CARGO_BIN_EXE_ripr"), Some(root), &args)?;
    let report: Value = serde_json::from_slice(&output.stdout)?;
    Ok((output, report))
}

#[test]
fn all_quoting_layouts_disclose_renames_pure_and_edited() -> Result<(), Box<dyn std::error::Error>>
{
    for (sequence, case) in LAYOUTS.iter().enumerate() {
        for edited in [false, true] {
            let built = build_case(case, edited, sequence as u64)?;
            let _cleanup = FixtureCleanup(&built.root);
            let (output, report) = check_json(&built.root, &["--base", "HEAD", "--worktree"])?;
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                output.status.success(),
                "{label}/edited={edited}: exit {code:?}\nstderr: {stderr}",
                label = case.label,
                edited = edited,
                code = output.status.code()
            );
            assert_eq!(
                report["analysis_outcome"]["analysis_complete"],
                true,
                "{label}/edited={edited}: {report}",
                label = case.label,
                edited = edited
            );
            assert_eq!(
                report["analysis_outcome"]["outcome"]["counts"]["changed_file_count"],
                2,
                "{label}/edited={edited}: rename plus the Cargo.toml path edit must \
                 both stay in scope (adjacent file cannot mask the rename): {report}",
                label = case.label,
                edited = edited
            );

            let expected_file = format!("./{}", built.inventory_new.replace('\\', "/"));
            if edited {
                assert!(
                    stderr.contains(EDITED_RENAME_DISCLOSURE),
                    "{label}/edited: missing rename disclosure\nstderr: {stderr}",
                    label = case.label
                );
                let findings = report["findings"].as_array().ok_or("missing findings")?;
                let predicates = findings
                    .iter()
                    .filter(|finding| finding["probe"]["family"] == "predicate")
                    .map(|finding| {
                        (
                            finding["probe"]["file"].as_str(),
                            finding["probe"]["line"].as_u64(),
                            finding["probe"]["expression"].as_str(),
                        )
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    predicates,
                    vec![(
                        Some(expected_file.as_str()),
                        Some(2),
                        Some("amount > threshold")
                    )],
                    "{label}/edited: findings must stay under the NUL-inventory new path: {report}",
                    label = case.label
                );
            } else {
                assert!(
                    stderr.contains(PURE_RENAME_DISCLOSURE),
                    "{label}/pure: missing pure-rename disclosure\nstderr: {stderr}",
                    label = case.label
                );
                assert!(
                    report["findings"]
                        .as_array()
                        .is_some_and(|findings| findings.is_empty()),
                    "{label}/pure: a rename-only change must not invent probes: {report}",
                    label = case.label
                );
                assert_eq!(
                    report["summary"]["probes"],
                    0,
                    "{label}/pure: {report}",
                    label = case.label
                );
            }
            // No arm of this matrix may fall into the garbage/zero-scope or
            // non-source disclosures: those were the masking signatures.
            assert!(
                !stderr.contains("parsed to zero changed files"),
                "{label}/edited={edited}: zero-scope disclosure leaked\nstderr: {stderr}",
                label = case.label,
                edited = edited
            );
            assert!(
                !stderr.contains("non-source"),
                "{label}/edited={edited}: non-source disclosure leaked\nstderr: {stderr}",
                label = case.label,
                edited = edited
            );
            assert!(
                report["analysis_outcome"]["outcome"]["limitations"]
                    .as_array()
                    .is_some_and(|limitations| limitations.is_empty()),
                "{label}/edited={edited}: limitations leaked: {report}",
                label = case.label,
                edited = edited
            );
            let cleanup = std::fs::remove_dir_all(&built.root);
            drop(_cleanup);
            cleanup?;
        }
    }
    Ok(())
}

#[test]
fn isolated_mixed_quote_pure_rename_section_is_disclosed_not_malformed()
-> Result<(), Box<dyn std::error::Error>> {
    let witness = &LAYOUTS[2]; // new-quoted-only: the #7795 header shape
    let built = build_case(witness, false, 100)?;
    let _cleanup = FixtureCleanup(&built.root);

    // Isolate the rename section alone (no adjacent Cargo.toml section).
    let section = fixture_git_output(built.root.as_path(), &["diff", "HEAD", "--", "src"])?;
    assert!(
        section.contains("\"b/"),
        "fixture must produce the quoted-destination header: {section}"
    );
    let diff_file = built.root.join("isolated.diff");
    std::fs::write(&diff_file, section)?;

    let (output, report) = check_json(
        &built.root,
        &["--diff", diff_file.to_str().ok_or("non-UTF8 diff path")?],
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "isolated pure rename section refused: exit {:?}\nstderr: {stderr}\n{report}",
        output.status.code()
    );
    assert_eq!(
        report["analysis_outcome"]["analysis_complete"], true,
        "{report}"
    );
    assert!(
        stderr.contains(PURE_RENAME_DISCLOSURE),
        "isolated pure rename section must be disclosed, not diagnosed:\nstderr: {stderr}"
    );
    assert!(
        report["analysis_outcome"]["outcome"]["limitations"]
            .as_array()
            .is_some_and(|limitations| limitations.is_empty()),
        "a valid pure-rename section must not carry a malformed_diff limitation: {report}"
    );
    assert_eq!(
        report["analysis_outcome"]["outcome"]["counts"]["changed_file_count"], 1,
        "{report}"
    );
    let cleanup = std::fs::remove_dir_all(&built.root);
    drop(_cleanup);
    cleanup?;
    Ok(())
}

#[test]
fn mixed_quote_rename_disclosure_names_the_native_inventory_paths()
-> Result<(), Box<dyn std::error::Error>> {
    // The witness disclosure must name the NUL-inventory paths, so a parser
    // that silently re-decodes or re-spells the destination cannot satisfy
    // the assertion with coincidental text.
    let witness = &LAYOUTS[2];
    let built = build_case(witness, false, 200)?;
    let _cleanup = FixtureCleanup(&built.root);
    let (output, report) = check_json(&built.root, &["--base", "HEAD", "--worktree"])?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains(PURE_RENAME_DISCLOSURE), "{stderr}");
    // The changed scope must carry the decoded on-disk identity: the live
    // file exists at exactly the inventory new path.
    assert!(
        built.root.join(&built.inventory_new).is_file(),
        "inventory new path must name the on-disk file: {:?}",
        built.inventory_new
    );
    assert_eq!(
        report["analysis_outcome"]["outcome"]["counts"]["changed_file_count"], 2,
        "{report}"
    );
    let _ = built.inventory_old; // old path is the association side; presence verified at build
    let cleanup = std::fs::remove_dir_all(&built.root);
    drop(_cleanup);
    cleanup?;
    Ok(())
}
