//! `ripr first-pr --run-chain` (#11720): the opt-in that runs the missing
//! artifacts of the regeneration chain itself, then lets the ordinary write
//! path compose the packet.
//!
//! Every step is a command first-pr already prints for that artifact. The
//! printed display string is the single source: this module only tokenizes it
//! and re-executes the current binary with those arguments, so the scope
//! (`--root`, `--base`, `--head`, artifact paths) cannot diverge from what a
//! user would paste. Default `first-pr` never reaches this module.

use super::{
    DEFAULT_REPO_EXPOSURE, FirstPrOptions, available_repo_exposure, read_json,
    regenerate_check_output_gap_ledger_command, regenerate_repo_exposure_command,
    regenerate_repo_exposure_gap_ledger_command, repo_exposure_latency_report_available,
    resolve_path, review_comments_regeneration_command, ripr_flag_value, select_from_gap_ledger,
    tokenize_command_words, uses_check_output_gap_ledger,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Stdio;

/// One executable step of the chain, derived from a printed command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ChainStep {
    pub(super) label: &'static str,
    /// The printed command this step executes, for reporting.
    pub(super) display: String,
    /// Arguments after the program word (`ripr`).
    pub(super) args: Vec<String>,
    /// The file the step's stdout is redirected to (`> file` in the display).
    pub(super) stdout_to: Option<PathBuf>,
    /// The file that must exist once the step succeeded. A `.json` one must
    /// also hold non-empty, parseable JSON.
    pub(super) expected: PathBuf,
    /// Companion files (`--out-md`) that must exist once the step succeeded.
    pub(super) companions: Vec<PathBuf>,
    /// The review cards are consulted only when the gap ledger names no
    /// repair start (the packet's `needed_when`); this step runs on that
    /// condition alone, judged when its turn comes with the ledger on disk.
    pub(super) only_when_cards_needed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ChainPlan {
    Steps(Vec<ChainStep>),
    /// The chain does not apply; the reason is printed and the ordinary
    /// write path records the real state.
    NotApplicable(String),
}

/// The missing artifacts, in dependency order: repo exposure, gap ledger,
/// review cards. An artifact that exists is never regenerated: stale or
/// malformed evidence keeps its own recovery route.
pub(super) fn plan_chain(root: &Path, options: &FirstPrOptions) -> Result<ChainPlan, String> {
    let mut displays: Vec<(&'static str, String)> = Vec::new();
    if !resolve_path(root, &options.gap_ledger).exists() {
        if uses_check_output_gap_ledger(root) {
            // `ripr check ... --json > check.json && ripr reports gap-ledger ...`
            let compound = regenerate_check_output_gap_ledger_command(options);
            let mut parts = split_chain(&compound)?.into_iter();
            if let Some(check) = parts.next() {
                displays.push(("check output", check));
            }
            if let Some(ledger) = parts.next() {
                displays.push(("gap ledger", ledger));
            }
        } else {
            let exposure = available_repo_exposure(root, options);
            if exposure.is_none() {
                if repo_exposure_latency_report_available(root) {
                    return Ok(ChainPlan::NotApplicable(format!(
                        "the repo exposure report `{DEFAULT_REPO_EXPOSURE}` is missing and this repository routes that step through the bounded latency report instead"
                    )));
                }
                displays.push((
                    "repo exposure report",
                    regenerate_repo_exposure_command(&options.command_root()),
                ));
            }
            displays.push((
                "gap ledger",
                regenerate_repo_exposure_gap_ledger_command(
                    options,
                    exposure.unwrap_or(DEFAULT_REPO_EXPOSURE),
                ),
            ));
        }
    }
    if !resolve_path(root, &options.review_comments).exists() {
        displays.push((
            "review cards",
            review_comments_regeneration_command(root, options),
        ));
    }
    let mut steps = displays
        .into_iter()
        .map(|(label, display)| step_from_display(label, &display))
        .collect::<Result<Vec<_>, _>>()?;
    for step in &mut steps {
        step.only_when_cards_needed = step.label == "review cards";
    }
    Ok(ChainPlan::Steps(steps))
}

/// Split a printed compound command at its unquoted `&&`.
fn split_chain(command: &str) -> Result<Vec<String>, String> {
    let words = tokenize_command_words(command)
        .ok_or_else(|| format!("cannot read the printed command `{command}`"))?;
    // Re-render per segment from the original text: find each `&&` outside
    // quotes by walking the same quoting rules the tokenizer applies.
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some(active), c) if c == active => {
                quote = None;
                current.push(c);
            }
            (Some('"'), '\\') => {
                current.push(ch);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (Some(_), c) => current.push(c),
            (None, '\'' | '"') => {
                quote = Some(ch);
                current.push(ch);
            }
            (None, '\\') => {
                current.push(ch);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (None, '&') if chars.peek() == Some(&'&') => {
                chars.next();
                segments.push(std::mem::take(&mut current).trim().to_string());
            }
            (None, c) => current.push(c),
        }
    }
    segments.push(current.trim().to_string());
    // The segment count must agree with the tokenizer's operator count, so a
    // split can never disagree with how the words read.
    let operators = words
        .iter()
        .filter(|(word, quoted)| !quoted && word == "&&")
        .count();
    if segments.len() != operators + 1 {
        return Err(format!("cannot split the printed command `{command}`"));
    }
    Ok(segments)
}

fn step_from_display(label: &'static str, display: &str) -> Result<ChainStep, String> {
    let words = tokenize_command_words(display)
        .ok_or_else(|| format!("cannot read the printed {label} command `{display}`"))?;
    let mut args = Vec::new();
    let mut stdout_to = None;
    let mut iter = words.iter().enumerate();
    match iter.next() {
        Some((_, (program, _))) if program == "ripr" => {}
        _ => {
            return Err(format!(
                "the printed {label} command does not start with `ripr`"
            ));
        }
    }
    while let Some((index, (word, quoted))) = iter.next() {
        if !quoted && word == ">" {
            let Some((_, (target, _))) = iter.next() else {
                return Err(format!(
                    "the printed {label} command has a redirect with no target"
                ));
            };
            if iter.next().is_some() {
                return Err(format!(
                    "the printed {label} command has words after its redirect"
                ));
            }
            stdout_to = Some(PathBuf::from(target));
            break;
        }
        if !quoted && matches!(word.as_str(), ">>" | "|" | "&&" | "||" | ";") {
            return Err(format!(
                "the printed {label} command uses shell operator `{word}` at word {index}, which --run-chain does not execute"
            ));
        }
        // The control-character quoting form needs a shell to evaluate;
        // executing it literally would name a different path.
        if word.contains("$(printf") {
            return Err(format!(
                "the printed {label} command spells a path with control characters, which --run-chain cannot execute; run the printed command yourself"
            ));
        }
        args.push(word.clone());
    }
    let expected = match &stdout_to {
        Some(path) => path.clone(),
        None => ripr_flag_value(&words, "--out")
            .map(PathBuf::from)
            .ok_or_else(|| format!("the printed {label} command names no output file"))?,
    };
    let companions = ripr_flag_value(&words, "--out-md")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    Ok(ChainStep {
        label,
        display: display.to_string(),
        args,
        stdout_to,
        expected,
        companions,
        only_when_cards_needed: false,
    })
}

/// Run the steps in order, stopping at the first failure. Nothing is
/// published from a failed step: a redirected stdout is written only after
/// the step exits 0, so no partial artifact can be mistaken for a result.
pub(super) fn run_steps(
    root: &Path,
    options: &FirstPrOptions,
    steps: &[ChainStep],
) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|err| {
        format!("first-pr --run-chain: cannot resolve the running ripr executable: {err}")
    })?;
    let total = steps.len();
    for (index, step) in steps.iter().enumerate() {
        let number = index + 1;
        if step.only_when_cards_needed && !review_cards_needed(root, options) {
            println!(
                "run-chain step {number}/{total}: {}: not run; the gap ledger does not leave the repair start to the cards",
                step.label
            );
            continue;
        }
        println!(
            "run-chain step {number}/{total}: {}: {}",
            step.label, step.display
        );
        let output = Command::new(&exe)
            .args(&step.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|err| stopped(number, total, step, &format!("could not start: {err}")))?;
        // The step's diagnostics reach the operator exactly as if it had run
        // directly; the last of them is also the reason named below.
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            eprint!("{stderr}");
        }
        if !output.status.success() {
            let code = output
                .status
                .code()
                .map_or_else(|| "no exit code".to_string(), |code| format!("exit {code}"));
            let reason = match stderr.lines().rev().find(|line| !line.trim().is_empty()) {
                Some(line) => format!("failed ({code}); {}", line.trim()),
                None => format!("failed ({code})"),
            };
            return Err(stopped(number, total, step, &reason));
        }
        if let Some(target) = &step.stdout_to {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|err| {
                    stopped(
                        number,
                        total,
                        step,
                        &format!("cannot create {}: {err}", parent.display()),
                    )
                })?;
            }
            crate::output::file_write::write(target, &output.stdout).map_err(|err| {
                stopped(
                    number,
                    total,
                    step,
                    &crate::output::file_write::write_failure_message(target, &err),
                )
            })?;
        }
        if let Some(problem) = output_problem(step) {
            return Err(stopped(number, total, step, &problem));
        }
    }
    Ok(())
}

