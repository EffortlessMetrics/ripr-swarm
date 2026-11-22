//! #15954: a user's hunk, rename-limit, big-file and abbreviation settings
//! cannot change the patch `ripr check` analyzes, so one range gives one
//! `input_identity` and one set of changed lines on every machine.
use super::common::fixture_git::{fixture_git_ok, fixture_git_output};
use super::{DeclaredHunkScratch, run_command, unique_external_workspace};
use std::fs;
use std::path::{Path, PathBuf};

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

/// One commit with a stimulus for each ambient setting: a function reorder
/// that myers and histogram split differently, a test insertion the indent
/// heuristic slides, two inexact renames, and source files over 100 bytes.
/// Returns the workspace guard, its root and the base commit.
fn ambient_fixture(label: &str) -> TestResult<(DeclaredHunkScratch, PathBuf, String)> {
    let root = unique_external_workspace(label)?;
    let cleanup = DeclaredHunkScratch(root.clone());
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

    Ok((cleanup, root, base))
}

/// The five settings, each paired with the value that changed the patch.
const HOSTILE: [(&str, &str); 5] = [
    ("diff.algorithm", "histogram"),
    ("diff.indentHeuristic", "false"),
    ("diff.renameLimit", "1"),
    ("core.bigFileThreshold", "100"),
    ("core.abbrev", "12"),
];

#[test]
fn ambient_git_diff_settings_do_not_change_the_analyzed_input() -> TestResult {
    let (_cleanup, root, base) = ambient_fixture("diff-ambient-settings")?;
    let range = format!("{base}...HEAD");
    let raw_default = fixture_git_output(&root, &["diff", &range])?;
    let expected = analyzed(&root, &base)?;
    assert!(expected.1 > 0, "the fixture analyzed no changed lines");
    for (setting, value) in HOSTILE {
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

/// A per-path diff driver names its own algorithm, and git lets that win over
/// the `diff.algorithm` config pin; only the `--diff-algorithm` flag outranks
/// it. A `*.rs diff=<driver>` attribute line is ordinary, so a user whose
/// config sets that driver's algorithm would otherwise analyze a different
/// patch. The driver here is not a built-in one: git's built-in `rust` driver
/// also rewrites hunk-header function names, which is a separate input.
#[test]
fn a_diff_driver_algorithm_does_not_change_the_analyzed_input() -> TestResult {
    let (_cleanup, root, base) = ambient_fixture("diff-driver-algorithm")?;
    let range = format!("{base}...HEAD");
    let raw_default = fixture_git_output(&root, &["diff", &range])?;
    let expected = analyzed(&root, &base)?;
    assert!(expected.1 > 0, "the fixture analyzed no changed lines");
    fs::write(root.join(".git/info/attributes"), "*.rs diff=plain\n")?;
    fixture_git_ok(&root, &["config", "diff.plain.algorithm", "histogram"])?;
    // Pin the stimulus twice: the driver changes plain git's patch, and it
    // still does under the config pin that holds for `diff.algorithm`.
    let raw = fixture_git_output(&root, &["diff", &range])?;
    assert_ne!(
        raw, raw_default,
        "the driver algorithm does not discriminate"
    );
    let config_pinned = fixture_git_output(&root, &["-c", "diff.algorithm=myers", "diff", &range])?;
    assert_eq!(
        config_pinned, raw,
        "the config pin now outranks a driver algorithm; this case no longer needs the flag"
    );
    assert_eq!(
        analyzed(&root, &base)?,
        expected,
        "a diff driver algorithm changed the analyzed input"
    );
    Ok(())
}

/// The packet a pull-request workflow records (`reports ci-packet`, step
/// "Capture pull request diff") is captured through the same pinned argv as
/// analysis (#9365), so no ambient setting changes the recorded bytes.
#[test]
fn ci_packet_diff_capture_ignores_ambient_git_diff_settings() -> TestResult {
    let (_cleanup, root, base) = ambient_fixture("ci-packet-ambient-settings")?;
    fixture_git_ok(&root, &["update-ref", "refs/remotes/origin/main", &base])?;
    let capture = || -> TestResult<Vec<u8>> {
        let output = super::run_command_with_env(
            env!("CARGO_BIN_EXE_ripr"),
            &root,
            &[
                "reports",
                "ci-packet",
                "--root",
                ".",
                "--step",
                "Capture pull request diff",
            ],
            &[
                ("GITHUB_EVENT_NAME", "pull_request"),
                ("GITHUB_BASE_REF", "main"),
                ("GITHUB_EVENT_PATH", ""),
            ],
        )?;
        assert!(
            output.status.success(),
            "ci-packet capture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(fs::read(root.join("target/ripr/reports/pr.diff"))?)
    };
    let range = format!("{base}...HEAD");
    let raw_default = fixture_git_output(&root, &["diff", &range])?;
    let expected = capture()?;
    let text = String::from_utf8_lossy(&expected).into_owned();
    // Pin the default capture: both renames, text sections, full blob ids.
    assert_eq!(text.matches("rename from ").count(), 2, "{text}");
    assert!(!text.contains("Binary files"), "{text}");
    let full = fixture_git_output(&root, &["rev-parse", "HEAD"])?
        .trim()
        .len();
    for line in text.lines().filter(|line| line.starts_with("index ")) {
        let ids = line["index ".len()..]
            .split_whitespace()
            .next()
            .and_then(|ids| ids.split_once(".."));
        assert!(
            ids.is_some_and(|(old, new)| old.len() == full && new.len() == full),
            "abbreviated index line {line:?}"
        );
    }
    for (setting, value) in HOSTILE {
        fixture_git_ok(&root, &["config", setting, value])?;
        let raw = fixture_git_output(&root, &["diff", &range])?;
        assert_ne!(raw, raw_default, "{setting}={value} does not discriminate");
        assert!(
            capture()? == expected,
            "{setting}={value} changed the captured pr.diff"
        );
        fixture_git_ok(&root, &["config", "--unset", setting])?;
    }
    Ok(())
}
