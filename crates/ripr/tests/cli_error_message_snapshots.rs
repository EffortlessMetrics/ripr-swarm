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
    // An option typed before the command. The `ripr check` example may only
    // name a spelling `check` accepts: `ripr -q check` used to be told to run
    // `ripr check -q ...`, which `check` rejects.
    Case {
        args: &["--base", "main", "check"],
        exit: 2,
        stderr: "ripr: unknown command \"--base\"; options go after the command, e.g. `ripr check --base ...`\n",
    },
    Case {
        args: &["-q", "check"],
        exit: 2,
        stderr: "ripr: unknown command \"-q\"; options go after the command. Did you mean `ripr check --quiet`? Run `ripr --help`.\n",
    },
    Case {
        args: &["--quite", "check"],
        exit: 2,
        stderr: "ripr: unknown command \"--quite\"; options go after the command. Did you mean `ripr check --quiet`? Run `ripr --help`.\n",
    },
    // `--all` only modifies `--help`; `ripr check --all` is rejected, so a
    // near miss on it gets no `ripr check --all` example.
    Case {
        args: &["--al"],
        exit: 2,
        stderr: "ripr: unknown command \"--al\"; options go after the command, and `ripr check` has no `--al` option. Run `ripr --help`.\n",
    },
    Case {
        args: &["-x", "check"],
        exit: 2,
        stderr: "ripr: unknown command \"-x\"; options go after the command, and `ripr check` has no `-x` option. Run `ripr --help`.\n",
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
    let _ = std::fs::remove_dir_all(&dir);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}