/// Exit 0 is not enough: the step must have left what it promised. A
/// `.json` artifact must hold non-empty, parseable JSON, so an empty or
/// truncated write cannot pass for evidence.
fn output_problem(step: &ChainStep) -> Option<String> {
    for companion in &step.companions {
        if !companion.is_file() {
            return Some(format!(
                "exited 0 but did not produce {}",
                companion.display()
            ));
        }
    }
    if !step.expected.is_file() {
        return Some(format!(
            "exited 0 but did not produce {}",
            step.expected.display()
        ));
    }
    if step.expected.extension().is_some_and(|ext| ext == "json") {
        let text = match std::fs::read_to_string(&step.expected) {
            Ok(text) => text,
            Err(err) => {
                return Some(format!(
                    "exited 0 but {} cannot be read: {err}",
                    step.expected.display()
                ));
            }
        };
        if text.trim().is_empty() {
            return Some(format!(
                "exited 0 but wrote an empty {}",
                step.expected.display()
            ));
        }
        if let Err(err) = serde_json::from_str::<serde_json::Value>(&text) {
            return Some(format!(
                "exited 0 but {} is not valid JSON: {err}",
                step.expected.display()
            ));
        }
    }
    None
}

/// The packet's `needed_when` for the review cards: the gap ledger names no
/// repair start. Judged exactly as the ordinary selection would with the
/// cards missing: it asks for the cards only when it reaches them. An
/// unreadable ledger, or one that selects or blocks on its own, does not.
pub(super) fn review_cards_needed(root: &Path, options: &FirstPrOptions) -> bool {
    let Ok(ledger) = read_json(&resolve_path(root, &options.gap_ledger)) else {
        return false;
    };
    let selected = select_from_gap_ledger(&ledger, root, options).to_json();
    selected["state"] == "missing_artifact" && selected["artifact"]["id"] == "review_comments"
}

