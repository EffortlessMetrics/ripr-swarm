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
    // Omitting `--out` updates `--baseline` in place, so a `--baseline`
    // that is also `--current` would be replaced too (review of #14341).
    Case {
        args: &[
            "baseline",
            "update",
            "--baseline",
            "art/in.json",
            "--current",
            "art/in.json",
        ],
        command: "baseline update",
        out_flag: "--baseline",
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
    Case {
        args: &[
            "first-action",
            "--pr-guidance",
            "art/in.json",
            "--out-md",
            "art/in.json",
        ],
        command: "first-action",
        out_flag: "--out-md",
        out_path: "art/in.json",
        in_flag: "--pr-guidance",
    },
    Case {
        args: &[
            "assistant-loop",
            "health",
            "--proof",
            "art/other.json",
            "--proof",
            "art/in.json",
            "--out",
            "art/in.json",
        ],
        command: "assistant-loop health",
        out_flag: "--out",
        out_path: "art/in.json",
        in_flag: "--proof",
    },
    Case {
        args: &[
            "pr-ledger",
            "record",
            "--pr-number",
            "1",
            "--base",
            "a",
            "--head",
            "b",
            "--gap-ledger",
            "art/in.json",
            "--out",
            "art/in.json",
        ],
        command: "pr-ledger record",
        out_flag: "--out",
        out_path: "art/in.json",
        in_flag: "--gap-ledger",
    },
    Case {
        args: &[
            "coverage-grip",
            "frontier",
            "--ledger",
            "art/in.json",
            "--out",
            "art/in.json",
        ],
        command: "coverage-grip frontier",
        out_flag: "--out",
        out_path: "art/in.json",
        in_flag: "--ledger",
    },
    Case {
        args: &[
            "baseline",
            "create",
            "--from",
            "art/in.json",
            "--out",
            "art/in.json",
        ],
        command: "baseline create",
        out_flag: "--out",
        out_path: "art/in.json",
        in_flag: "--from",
    },
    // The rolling update exempts only `--out`; the Markdown companion must
    // not replace the previous JSON report.
    Case {
        args: &[
            "policy",
            "readiness",
            "--previous-readiness",
            "art/in.json",
            "--out",
            "art/next.json",
            "--out-md",
            "art/in.json",
        ],
        command: "policy readiness",
        out_flag: "--out-md",
        out_path: "art/in.json",
        in_flag: "--previous-readiness",
    },
    // The manifest is read relative to `--root`, so `--root art --manifest
    // in.json` reads art/in.json, the same file as `--out art/in.json`.
    Case {
        args: &[
            "policy",
            "suppression-health",
            "--root",
            "art",
            "--manifest",
            "in.json",
            "--out",
            "art/in.json",
        ],
        command: "policy suppression-health",
        out_flag: "--out",
        out_path: "art/in.json",
        in_flag: "--manifest",
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
    // Not canonicalized: on Windows that would add a `\\?\` prefix the
    // human renderer folds away. Both files exist, so canonical identity
    // still decides the refusal.
    let absolute = dir.join("art").join("in.json");
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
    if !stderr.starts_with(
        "ripr: read baseline update current gate-decision art/current.json failed: not found.",
    ) {
        return Err(format!(
            "instrument check: expected the run to reach the --current read; stderr: {stderr}"
        ));
    }
    Ok(())
}

/// Flows that never overwrite an input they still need are not refused:
/// `baseline create --dry-run` never writes `--out`, and `policy readiness
/// --previous-readiness` naming `--out` is the rolling trend update (the
/// previous report is read whole before the new one replaces it).
#[test]
fn non_destructive_same_path_flows_are_not_refused() -> Result<(), String> {
    const GATE_DECISION: &str = r#"{"schema_version":"0.1","status":"advisory","decisions":[]}"#;
    let dir = scratch("non-destructive")?;
    fs::write(dir.join("art/gate.json"), GATE_DECISION)
        .map_err(|error| format!("write gate decision: {error}"))?;
    let result = (|| {
        let dry_run = run(
            &dir,
            &[
                "baseline",
                "create",
                "--dry-run",
                "--from",
                "art/gate.json",
                "--out",
                "art/gate.json",
            ],
        )?;
        let stdout = String::from_utf8_lossy(&dry_run.stdout).to_string();
        if !dry_run.status.success() {
            return Err(format!(
                "baseline create --dry-run: exit {:?}; stderr: {}",
                dry_run.status.code(),
                String::from_utf8_lossy(&dry_run.stderr)
            ));
        }
        serde_json::from_str::<serde_json::Value>(&stdout)
            .map_err(|error| format!("dry-run stdout is not JSON ({error}): {stdout}"))?;
        let gate = fs::read_to_string(dir.join("art/gate.json"))
            .map_err(|error| format!("read gate decision: {error}"))?;
        if gate != GATE_DECISION {
            return Err(format!("dry run changed --from: {gate:?}"));
        }
        // Seed a real readiness report, then roll it forward in place.
        for args in [
            &["policy", "readiness", "--out", "art/readiness.json"][..],
            &[
                "policy",
                "readiness",
                "--previous-readiness",
                "art/readiness.json",
                "--out",
                "art/readiness.json",
            ][..],
        ] {
            let output = run(&dir, args)?;
            if !output.status.success() {
                return Err(format!(
                    "ripr {}: exit {:?}; stderr: {}",
                    args.join(" "),
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
        }
        let report = fs::read_to_string(dir.join("art/readiness.json"))
            .map_err(|error| format!("read readiness: {error}"))?;
        serde_json::from_str::<serde_json::Value>(&report)
            .map_err(|error| format!("rolled readiness report is not JSON ({error}): {report}"))?;
        Ok(())
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

/// Filesystem identity, not lexical `..` collapse, decides (review of #14341):
/// `alias -> physical/nested` makes `alias/../in.json` name
/// `physical/in.json`, a different file from `in.json`, so the write goes
/// ahead. A symlink that really points at the input is still refused (the
/// control at the end).
#[cfg(unix)]
#[test]
fn a_symlinked_parent_followed_by_dotdot_is_not_a_false_match() -> Result<(), String> {
    let dir = scratch("symlink-dotdot")?;
    fs::create_dir_all(dir.join("physical/nested"))
        .map_err(|error| format!("create physical: {error}"))?;
    fs::write(dir.join("physical/in.json"), "other\n")
        .map_err(|error| format!("write physical input: {error}"))?;
    fs::write(dir.join("in.json"), SENTINEL).map_err(|error| format!("write input: {error}"))?;
    std::os::unix::fs::symlink("physical/nested", dir.join("alias"))
        .map_err(|error| format!("symlink: {error}"))?;
    let distinct = run(
        &dir,
        &[
            "reports",
            "gap-ledger",
            "--records",
            "in.json",
            "--out",
            "alias/../in.json",
            "--out-md",
            "ledger.md",
        ],
    )?;
    let distinct_stderr = String::from_utf8_lossy(&distinct.stderr).to_string();
    let input_after = fs::read_to_string(dir.join("in.json")).unwrap_or_default();
    let physical_after = fs::read_to_string(dir.join("physical/in.json")).unwrap_or_default();
    // Control: a symlink that really names the input is still refused.
    std::os::unix::fs::symlink("../in.json", dir.join("physical/link.json"))
        .map_err(|error| format!("symlink control: {error}"))?;
    let alias = run(
        &dir,
        &[
            "reports",
            "gap-ledger",
            "--records",
            "in.json",
            "--out",
            "alias/../link.json",
        ],
    )?;
    let alias_stderr = String::from_utf8_lossy(&alias.stderr).to_string();
    let _ = fs::remove_dir_all(&dir);
    if distinct.status.code() != Some(0) || distinct_stderr.contains("names the same file as") {
        return Err(format!(
            "distinct file behind a symlinked parent was refused: exit {:?}; {distinct_stderr}",
            distinct.status.code()
        ));
    }
    if input_after != SENTINEL || physical_after == "other\n" {
        return Err(format!(
            "write went to the wrong file: in.json={input_after:?} physical/in.json={physical_after:?}"
        ));
    }
    let expected = expected_refusal(
        "reports gap-ledger",
        "--out",
        "alias/../link.json",
        "--records",
    );
    if alias.status.code() != Some(2) || alias_stderr != expected {
        return Err(format!(
            "true alias was not refused: exit {:?}; {alias_stderr}",
            alias.status.code()
        ));
    }
    Ok(())
}

/// A cargo-mutants directory is read through its `outcomes.json`, so an
/// `--out` naming that member is refused; a missing input has nothing to
/// replace, so a report that tolerates it still runs (review of #14341).
#[test]
fn directory_members_are_inputs_and_missing_inputs_are_not() -> Result<(), String> {
    let dir = scratch("dir-members")?;
    fs::create_dir_all(dir.join("art/mutants.out"))
        .map_err(|error| format!("create mutants dir: {error}"))?;
    fs::write(dir.join("art/mutants.out/outcomes.json"), SENTINEL)
        .map_err(|error| format!("write outcomes: {error}"))?;
    let member = run(
        &dir,
        &[
            "calibrate",
            "cargo-mutants",
            "--mutants-json",
            "art/mutants.out",
            "--repo-exposure-json",
            "art/in.json",
            "--out",
            "art/mutants.out/outcomes.json",
        ],
    )?;
    let member_stderr = String::from_utf8_lossy(&member.stderr).to_string();
    let outcomes_after =
        fs::read_to_string(dir.join("art/mutants.out/outcomes.json")).unwrap_or_default();
    let missing = run(
        &dir,
        &[
            "baseline",
            "diff",
            "--baseline",
            "ghost/../art/report.json",
            "--current",
            "art/in.json",
            "--out",
            "art/report.json",
            "--out-md",
            "art/report.md",
        ],
    )?;
    let missing_stderr = String::from_utf8_lossy(&missing.stderr).to_string();
    let _ = fs::remove_dir_all(&dir);
    let expected = expected_refusal(
        "calibrate cargo-mutants",
        "--out",
        "art/mutants.out/outcomes.json",
        "--mutants-json",
    );
    if member.status.code() != Some(2) || member_stderr != expected || outcomes_after != SENTINEL {
        return Err(format!(
            "directory member was not protected: exit {:?}; {member_stderr}",
            member.status.code()
        ));
    }
    if missing_stderr.contains("names the same file as") {
        return Err(format!(
            "a missing input was treated as the output: {missing_stderr}"
        ));
    }
    Ok(())
}
