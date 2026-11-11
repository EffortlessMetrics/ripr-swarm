//! An artifact report command refuses an output flag that names one of its own
//! inputs (#14312). The write would replace the evidence the user or agent
//! wants to rerun from, so the command exits `2` before writing anything and
//! names both flags plus the next step.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// Bytes the input holds before each run; a refusal must leave them intact.
const SENTINEL: &str = "[]\n";

fn scratch(label: &str) -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "ripr-output-names-input-{label}-{}-{nonce}-{sequence}",
        std::process::id()
    ));
    fs::create_dir_all(dir.join("art")).map_err(|error| format!("create scratch: {error}"))?;
    fs::write(dir.join("art/in.json"), SENTINEL)
        .map_err(|error| format!("write input: {error}"))?;
    Ok(dir)
}

fn run(dir: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|error| format!("run ripr {args:?}: {error}"))
}

struct Case {
    args: &'static [&'static str],
    command: &'static str,
    out_flag: &'static str,
    out_path: &'static str,
    in_flag: &'static str,
}

const CASES: &[Case] = &[
    Case {
        args: &[
            "reports",
            "gap-ledger",
            "--records",
            "art/in.json",
            "--out",
            "art/in.json",
        ],
        command: "reports gap-ledger",
        out_flag: "--out",
        out_path: "art/in.json",
        in_flag: "--records",
    },
    Case {
        args: &[
            "reports",
            "gap-ledger",
            "--records",
            "art/in.json",
            "--out-md",
            "./art/../art/in.json",
        ],
        command: "reports gap-ledger",
        out_flag: "--out-md",
        out_path: "./art/../art/in.json",
        in_flag: "--records",
    },
    Case {
        args: &[
            "outcome",
            "--before",
            "art/in.json",
            "--after",
            "art/after.json",
            "--format",
            "json",
            "--out",
            "art/in.json",
        ],
        command: "outcome",
        out_flag: "--out",
        out_path: "art/in.json",
        in_flag: "--before",
    },
    Case {
        args: &[
            "outcome",
            "--before",
            "art/before.json",
            "--after",
            "art/in.json",
            "--out",
            "art/in.json",
        ],
        command: "outcome",
        out_flag: "--out",
        out_path: "art/in.json",
        in_flag: "--after",
    },
    Case {
        args: &[
            "calibrate",
            "cargo-mutants",
            "--mutants-json",
            "art/in.json",
            "--repo-exposure-json",
            "art/exposure.json",
            "--out",
            "art/in.json",
        ],
        command: "calibrate cargo-mutants",
        out_flag: "--out",
        out_path: "art/in.json",
        in_flag: "--mutants-json",
    },
    Case {
        args: &[
            "baseline",
            "diff",
            "--baseline",
            "art/baseline.json",
            "--current",
            "art/in.json",
            "--out",
            "art/in.json",
        ],
        command: "baseline diff",
        out_flag: "--out",
        out_path: "art/in.json",
        in_flag: "--current",
    },
    Case {
        args: &[
            "baseline",
            "update",
            "--baseline",
            "art/baseline.json",
            "--current",
            "art/in.json",
            "--out",
            "art/in.json",
        ],
        command: "baseline update",
        out_flag: "--out",
        out_path: "art/in.json",
        in_flag: "--current",
    },
    Case {
        args: &[
            "zero",
            "status",
            "--delta",
            "art/in.json",
            "--out-md",
            "art/in.json",
        ],
        command: "zero status",
        out_flag: "--out-md",
        out_path: "art/in.json",
        in_flag: "--delta",
    },
    Case {
        args: &[
            "policy",
            "history",
            "--current",
            "art/in.json",
            "--out",
            "art/in.json",
        ],
        command: "policy history",
        out_flag: "--out",
        out_path: "art/in.json",
        in_flag: "--current",
    },
    Case {
        args: &[
            "policy",
            "readiness",
            "--gate-decision",
            "art/in.json",
            "--out",
            "art/in.json",
        ],
        command: "policy readiness",
        out_flag: "--out",
        out_path: "art/in.json",
        in_flag: "--gate-decision",
    },
];

fn expected_refusal(command: &str, out_flag: &str, out_path: &str, in_flag: &str) -> String {
    format!(
        "ripr: {command} {out_flag} {out_path} names the same file as {in_flag}; writing there would replace that input. Pass a different {out_flag} path\n"
    )
}