fn stopped(number: usize, total: usize, step: &ChainStep, reason: &str) -> String {
    format!(
        "first-pr --run-chain stopped at step {number}/{total} ({}): {reason}. Later steps did not run and no start-here packet was written from partial artifacts. Fix the cause and rerun, or run the printed command yourself: {}",
        step.label, step.display
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_chain_splits_only_at_unquoted_operators() -> Result<(), String> {
        let parts = split_chain(
            "ripr check --root 'a && b' --json > 'x y.json' && ripr reports gap-ledger --out o.json",
        )?;
        assert_eq!(
            parts,
            vec![
                "ripr check --root 'a && b' --json > 'x y.json'".to_string(),
                "ripr reports gap-ledger --out o.json".to_string(),
            ]
        );
        Ok(())
    }

    #[test]
    fn step_from_display_takes_the_redirect_as_the_stdout_target() -> Result<(), String> {
        let step = step_from_display(
            "repo exposure report",
            "ripr check --root '/tmp/a b' --mode instant > /tmp/a\\ b/out.json",
        )?;
        assert_eq!(
            step.args,
            vec!["check", "--root", "/tmp/a b", "--mode", "instant"]
        );
        assert_eq!(step.stdout_to, Some(PathBuf::from("/tmp/a b/out.json")));
        assert_eq!(step.expected, PathBuf::from("/tmp/a b/out.json"));
        Ok(())
    }

    #[test]
    fn step_from_display_uses_out_as_the_expected_artifact() -> Result<(), String> {
        let step = step_from_display(
            "review cards",
            "ripr review-comments --root . --base main --head HEAD --out c.json",
        )?;
        assert_eq!(step.stdout_to, None);
        assert_eq!(step.expected, PathBuf::from("c.json"));
        Ok(())
    }

    #[test]
    fn step_from_display_refuses_what_it_cannot_execute_literally() {
        let refusal = |display: &str| step_from_display("x", display).err().unwrap_or_default();
        assert!(refusal("other check > f").contains("does not start with `ripr`"));
        assert!(refusal("ripr check | tee f --out f").contains("shell operator `|`"));
        assert!(
            refusal("ripr check --root 'a'\"$(printf '\\033')\"'b' --out f")
                .contains("control characters")
        );
        assert!(refusal("ripr check").contains("names no output file"));
    }

    fn step_expecting(path: &Path, companions: Vec<PathBuf>) -> ChainStep {
        ChainStep {
            label: "gap ledger",
            display: String::new(),
            args: Vec::new(),
            stdout_to: None,
            expected: path.to_path_buf(),
            companions,
            only_when_cards_needed: false,
        }
    }

    #[test]
    fn output_problem_fails_closed_on_an_exit_zero_that_left_nothing_usable() -> Result<(), String>
    {
        let dir = std::env::temp_dir().join(format!("ripr-11720-output-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let json = dir.join("a.json");
        let md = dir.join("a.md");
        let problem = |step: &ChainStep| output_problem(step).unwrap_or_default();

        assert!(problem(&step_expecting(&json, Vec::new())).contains("did not produce"));
        std::fs::write(&json, "  \n").map_err(|err| err.to_string())?;
        assert!(problem(&step_expecting(&json, Vec::new())).contains("empty"));
        std::fs::write(&json, "{\"a\":").map_err(|err| err.to_string())?;
        assert!(problem(&step_expecting(&json, Vec::new())).contains("not valid JSON"));
        std::fs::write(&json, "{}").map_err(|err| err.to_string())?;
        assert_eq!(output_problem(&step_expecting(&json, Vec::new())), None);
        assert!(problem(&step_expecting(&json, vec![md.clone()])).contains("a.md"));
        std::fs::write(&md, "x").map_err(|err| err.to_string())?;
        assert_eq!(output_problem(&step_expecting(&json, vec![md])), None);
        std::fs::remove_dir_all(&dir).map_err(|err| err.to_string())
    }
}
