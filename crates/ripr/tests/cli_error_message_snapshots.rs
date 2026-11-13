//! Exact-stderr snapshots for the argument refusals that used to end at the
//! failure ("requires --before <path>") without saying where the value comes
//! from. Each case pins the whole refusal line so a regression that drops the
//! concrete next command fails here, not just a loose "has an action word"
//! check (that floor lives in `refusal_next_step.rs`).
//!
//! The cases need no repository, so they run in an empty scratch directory.

use std::process::Command;

struct Case {
    args: &'static [&'static str],
    exit: i32,
    stderr: &'static str,
}

const CASES: &[Case] = &[
    Case {
        args: &["outcome"],
        exit: 2,
        stderr: "ripr: outcome requires --before <path>; pass the snapshot saved before your edit (capture one with `ripr check --format repo-exposure-json > before.json` before editing, not after)\n",
    },
    Case {
        args: &["outcome", "--before", "before.json"],
        exit: 2,
        stderr: "ripr: outcome requires --after <path>; after your edit run `ripr check --format repo-exposure-json > after.json`, then pass `--after after.json`\n",
    },
    Case {
        args: &["review-comments"],
        exit: 2,
        stderr: "ripr: review-comments requires --base <sha>; pass the pull request's base commit, for example `--base origin/main --head HEAD`\n",
    },
    Case {
        args: &["review-comments", "--base", "origin/main"],
        exit: 2,
        stderr: "ripr: review-comments requires --head <sha>; pass the pull request's head commit, for example `--base origin/main --head HEAD`\n",
    },
    Case {
        args: &["receipt", "write"],
        exit: 2,
        stderr: "ripr: receipt write requires a canonical_gap_id; re-run with `--gap <canonical_gap_id>` (`ripr receipt --help` shows the full command)\n",
    },
    Case {
        args: &["assistant-loop", "health"],
        exit: 2,
        stderr: "ripr: assistant-loop health requires at least one --proof path; first run `ripr assistant-loop proof --pr-guidance PATH`, then pass its JSON as `--proof target/ripr/reports/test-oracle-assistant-proof.json`\n",
    },
    Case {
        args: &["first-action"],
        exit: 2,
        stderr: "ripr: first-action requires at least one explicit artifact input; pass one, for example `--pr-guidance PATH` (run `ripr help first-action` for the list)\n",
    },
    Case {
        args: &["reports", "ts-limitations"],
        exit: 2,
        stderr: "ripr: reports ts-limitations requires --check-output PATH; pass the JSON saved by `ripr check --format json > check.json`\n",
    },
    Case {
        args: &["reports", "ts-false-actionable"],
        exit: 2,
        stderr: "ripr: reports ts-false-actionable requires --corpus PATH; pass the TypeScript preview audit corpus JSON (run `ripr help reports` for the format)\n",
    },
    Case {
        args: &["explain", "--root", "/ripr-missing-root", "some-id"],
        exit: 2,
        stderr: "ripr: repository root /ripr-missing-root does not exist or is not a directory; pass `--root <dir>` naming an existing repository\n",
    },
    // #14076: missing-required-input refusals that used to stop at "requires X".
    Case {
        args: &["zero", "status"],
        exit: 2,
        stderr: "ripr: zero status requires --delta <path>; pass the baseline debt delta JSON that `ripr baseline diff` writes, for example `--delta target/ripr/reports/baseline-debt-delta.json`\n",
    },
    Case {
        args: &["calibrate", "cargo-mutants"],
        exit: 2,
        stderr: "ripr: calibrate cargo-mutants requires --mutants-json <path>; run `cargo mutants` first, then pass its `mutants.out` directory or `mutants.out/outcomes.json`\n",
    },
    Case {
        args: &[
            "calibrate",
            "cargo-mutants",
            "--mutants-json",
            "mutants.out",
        ],
        exit: 2,
        stderr: "ripr: calibrate cargo-mutants requires --repo-exposure-json <path>; capture one with `ripr check --format repo-exposure-json > repo-exposure.json`, then pass `--repo-exposure-json repo-exposure.json`\n",
    },
    Case {
        args: &["cache", "export"],
        exit: 2,
        stderr: "ripr: cache export requires --out NEW_DIRECTORY; pass a directory that does not exist yet and whose parent does, for example `ripr cache export --out ripr-cache-export`\n",
    },
    Case {
        args: &["pr-ledger", "record", "--gate", "gate.json"],
        exit: 2,
        stderr: "ripr: pr-ledger record requires --pr-number <value>; pass the pull request number or a local identifier, for example `--pr-number 123`\n",
    },
    Case {
        args: &[
            "pr-ledger",
            "record",
            "--gate",
            "gate.json",
            "--pr-number",
            "123",
        ],
        exit: 2,
        stderr: "ripr: pr-ledger record requires --base <revision>; pass the pull request's base commit, for example `--base origin/main`\n",
    },
    Case {
        args: &[
            "pr-ledger",
            "record",
            "--gate",
            "gate.json",
            "--pr-number",
            "123",
            "--base",
            "origin/main",
        ],
        exit: 2,
        stderr: "ripr: pr-ledger record requires --head <revision>; pass the pull request's head commit, for example `--head HEAD`\n",
    },
    Case {
        args: &["pr-review", "front-panel"],
        exit: 2,
        stderr: "ripr: pr-review front-panel requires at least one explicit artifact input; pass one, for example `--pr-guidance target/ripr/review/comments.json` (run `ripr help pr-review` for the list)\n",
    },
    Case {
        args: &["reports", "gap-ledger"],
        exit: 2,
        stderr: "ripr: reports gap-ledger requires --records PATH, --repo-exposure PATH, or --check-output PATH; pass one source, for example `--check-output check.json` saved by `ripr check --format json > check.json`\n",
    },
    // An empty path value (usually an unset shell variable) is refused before
    // any work. `pilot --out ""` used to write its five reports into the
    // working directory and exit 0; the scratch-directory check below pins
    // that nothing is written.
    Case {
        args: &["check", "--diff", ""],
        exit: 2,
        stderr: "ripr: --diff requires a non-empty path; pass a path right after it (an empty value usually means an unset shell variable)\n",
    },
    Case {
        args: &["check", "--write-artifact", ""],
        exit: 2,
        stderr: "ripr: --write-artifact requires a non-empty path; pass a path right after it (an empty value usually means an unset shell variable)\n",
    },
    Case {
        args: &["check", "--suppression-policy", ""],
        exit: 2,
        stderr: "ripr: --suppression-policy requires a non-empty path; pass a path right after it (an empty value usually means an unset shell variable)\n",
    },
    Case {
        args: &["evidence-health", "--root", ""],
        exit: 2,
        stderr: "ripr: --root requires a non-empty path; pass a path right after it (an empty value usually means an unset shell variable)\n",
    },
    Case {
        args: &["evidence-health", "--out", ""],
        exit: 2,
        stderr: "ripr: --out requires a non-empty path; pass a path right after it (an empty value usually means an unset shell variable)\n",
    },
    Case {
        args: &["evidence-health", "--out-md", ""],
        exit: 2,
        stderr: "ripr: --out-md requires a non-empty path; pass a path right after it (an empty value usually means an unset shell variable)\n",
    },
    Case {
        args: &["evidence-health", "--mutation-calibration", ""],
        exit: 2,
        stderr: "ripr: --mutation-calibration requires a non-empty path; pass a path right after it (an empty value usually means an unset shell variable)\n",
    },
    Case {
        args: &["pilot", "--out", ""],
        exit: 2,
        stderr: "ripr: --out requires a non-empty path; pass a path right after it (an empty value usually means an unset shell variable)\n",
    },
    Case {
        args: &["pilot", "--out", "  "],
        exit: 2,
        stderr: "ripr: --out requires a non-empty path; pass a path right after it (an empty value usually means an unset shell variable)\n",
    },
    Case {
        args: &["evidence-health", "--root", "/ripr-missing-root"],
        exit: 2,
        stderr: "ripr: evidence-health root /ripr-missing-root does not exist or is not a directory; pass `--root <dir>` naming the workspace directory\n",
    },
];

