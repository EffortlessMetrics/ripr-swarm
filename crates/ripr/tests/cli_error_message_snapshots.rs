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
    // #15985: the two agent refusals that named no source for the id.
    Case {
        args: &["agent", "start"],
        exit: 2,
        stderr: "ripr: agent start requires --seam-id; add `--seam-id <id>`. Use an id that `ripr pilot --root <dir>` lists, or the spot id on the `ripr repair` line that `ripr check` prints; a `probe:...` finding id is not accepted here\n",
    },
    Case {
        args: &["agent", "brief"],
        exit: 2,
        stderr: "ripr: agent brief requires exactly one of --diff, --base, --files, or --seam-id; pass `--base <rev> --json` to brief the changes since that revision, or `--seam-id <id> --json` for one seam. Use an id that `ripr pilot --root <dir>` lists, or the spot id on the `ripr repair` line that `ripr check` prints; a `probe:...` finding id is not accepted here\n",
    },
    // #15985: an empty command name is named as empty, not echoed as `""`.
    Case {
        args: &[""],
        exit: 2,
        stderr: "ripr: the command name is empty, which usually means an unset shell variable. Run `ripr --help` to list commands.\n",
    },
    // #15985: a word after the version flag is refused, never discarded.
    Case {
        args: &["--version", "extra"],
        exit: 2,
        stderr: "ripr: usage: ripr --version (this route accepts no other arguments, and \"extra\" follows it); drop \"extra\" to print the version, or run `ripr --help` to list commands\n",
    },
    Case {
        args: &["-V", "check"],
        exit: 2,
        stderr: "ripr: usage: ripr --version (this route accepts no other arguments, and \"check\" follows it); drop \"check\" to print the version, or run `ripr --help` to list commands\n",
    },
    Case {
        args: &["reports", "ts-limitations"],
        exit: 2,
        stderr: "ripr: reports ts-limitations requires --check-output PATH; pass the JSON saved by `ripr check --format json > check.json`\n",
    },
    Case {
        args: &["reports", "ts-false-actionable"],
        exit: 2,
        stderr: "ripr: reports ts-false-actionable requires --corpus PATH; pass the TypeScript preview audit corpus JSON (`ripr reports ts-false-actionable --help` documents the corpus format)\n",
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
        stderr: "ripr: evidence-health root /ripr-missing-root is not a directory; pass the directory that contains the workspace (for a Cargo.toml path, its parent directory)\n",
    },
    // An option typed before the command. The `ripr check` example may only
    // name a spelling `check` accepts: `ripr -q check` used to be told to run
    // `ripr check -q ...`, which `check` rejects.
    Case {
        args: &["--base", "main", "check"],
        exit: 2,
        stderr: "ripr: unknown command \"--base\"; options go after the command, e.g. `ripr check --base ...`\n",
    },
    // `--name=value` is an accepted spelling after the command, so the
    // assignment keeps its example and is judged by its flag name.
    Case {
        args: &["--root=.", "check"],
        exit: 2,
        stderr: "ripr: unknown command \"--root=.\"; options go after the command, e.g. `ripr check --root=. ...`\n",
    },
    Case {
        args: &["--bogus=1", "check"],
        exit: 2,
        stderr: "ripr: unknown command \"--bogus=1\"; options go after the command, and `ripr check` has no `--bogus` option. Run `ripr --help`.\n",
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
        stderr: "ripr: unknown option --al; run `ripr --help` to list commands and options\n",
    },
    // #15201: the same near miss after the command gets no `--all` either.
    Case {
        args: &["check", "--al"],
        exit: 2,
        stderr: "ripr: unknown check argument \"--al\". Run `ripr check --help --all`.\n",
    },
    // #15898: a switch given a value names a flag that exists, so the refusal
    // says it is a switch and which spellings work.
    Case {
        args: &["check", "--quiet=false"],
        exit: 2,
        stderr: "ripr: unknown check argument \"--quiet=false\". `--quiet` is a switch and takes no value: write `--quiet` alone; a switch is on when present and off when absent. Run `ripr check --help --all`.\n",
    },
    Case {
        args: &["cache", "status", "--json=true"],
        exit: 2,
        stderr: "ripr: unknown cache status argument \"--json=true\". `--json` is a switch and takes no value: write `--json` alone; a switch is on when present and off when absent. Run `ripr cache status --help`.\n",
    },
    // A sibling's switch in a shared help body is not offered: `baseline
    // diff` has no `--dry-run`. Its owner, `baseline create`, is told.
    Case {
        args: &["baseline", "diff", "--dry-run=true"],
        exit: 2,
        stderr: "ripr: unknown baseline diff argument \"--dry-run=true\". Run `ripr baseline diff --help`.\n",
    },
    Case {
        args: &["baseline", "create", "--dry-run=true"],
        exit: 2,
        stderr: "ripr: unknown baseline create argument \"--dry-run=true\". `--dry-run` is a switch and takes no value: write `--dry-run` alone; a switch is on when present and off when absent. Run `ripr baseline create --help`.\n",
    },
    // A value-taking flag still splits at `=` and reaches its own validation.
    Case {
        args: &["pilot", "--format=yaml"],
        exit: 2,
        stderr: "ripr: unknown format \"yaml\". Accepted: human, json.\n",
    },
    Case {
        args: &["-x", "check"],
        exit: 2,
        stderr: "ripr: unknown command \"-x\"; options go after the command, and `ripr check` has no `-x` option. Run `ripr --help`.\n",
    },
];

