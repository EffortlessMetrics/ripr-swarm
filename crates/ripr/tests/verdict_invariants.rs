//! Verdict invariants through `ripr check` (property and metamorphic checks).
//!
//! Complements `metamorphic_relations.rs` (comment/whitespace decoration,
//! local rename, unrelated test, test order, oracle strength) with relations
//! on the diff and on the test name:
//!
//! 1. a diff that only adds, edits or removes comments and blank lines in the
//!    owner never produces a finding;
//! 2. the order of file sections inside a unified diff does not change the
//!    set of findings (ids, classes);
//! 3. renaming the test function leaves the finding class unchanged;
//! 4. repeated runs on identical input produce identical findings.
//!
//! Each case first asserts a non-empty baseline so a relation cannot pass over
//! an empty or misparsed workspace. These are static classes only.

use ripr::app::check_workspace_with_config;
use ripr::config::RiprConfig;
use ripr::domain::LanguageId;
use ripr::{CheckInput, Finding};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Lang {
    name: &'static str,
    language: LanguageId,
    config: &'static str,
    comment: &'static str,
    /// (path, before, after) of two independently changed owner files.
    owners: [(&'static str, &'static str, &'static str); 2],
    manifest: Option<(&'static str, &'static str)>,
    /// (path, text) test files; `{T}` is the test name placeholder.
    tests: [(&'static str, &'static str); 2],
    test_name: &'static str,
    renamed_test: &'static str,
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn section(path: &str, before: &str, after: &str) -> String {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let mut common = vec![vec![0_usize; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            common[i][j] = if old[i] == new[j] {
                1 + common[i + 1][j + 1]
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let mut diff = format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,{} +1,{} @@\n",
        old.len(),
        new.len()
    );
    let (mut i, mut j) = (0, 0);
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            diff.push_str(&format!(" {}\n", old[i]));
            i += 1;
            j += 1;
        } else if i < old.len() && (j == new.len() || common[i + 1][j] >= common[i][j + 1]) {
            diff.push_str(&format!("-{}\n", old[i]));
            i += 1;
        } else {
            diff.push_str(&format!("+{}\n", new[j]));
            j += 1;
        }
    }
    diff
}

/// Writes `files` into a scratch workspace, runs the check with `diff`, and
/// returns findings as sorted `(id, class, expression)` rows.
fn run(lang: &Lang, files: &[(String, String)], diff: &str) -> Result<Vec<String>, String> {
    run_full(lang, files, diff).map(|(rows, _)| rows)
}

/// Like [`run`], but also returns every finding's complete `Debug` form
/// (evidence, locations, test pointers) with the scratch root masked, so the
/// determinism relation compares whole findings, not a projection.
fn run_full(
    lang: &Lang,
    files: &[(String, String)],
    diff: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-verdict-inv-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let _scratch = Scratch(root.clone());
    let mut all: Vec<(String, String)> = files.to_vec();
    all.push(("change.diff".to_string(), diff.to_string()));
    all.push(("ripr.toml".to_string(), lang.config.to_string()));
    if let Some((p, t)) = lang.manifest {
        all.push((p.to_string(), t.to_string()));
    }
    for (path, text) in all {
        let full = root.join(&path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&full, text).map_err(|e| format!("write {path}: {e}"))?;
    }
    let mut config = RiprConfig::default();
    config.languages.enabled = vec![lang.language];
    let output = check_workspace_with_config(
        CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("change.diff")),
            ..CheckInput::default()
        },
        &config,
    )?;
    let mut rows: Vec<String> = output
        .findings
        .iter()
        .map(|f: &Finding| {
            format!(
                "{}|{}|{}",
                f.id,
                f.class.as_str(),
                f.probe.expression.trim()
            )
        })
        .collect();
    rows.sort();
    let root_text = root.display().to_string();
    let mut full: Vec<String> = output
        .findings
        .iter()
        .map(|f| format!("{f:?}").replace(&root_text, "<root>"))
        .collect();
    full.sort();
    Ok((rows, full))
}

fn files(lang: &Lang, test_name: &str, owners_after: bool) -> Vec<(String, String)> {
    let texts = lang
        .owners
        .map(|(_, before, after)| (if owners_after { after } else { before }).to_string());
    files_with(lang, test_name, texts)
}

/// Workspace files with explicit owner contents, so the on-disk owner source
/// always matches the "after" side of the diff under test.
fn files_with(lang: &Lang, test_name: &str, owner_texts: [String; 2]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for ((path, _, _), text) in lang.owners.iter().zip(owner_texts) {
        out.push((path.to_string(), text));
    }
    for (path, text) in lang.tests {
        out.push((path.to_string(), text.replace("{T}", test_name)));
    }
    out
}

fn semantic_diff(lang: &Lang, reversed: bool) -> String {
    let mut sections: Vec<String> = lang
        .owners
        .iter()
        .map(|(p, b, a)| section(p, b, a))
        .collect();
    if reversed {
        sections.reverse();
    }
    sections.concat()
}

fn each(check: fn(&Lang) -> Result<(), String>) -> Result<(), String> {
    let mut failures = Vec::new();
    for lang in langs() {
        if let Err(e) = check(&lang) {
            failures.push(format!("{}: {e}", lang.name));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

fn baseline(lang: &Lang) -> Result<Vec<String>, String> {
    let rows = run(
        lang,
        &files(lang, lang.test_name, true),
        &semantic_diff(lang, false),
    )?;
    // Fixture construction: each changed owner must yield its own finding.
    for marker in ["18", "100"] {
        if !rows
            .iter()
            .any(|r| r.rsplit('|').next().is_some_and(|e| e.contains(marker)))
        {
            return Err(format!(
                "baseline has no finding for the `{marker}` owner: {rows:?}"
            ));
        }
    }
    Ok(rows)
}

fn comment_only_diff_has_no_findings(lang: &Lang) -> Result<(), String> {
    baseline(lang)?;
    let c = lang.comment;
    let mut diff = String::new();
    let mut texts = Vec::new();
    for (path, _, after) in lang.owners {
        // Same source plus comments and blank lines only; the decorated text
        // is also what lands on disk so the diff and the file agree.
        let decorated = format!("{c} added note\n\n{after}\n{c} trailing note\n");
        diff.push_str(&section(path, after, &decorated));
        texts.push(decorated);
    }
    let owner_texts: [String; 2] = texts
        .try_into()
        .map_err(|v: Vec<String>| format!("two owners expected, got {}", v.len()))?;
    let rows = run(lang, &files_with(lang, lang.test_name, owner_texts), &diff)?;
    if rows.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "comment/whitespace-only diff produced findings: {rows:?}"
        ))
    }
}

fn diff_file_order_is_irrelevant(lang: &Lang) -> Result<(), String> {
    let a = baseline(lang)?;
    let b = run(
        lang,
        &files(lang, lang.test_name, true),
        &semantic_diff(lang, true),
    )?;
    if a == b {
        Ok(())
    } else {
        Err(format!("order changed findings:\n{a:?}\n{b:?}"))
    }
}

fn test_function_rename_keeps_classes(lang: &Lang) -> Result<(), String> {
    let strip = |rows: Vec<String>| -> Vec<String> {
        rows.into_iter()
            .map(|r| r.splitn(3, '|').skip(1).collect::<Vec<_>>().join("|"))
            .collect()
    };
    let base = baseline(lang)?;
    if !base.iter().any(|r| {
        r.contains("|exposed|")
            || r.contains("|weakly_exposed|")
            || r.contains("|reachable_unrevealed|")
    }) {
        return Err(format!("baseline has no oracle-dependent class: {base:?}"));
    }
    let a = strip(base);
    let b = strip(run(
        lang,
        &files(lang, lang.renamed_test, true),
        &semantic_diff(lang, false),
    )?);
    if a == b {
        Ok(())
    } else {
        Err(format!("test rename changed verdicts:\n{a:?}\n{b:?}"))
    }
}

fn repeated_runs_are_identical(lang: &Lang) -> Result<(), String> {
    baseline(lang)?;
    let input = files(lang, lang.test_name, true);
    let diff = semantic_diff(lang, false);
    let (_, first) = run_full(lang, &input, &diff)?;
    for n in 0..3 {
        let (_, again) = run_full(lang, &input, &diff)?;
        if again != first {
            return Err(format!("run {n} differs:\n{first:?}\n{again:?}"));
        }
    }
    Ok(())
}

#[test]
fn comment_and_whitespace_only_diffs_create_no_findings() -> Result<(), String> {
    each(comment_only_diff_has_no_findings)
}

#[test]
fn diff_file_order_does_not_change_findings() -> Result<(), String> {
    each(diff_file_order_is_irrelevant)
}

#[test]
fn renaming_a_test_function_keeps_classes() -> Result<(), String> {
    each(test_function_rename_keeps_classes)
}

#[test]
fn repeated_runs_produce_identical_findings() -> Result<(), String> {
    each(repeated_runs_are_identical)
}

fn langs() -> Vec<Lang> {
    let mut v = Vec::new();
    #[cfg(feature = "lang-rust")]
    v.extend([rust_lang()]);
    #[cfg(feature = "lang-python")]
    v.extend([python_lang()]);
    #[cfg(feature = "lang-typescript")]
    v.extend([typescript_lang()]);
    v
}

#[cfg(feature = "lang-rust")]
fn rust_lang() -> Lang {
    Lang {
        name: "rust",
        language: LanguageId::Rust,
        config: "[languages]\nenabled = [\"rust\"]\n",
        comment: "//",
        owners: [
            (
                "src/lib.rs",
                "pub fn admit(age: u32) -> Result<u32, String> {\n    if age > 18 {\n        Ok(age)\n    } else {\n        Err(\"minor\".to_string())\n    }\n}\n\npub mod ports;\n",
                "pub fn admit(age: u32) -> Result<u32, String> {\n    if age >= 18 {\n        Ok(age)\n    } else {\n        Err(\"minor\".to_string())\n    }\n}\n\npub mod ports;\n",
            ),
            (
                "src/ports.rs",
                "pub fn is_full(n: u32) -> bool {\n    n > 100\n}\n",
                "pub fn is_full(n: u32) -> bool {\n    n >= 100\n}\n",
            ),
        ],
        manifest: Some((
            "Cargo.toml",
            "[package]\nname = \"seed_rs\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )),
        tests: [
            (
                "tests/admit.rs",
                "use seed_rs::admit;\n\n#[test]\nfn {T}() {\n    let got = admit(18);\n    assert_eq!(got, Ok(18));\n}\n",
            ),
            (
                "tests/ports.rs",
                "use seed_rs::ports::is_full;\n\n#[test]\nfn {T}() {\n    let got = is_full(100);\n    assert_eq!(got, true);\n}\n",
            ),
        ],
        test_name: "pins_the_boundary",
        renamed_test: "renamed_boundary_case",
    }
}

#[cfg(feature = "lang-python")]
fn python_lang() -> Lang {
    Lang {
        name: "python",
        language: LanguageId::Python,
        config: "[languages]\nenabled = [\"python\"]\n",
        comment: "#",
        owners: [
            (
                "src/gate.py",
                "def admit(age):\n    if age > 18:\n        return age\n    raise ValueError(\"minor\")\n",
                "def admit(age):\n    if age >= 18:\n        return age\n    raise ValueError(\"minor\")\n",
            ),
            (
                "src/ports.py",
                "def is_full(n):\n    return n > 100\n",
                "def is_full(n):\n    return n >= 100\n",
            ),
        ],
        manifest: None,
        tests: [
            (
                "tests/test_gate.py",
                "from src.gate import admit\n\n\ndef {T}():\n    got = admit(18)\n    assert got == 18\n",
            ),
            (
                "tests/test_ports.py",
                "from src.ports import is_full\n\n\ndef {T}():\n    got = is_full(100)\n    assert got == True\n",
            ),
        ],
        test_name: "test_pins_the_boundary",
        renamed_test: "test_renamed_boundary_case",
    }
}

#[cfg(feature = "lang-typescript")]
fn typescript_lang() -> Lang {
    Lang {
        name: "typescript",
        language: LanguageId::TypeScript,
        config: "[languages]\nenabled = [\"typescript\"]\n",
        comment: "//",
        owners: [
            (
                "src/gate.ts",
                "export function admit(age: number): number {\n  if (age > 18) {\n    return age;\n  }\n  throw new Error('minor');\n}\n",
                "export function admit(age: number): number {\n  if (age >= 18) {\n    return age;\n  }\n  throw new Error('minor');\n}\n",
            ),
            (
                "src/ports.ts",
                "export function isFull(n: number): boolean {\n  return n > 100;\n}\n",
                "export function isFull(n: number): boolean {\n  return n >= 100;\n}\n",
            ),
        ],
        manifest: None,
        tests: [
            (
                "tests/gate.test.ts",
                "import { expect, test } from 'vitest';\nimport { admit } from '../src/gate';\n\ntest('{T}', () => {\n  const got = admit(18);\n  expect(got).toBe(18);\n});\n",
            ),
            (
                "tests/ports.test.ts",
                "import { expect, test } from 'vitest';\nimport { isFull } from '../src/ports';\n\ntest('{T}', () => {\n  const got = isFull(100);\n  expect(got).toBe(true);\n});\n",
            ),
        ],
        test_name: "pins the boundary",
        renamed_test: "renamed boundary case",
    }
}