fn assert_refused(dir: &Path, output: &Output, label: &str, expected: &str) -> Result<(), String> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.code() != Some(2) {
        return Err(format!(
            "{label}: exit {:?}, wanted 2; stderr: {stderr}",
            output.status.code()
        ));
    }
    if stderr != expected {
        return Err(format!("{label}: stderr\n{stderr}\nwanted\n{expected}"));
    }
    if !output.stdout.is_empty() {
        return Err(format!(
            "{label}: refusal printed stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    let after = fs::read_to_string(dir.join("art/in.json"))
        .map_err(|error| format!("{label}: read input back: {error}"))?;
    if after != SENTINEL {
        return Err(format!("{label}: input was rewritten to {after:?}"));
    }
    Ok(())
}

#[test]
fn output_naming_an_input_is_refused_before_writing() -> Result<(), String> {
    let mut failures = Vec::new();
    for (index, case) in CASES.iter().enumerate() {
        let dir = scratch(&format!("case{index}"))?;
        let output = run(&dir, case.args)?;
        let label = format!("ripr {}", case.args.join(" "));
        let expected = expected_refusal(case.command, case.out_flag, case.out_path, case.in_flag);
        if let Err(failure) = assert_refused(&dir, &output, &label, &expected) {
            failures.push(failure);
        }
        let _ = fs::remove_dir_all(&dir);
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n\n"))
    }
}

#[test]
fn an_absolute_spelling_of_the_input_is_still_refused() -> Result<(), String> {
    let dir = scratch("absolute")?;
    let absolute = fs::canonicalize(dir.join("art"))
        .map_err(|error| format!("canonicalize: {error}"))?
        .join("in.json");
    let absolute_text = absolute.display().to_string();
    let output = run(
        &dir,
        &[
            "reports",
            "gap-ledger",
            "--records",
            "art/in.json",
            "--out",
            &absolute_text,
        ],
    )?;
    let shown = absolute_text.replace('\\', "/");
    let expected = expected_refusal("reports gap-ledger", "--out", &shown, "--records");
    let result = assert_refused(&dir, &output, "absolute --out", &expected);
    let _ = fs::remove_dir_all(&dir);
    result
}

#[cfg(unix)]
#[test]
fn a_symlink_to_the_input_is_still_refused() -> Result<(), String> {
    let dir = scratch("symlink")?;
    std::os::unix::fs::symlink("in.json", dir.join("art/link.json"))
        .map_err(|error| format!("symlink: {error}"))?;
    let output = run(
        &dir,
        &[
            "reports",
            "gap-ledger",
            "--records",
            "art/in.json",
            "--out",
            "art/link.json",
        ],
    )?;
    let expected = expected_refusal("reports gap-ledger", "--out", "art/link.json", "--records");
    let result = assert_refused(&dir, &output, "symlinked --out", &expected);
    let _ = fs::remove_dir_all(&dir);
    result
}

/// Discriminating negative: distinct destinations still write.
#[test]
fn distinct_output_paths_still_write() -> Result<(), String> {
    let dir = scratch("distinct")?;
    let output = run(
        &dir,
        &[
            "reports",
            "gap-ledger",
            "--records",
            "art/in.json",
            "--out",
            "art/ledger.json",
            "--out-md",
            "art/ledger.md",
        ],
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let wrote = dir.join("art/ledger.json").is_file() && dir.join("art/ledger.md").is_file();
    let input = fs::read_to_string(dir.join("art/in.json")).unwrap_or_default();
    let _ = fs::remove_dir_all(&dir);
    if output.status.code() != Some(0) || !wrote {
        return Err(format!(
            "distinct paths: exit {:?}, wrote both = {wrote}; stderr: {stderr}",
            output.status.code()
        ));
    }
    if input != SENTINEL {
        return Err(format!("distinct paths: input changed to {input:?}"));
    }
    Ok(())
}

/// `baseline update --out` naming `--baseline` is the documented in-place
/// update (the same as omitting `--out`), so it is not refused as an
/// overwrite; only `--current` is protected.
#[test]
fn baseline_update_in_place_is_not_an_overwrite_refusal() -> Result<(), String> {
    let dir = scratch("baseline-in-place")?;
    let output = run(
        &dir,
        &[
            "baseline",
            "update",
            "--baseline",
            "art/in.json",
            "--current",
            "art/current.json",
            "--out",
            "art/in.json",
            "--remove-resolved",
        ],
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let _ = fs::remove_dir_all(&dir);
    if stderr.contains("names the same file as") {
        return Err(format!(
            "baseline update in place was refused as an overwrite: {stderr}"
        ));
    }
    // The command itself still runs (and fails on the missing current
    // snapshot), proving parsing got past the guard.
    if !stderr.contains("current") {
        return Err(format!(
            "instrument check: expected the run to reach the --current read; stderr: {stderr}"
        ));
    }
    Ok(())
}
