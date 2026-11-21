//! #15954: a user's hunk, rename-limit, big-file and abbreviation settings
//! cannot change the patch `ripr check` analyzes, so one range gives one
//! `input_identity` and one set of changed lines on every machine.
use super::common::fixture_git::{fixture_git_ok, fixture_git_output};
use super::{DeclaredHunkScratch, run_command, unique_external_workspace};
use std::fs;
use std::path::Path;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn reorder(order: &[usize]) -> String {
    let bodies = [2, 0, 1, 1, 1];
    order
        .iter()
        .map(|&n| format!("pub fn f{n}() -> u8 {{\n    {}\n}}\n", bodies[n]))
        .collect::<Vec<_>>()
        .join("\n")
}

fn slide(names: &[(&str, u8)]) -> String {
    names
        .iter()
        .map(|(name, n)| format!("#[test]\nfn {name}() {{\n    check({n});\n}}\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn numbered(prefix: &str) -> String {
    (1..=12)
        .map(|n| format!("pub fn {prefix}_{n}() -> u16 {{ {n}00 }}\n"))
        .collect()
}

/// The identity and changed-line count `ripr check` reports for the range.
fn analyzed(root: &Path, base: &str) -> TestResult<(String, u64)> {
    let output = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(root),
        &[
            "check",
            "--base",
            base,
            "--committed",
            "--format",
            "json",
            "--quiet",
        ],
    )?;
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let outcome = &json["analysis_outcome"]["outcome"];
    let identity = outcome["identity"]["input_identity"]
        .as_str()
        .ok_or_else(|| format!("no input_identity: {json}"))?
        .to_string();
    let lines = outcome["counts"]["changed_line_count"]
        .as_u64()
        .ok_or_else(|| format!("no changed_line_count: {json}"))?;
    Ok((identity, lines))
}

#[test]
fn ambient_git_diff_settings_do_not_change_the_analyzed_input() -> TestResult {
    let root = unique_external_workspace("diff-ambient-settings")?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"ambient\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fixture_git_ok(&root, &["init", "-q"])?;
    for (key, value) in [
        ("core.autocrlf", "false"),
        ("commit.gpgsign", "false"),
        ("user.name", "Ambient"),
        ("user.email", "ambient@example.invalid"),
    ] {
        fixture_git_ok(&root, &["config", key, value])?;
    }
    let write = |path: &str, text: &str| fs::write(root.join(path), text);
    write(
        "src/lib.rs",
        "pub mod reorder;\npub mod slide;\npub mod one;\npub mod two;\n",
    )?;
    write("src/reorder.rs", &reorder(&[0, 1, 2, 3, 4]))?;
    write("src/slide.rs", &slide(&[("a", 1), ("c", 3)]))?;
    write("src/one.rs", &numbered("one"))?;
    write("src/two.rs", &numbered("two"))?;
    fixture_git_ok(&root, &["add", "."])?;
    fixture_git_ok(&root, &["commit", "-qm", "base"])?;
    let base = fixture_git_output(&root, &["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    write(
        "src/lib.rs",
        "pub mod reorder;\npub mod slide;\npub mod uno;\npub mod dos;\n",
    )?;
    write("src/reorder.rs", &reorder(&[1, 3, 4, 0, 2]))?;
    write("src/slide.rs", &slide(&[("a", 1), ("b", 2), ("c", 3)]))?;
    for (old, new) in [("one", "uno"), ("two", "dos")] {
        fixture_git_ok(
            &root,
            &["mv", &format!("src/{old}.rs"), &format!("src/{new}.rs")],
        )?;
        write(
            &format!("src/{new}.rs"),
            &numbered(old).replace("{ 700 }", "{ 701 }"),
        )?;
    }
    fixture_git_ok(&root, &["add", "."])?;
    fixture_git_ok(&root, &["commit", "-qm", "change"])?;

    let range = format!("{base}...HEAD");
    let raw_default = fixture_git_output(&root, &["diff", &range])?;
    let expected = analyzed(&root, &base)?;
    assert!(expected.1 > 0, "the fixture analyzed no changed lines");
    for (setting, value) in [
        ("diff.algorithm", "histogram"),
        ("diff.indentHeuristic", "false"),
        ("diff.renameLimit", "1"),
        ("core.bigFileThreshold", "100"),
        ("core.abbrev", "12"),
    ] {
        fixture_git_ok(&root, &["config", setting, value])?;
        // Pin the stimulus: plain git under this setting gives another patch.
        let raw = fixture_git_output(&root, &["diff", &range])?;
        assert_ne!(raw, raw_default, "{setting}={value} does not discriminate");
        assert_eq!(
            analyzed(&root, &base)?,
            expected,
            "{setting}={value} changed the analyzed input"
        );
        fixture_git_ok(&root, &["config", "--unset", setting])?;
    }
    Ok(())
}