/// The built binary, run from `dir`. Shared by the cases that need more
/// than fixed arguments in an empty directory.
fn ripr_in(dir: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command.current_dir(dir);
    command
}

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
    let output = ripr_in(&dir)
        .args(["pr-evidence", "--base", "main", "--head", "HEAD"])
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

/// #15898: a `ripr.toml` value of the wrong TOML shape. `ripr config
/// validate` names the shape in TOML terms and gives one line on how to write
/// it; the deserializer's own words (`usize`, `a sequence`, an internal
/// `struct` name) do not appear. The same next step reaches `ripr doctor`,
/// whose config line carries no source excerpt.
#[test]
fn wrong_shape_config_values_say_what_to_write() -> Result<(), String> {
    const CONFIGS: &[(&str, &str, &str)] = &[
        (
            "[reports]\nmax_related_tests = \"five\"\n",
            "invalid type: string \"five\", expected a whole number of 1 or more",
            "write a whole number of 1 or more with no quotes, for example `5`",
        ),
        (
            "[languages]\nenabled = \"rust\"\n",
            "invalid type: string \"rust\", expected a list",
            "write a list: values in square brackets, for example `[\"value\"]`, or for a list of tables one double-bracket header per entry, for example `[[overrides]]`",
        ),
        (
            "analysis = 5\n",
            "invalid type: integer `5`, expected a table",
            "write it as a table header on its own line, for example `[analysis]`, with its keys on the lines below",
        ),
    ];
    let dir = std::env::temp_dir().join(format!("ripr-config-shape-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|error| format!("create scratch: {error}"))?;
    let ceiling = dir.parent().unwrap_or(dir.as_path()).to_path_buf();
    let mut failures = Vec::new();
    for (config, reason, next) in CONFIGS {
        std::fs::write(dir.join("ripr.toml"), config)
            .map_err(|error| format!("write ripr.toml: {error}"))?;
        let validate = ripr_in(&dir)
            .args(["config", "validate"])
            .env("GIT_CEILING_DIRECTORIES", &ceiling)
            .output()
            .map_err(|error| format!("run ripr config validate: {error}"))?;
        let stderr = String::from_utf8_lossy(&validate.stderr);
        let lines: Vec<&str> = stderr.lines().collect();
        let reason_at = lines.iter().position(|line| line == reason);
        let followed = reason_at.is_some_and(|at| lines.get(at + 2) == Some(next));
        let jargon = ["usize", "a sequence", "struct ", "Raw"]
            .iter()
            .any(|word| stderr.contains(word));
        if validate.status.code() != Some(2) || !followed || jargon {
            failures.push(format!(
                "config validate for {config:?}: exit {:?}\n{stderr}",
                validate.status.code()
            ));
        }
        // The JSON refusal carries the same next step in its message.
        let json = ripr_in(&dir)
            .args(["check", "--root", ".", "--json"])
            .env("GIT_CEILING_DIRECTORIES", &ceiling)
            .output()
            .map_err(|error| format!("run ripr check --json: {error}"))?;
        let document = String::from_utf8_lossy(&json.stdout);
        let escaped = next.replace('"', "\\\"");
        if json.status.code() != Some(2) || !document.contains(&escaped) {
            failures.push(format!(
                "check --json for {config:?}: exit {:?}\n{document}",
                json.status.code()
            ));
        }
        let doctor = ripr_in(&dir)
            .args(["doctor", "--root", "."])
            .env("GIT_CEILING_DIRECTORIES", &ceiling)
            .output()
            .map_err(|error| format!("run ripr doctor: {error}"))?;
        let report = format!(
            "{}{}",
            String::from_utf8_lossy(&doctor.stdout),
            String::from_utf8_lossy(&doctor.stderr)
        );
        let named = report
            .lines()
            .any(|line| line.contains("TOML parse error at line ") && line.ends_with(next));
        if !named {
            failures.push(format!(
                "doctor for {config:?} lost the next step:\n{report}"
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