#[test]
fn argument_refusals_name_the_exact_next_command() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("ripr-error-snapshots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|error| format!("create scratch: {error}"))?;
    let mut failures = Vec::new();
    for case in CASES {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(case.args)
            .current_dir(&dir)
            .output()
            .map_err(|error| format!("run ripr {:?}: {error}", case.args))?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Progress lines are separate stderr chatter, not part of the refusal.
        let refusal: String = stderr
            .lines()
            .filter(|line| !line.starts_with("ripr progress:"))
            .map(|line| format!("{line}\n"))
            .collect();
        if output.status.code() != Some(case.exit) || refusal != case.stderr {
            failures.push(format!(
                "ripr {}: exit {:?} (wanted {})\n  got:    {refusal:?}\n  wanted: {:?}",
                case.args.join(" "),
                output.status.code(),
                case.exit,
                case.stderr
            ));
        }
    }
    // pr-evidence outside a git repository: the refusal names the directory,
    // so it is assembled here rather than listed in `CASES`. It must not show
    // git's argv or tell the reader to check a ref spelling.
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["pr-evidence", "--base", "main", "--head", "HEAD"])
        .current_dir(&dir)
        // The scratch directory can sit inside a checkout (the workspace
        // points TMPDIR at `target`), so stop git's upward discovery at its
        // parent to make this a real non-repository run.
        .env(
            "GIT_CEILING_DIRECTORIES",
            dir.parent().unwrap_or(dir.as_path()),
        )
        .output()
        .map_err(|error| format!("run ripr pr-evidence: {error}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let refusal: Vec<&str> = stderr
        .lines()
        .filter(|line| !line.starts_with("ripr progress:"))
        .collect();
    let wanted_start = "ripr: pr-evidence needs a git repository, and ";
    let wanted_end = " is not inside one; run it from inside the repository, or pass `--root <path>` naming a directory inside it";
    let shaped = matches!(refusal.as_slice(), [line] if line.starts_with(wanted_start) && line.ends_with(wanted_end));
    if output.status.code() != Some(2) || !shaped || !output.stdout.is_empty() {
        failures.push(format!(
            "ripr pr-evidence outside a repository: exit {:?} (wanted 2)\n  got: {refusal:?}",
            output.status.code()
        ));
    }
    // Every case is a refusal, so none may leave a file behind.
    let leftovers: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|error| format!("list scratch: {error}"))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    if !leftovers.is_empty() {
        failures.push(format!(
            "refusals wrote into the working directory: {leftovers:?}"
        ));
    }
    let _ = std::fs::remove_dir_all(&dir);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}
