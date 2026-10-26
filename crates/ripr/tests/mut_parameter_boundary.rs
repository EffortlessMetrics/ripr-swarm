//! A `mut` parameter is read like any other parameter at a boundary while the
//! caller's value is what the comparison sees (#10306), and stays unresolved
//! once a write can reach the comparison first.

use ripr::{CheckInput, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The finding on the changed `n >= 10` -> `n > 9` line of `owner`, a crate
/// body whose boundary line is the one holding `n > 9`.
fn finding(owner: &str, test: &str) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-mut-parameter-{}-{sequence}",
            std::process::id()
        )),
    };
    let root = &repo.root;
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("src dir: {error}"))?;
    std::fs::create_dir_all(root.join("tests")).map_err(|error| format!("tests dir: {error}"))?;
    let write = |path: &str, text: &str| {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
    };
    write(
        "Cargo.toml",
        "[package]\nname = \"band\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write("src/lib.rs", owner)?;
    write("tests/it.rs", test)?;
    let changed_line = owner
        .lines()
        .position(|line| line.contains("n > 9"))
        .ok_or("owner has no `n > 9` line")?
        + 1;
    let lines: Vec<&str> = owner.lines().collect();
    let hunk: String = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if index + 1 == changed_line {
                format!("-{}\n+{line}\n", line.replace("n > 9", "n >= 10"))
            } else {
                format!(" {line}\n")
            }
        })
        .collect();
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{n} +1,{n} @@\n{hunk}",
        n = lines.len()
    );
    write("diff.patch", &diff)?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
    .map_err(|error| format!("check failed: {error}"))?;
    output
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == changed_line)
        .ok_or_else(|| format!("no finding on line {changed_line}"))
}

const BOTH_SIDES: &str = "use band::band;\n#[test]\nfn sides() {\n    assert_eq!(band(9), 1);\n    assert_eq!(band(10), 2);\n}\n";

fn infection(owner: &str, test: &str) -> Result<&'static str, String> {
    finding(owner, test).map(|found| found.ripr.infect.state.as_str())
}

#[test]
fn an_unwritten_mut_parameter_is_read_at_its_boundary() -> Result<(), String> {
    let owner =
        "pub fn band(mut n: u8) -> u8 {\n    let _ = &n;\n    if n > 9 { 2 } else { 1 }\n}\n";
    let state = infection(owner, BOTH_SIDES)?;
    if state != "yes" {
        return Err(format!("expected infection yes, got {state}"));
    }
    Ok(())
}

#[test]
fn a_write_in_an_earlier_arm_of_the_chain_keeps_the_boundary_readable() -> Result<(), String> {
    let owner = "pub fn band(mut n: u8) -> u8 {\n    if n >= 100 {\n        n /= 100;\n        3\n    } else if n > 9 {\n        2\n    } else {\n        1\n    }\n}\n";
    let test = "use band::band;\n#[test]\nfn sides() {\n    assert_eq!(band(9), 1);\n    assert_eq!(band(10), 2);\n    assert_eq!(band(200), 3);\n}\n";
    let state = infection(owner, test)?;
    if state != "yes" {
        return Err(format!("expected infection yes, got {state}"));
    }
    Ok(())
}

#[test]
fn a_write_before_the_comparison_keeps_it_unresolved() -> Result<(), String> {
    for owner in [
        "pub fn band(mut n: u8) -> u8 {\n    n /= 2;\n    if n > 9 { 2 } else { 1 }\n}\n",
        "pub fn band(mut n: u8) -> u8 {\n    bump(&mut n);\n    if n > 9 { 2 } else { 1 }\n}\npub fn bump(n: &mut u8) { *n += 1; }\n",
        "pub fn band(mut n: u8, big: bool) -> u8 {\n    if big { n = 200; }\n    if n > 9 { 2 } else { 1 }\n}\n",
    ] {
        let test = BOTH_SIDES
            .replace("band(9)", "band(9, false)")
            .replace("band(10)", "band(10, false)");
        let test = if owner.contains("big: bool") {
            test.as_str()
        } else {
            BOTH_SIDES
        };
        let state = infection(owner, test)?;
        if state != "unknown" {
            return Err(format!(
                "expected infection unknown, got {state} for\n{owner}"
            ));
        }
    }
    Ok(())
}

#[test]
fn a_parameter_that_is_not_mut_is_unchanged() -> Result<(), String> {
    let owner = "pub fn band(n: u8) -> u8 {\n    if n > 9 { 2 } else { 1 }\n}\n";
    let state = infection(owner, BOTH_SIDES)?;
    if state != "yes" {
        return Err(format!("expected infection yes, got {state}"));
    }
    Ok(())
}
