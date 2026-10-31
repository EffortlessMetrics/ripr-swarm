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
    uses_check_output_gap_ledger,
};
use std::collections::VecDeque;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Stderr lines kept to name a failed step's own last diagnostic.
const STDERR_TAIL_LINES: usize = 20;

/// Per-step wall-clock bound for `--run-chain` when the environment names no
/// override. Zero disables the bound (tests inject a limit through
/// [`run_steps_with`]).
const DEFAULT_STEP_TIMEOUT: Duration = Duration::from_hours(2);

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
    let words = tokenize_run_chain_words(command).ok_or_else(|| {
        format!(
            "cannot read the printed command `{command}`; run the printed commands yourself instead"
        )
    })?;
    // Re-render per segment from the original text: find each `&&` outside
    // quotes by walking the same quoting rules the tokenizer applies.
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some('"'), '"') => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    current.push('"');
                } else {
                    quote = None;
                    current.push(ch);
                }
            }
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
        return Err(format!(
            "cannot split the printed command `{command}`; run the printed commands yourself instead"
        ));
    }
    Ok(segments)
}

/// Split a printed `--run-chain` command into words. POSIX single-quoted tokens
/// match [`tokenize_command_words`]; double-quoted tokens follow the Windows
/// console rules the product emits on that host (`""` and trailing `\`).
fn tokenize_run_chain_words(command: &str) -> Option<Vec<(String, bool)>> {
    let mut words: Vec<(String, bool)> = Vec::new();
    let mut current = String::new();
    let mut quoted_word = false;
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = command.chars().peekable();
    while let Some(character) = chars.next() {
        match (quote, character) {
            (Some('"'), '"') => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    current.push('"');
                } else {
                    let trailing = current.chars().rev().take_while(|ch| *ch == '\\').count();
                    for _ in 0..(trailing / 2) {
                        if current.ends_with('\\') {
                            current.pop();
                        }
                    }
                    quote = None;
                }
            }
            (Some(active), value) if value == active => quote = None,
            (Some('"'), '\\') => {
                current.push('\\');
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (Some(_), value) => current.push(value),
            (None, '\'' | '"') => {
                quote = Some(character);
                quoted_word = true;
                in_word = true;
            }
            (None, '\\') => {
                current.push(chars.next()?);
                in_word = true;
            }
            (None, value) if value.is_whitespace() => {
                if in_word {
                    words.push((std::mem::take(&mut current), quoted_word));
                }
                quoted_word = false;
                in_word = false;
            }
            (None, value) => {
                current.push(value);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if in_word {
        words.push((current, quoted_word));
    }
    Some(words)
}

fn step_from_display(label: &'static str, display: &str) -> Result<ChainStep, String> {
    let words = tokenize_run_chain_words(display)
        .ok_or_else(|| format!("cannot read the printed {label} command `{display}`; run the printed commands yourself instead"))?;
    let mut args = Vec::new();
    let mut stdout_to = None;
    let mut iter = words.iter().enumerate();
    match iter.next() {
        Some((_, (program, _))) if program == "ripr" => {}
        _ => {
            return Err(format!(
                "the printed {label} command does not start with `ripr`; run the printed commands yourself instead"
            ));
        }
    }
    while let Some((index, (word, quoted))) = iter.next() {
        if !quoted && word == ">" {
            let Some((_, (target, _))) = iter.next() else {
                return Err(format!(
                    "the printed {label} command has a redirect with no target; run the printed commands yourself instead"
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
                "the printed {label} command uses shell operator `{word}` at word {index}, which --run-chain does not execute; run the printed commands yourself instead"
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
            .ok_or_else(|| format!("the printed {label} command names no output file; run the printed commands yourself instead"))?,
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
    run_steps_with(&exe, root, options, steps, step_timeout_from_env())
}

fn step_timeout_from_env() -> Option<Duration> {
    match std::env::var("RIPR_FIRST_PR_RUN_CHAIN_STEP_TIMEOUT_SECS") {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed == "0" {
                None
            } else {
                trimmed.parse::<u64>().ok().map(Duration::from_secs)
            }
        }
        Err(_) => Some(DEFAULT_STEP_TIMEOUT),
    }
}

fn run_steps_with(
    exe: &Path,
    root: &Path,
    options: &FirstPrOptions,
    steps: &[ChainStep],
    step_timeout: Option<Duration>,
) -> Result<(), String> {
    let total = steps.len();
    for (index, step) in steps.iter().enumerate() {
        let number = index + 1;
        if step.only_when_cards_needed && !review_cards_needed(root, options) {
            println!(
                "run-chain step {number}/{total}: {}: not run; the gap ledger does not call for the cards (see the packet state)",
                step.label
            );
            continue;
        }
        println!(
            "run-chain step {number}/{total}: {}: {}",
            step.label, step.display
        );
        // What this step creates (not what an earlier run left): only that is
        // removed again if the step turns out not to have produced a result.
        let created = |path: &Path| !path.exists();
        let fresh_outputs: Vec<PathBuf> = std::iter::once(&step.expected)
            .chain(step.companions.iter())
            .filter(|path| created(path))
            .cloned()
            .collect();
        let mut child = Command::new(exe)
            .args(&step.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| stopped(number, total, step, &format!("could not start: {err}")))?;
        // Diagnostics stream live as if the step ran directly; the tail is
        // kept so a failure can name the step's own last diagnostic.
        let tail = child.stderr.take().map(|pipe| {
            std::thread::spawn(move || {
                let mut reader = std::io::BufReader::new(pipe);
                let mut recent: VecDeque<String> = VecDeque::new();
                let mut line = Vec::new();
                while matches!(reader.read_until(b'\n', &mut line), Ok(read) if read > 0) {
                    let text = String::from_utf8_lossy(&line).to_string();
                    eprint!("{text}");
                    if !text.trim().is_empty() {
                        if recent.len() == STDERR_TAIL_LINES {
                            recent.pop_front();
                        }
                        recent.push_back(text.trim().to_string());
                    }
                    line.clear();
                }
                recent
            })
        });
        let (status, stdout) = wait_for_step(
            &mut child,
            step_timeout,
            number,
            total,
            step,
            &fresh_outputs,
        )?;
        let recent = tail
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();
        if !status.success() {
            discard(&fresh_outputs);
            let code = status
                .code()
                .map_or_else(|| "no exit code".to_string(), |code| format!("exit {code}"));
            let reason = match recent.back() {
                Some(line) => format!("failed ({code}); {line}"),
                None => format!("failed ({code})"),
            };
            return Err(stopped(number, total, step, &reason));
        }
        if let Some(target) = &step.stdout_to {
            // Validate before publishing: a bad stdout never reaches the
            // final path, so a rerun cannot mistake it for evidence.
            if let Some(problem) = bytes_problem(target, &stdout) {
                return Err(stopped(number, total, step, &problem));
            }
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
            crate::output::file_write::write(target, &stdout).map_err(|err| {
                stopped(
                    number,
                    total,
                    step,
                    &crate::output::file_write::write_failure_message(target, &err),
                )
            })?;
        } else if let Some(problem) = output_problem(step) {
            // The step wrote its own files; take back what it just created
            // so the next run re-attempts it instead of skipping it.
            discard(&fresh_outputs);
            return Err(stopped(number, total, step, &problem));
        }
    }
    Ok(())
}

fn wait_for_step(
    child: &mut Child,
    step_timeout: Option<Duration>,
    number: usize,
    total: usize,
    step: &ChainStep,
    fresh_outputs: &[PathBuf],
) -> Result<(ExitStatus, Vec<u8>), String> {
    let stdout = child.stdout.take();
    let stdout_handle = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = stdout {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    });
    let status = match step_timeout {
        None => child
            .wait()
            .map_err(|err| stopped(number, total, step, &format!("could not run: {err}")))?,
        Some(limit) => {
            let deadline = Instant::now() + limit;
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => {
                        if Instant::now() >= deadline {
                            discard(fresh_outputs);
                            let _ = child.kill();
                            let _ = child.wait();
                            let _ = stdout_handle.join();
                            let seconds = limit.as_secs();
                            return Err(stopped(
                                number,
                                total,
                                step,
                                &format!(
                                    "timed out after {seconds} second(s); the step did not finish"
                                ),
                            ));
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(err) => {
                        discard(fresh_outputs);
                        return Err(stopped(
                            number,
                            total,
                            step,
                            &format!("could not run: {err}"),
                        ));
                    }
                }
            }
        }
    };
    let stdout = stdout_handle.join().map_err(|join| {
        stopped(
            number,
            total,
            step,
            &format!("stdout reader panicked: {join:?}"),
        )
    })?;
    Ok((status, stdout))
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
    let bytes = match std::fs::read(&step.expected) {
        Ok(bytes) => bytes,
        Err(err) => {
            return Some(format!(
                "exited 0 but {} cannot be read: {err}",
                step.expected.display()
            ));
        }
    };
    bytes_problem(&step.expected, &bytes)
}

/// A `.json` artifact must be non-empty, parseable JSON.
fn bytes_problem(path: &Path, bytes: &[u8]) -> Option<String> {
    if path.extension().is_none_or(|ext| ext != "json") {
        return None;
    }
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Some(format!("exited 0 but wrote an empty {}", path.display()));
    }
    if let Err(err) = serde_json::from_slice::<serde_json::Value>(bytes) {
        return Some(format!(
            "exited 0 but {} is not valid JSON: {err}",
            path.display()
        ));
    }
    None
}

fn discard(paths: &[PathBuf]) {
    for path in paths {
        // Best effort: a file that is already gone needs no removal.
        let _ = std::fs::remove_file(path);
    }
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

    /// `check-local-context` forbids drive-letter path literals in source; build
    /// Windows-shaped paths from parts like the product does on that host.
    fn windows_path(drive_letter: char, segments: &[&str]) -> String {
        let bs = '\\';
        let mut path = format!("{drive_letter}:");
        for segment in segments {
            path.push(bs);
            path.push_str(segment);
        }
        path
    }

    #[test]
    fn split_chain_splits_windows_double_quoted_paths_at_unquoted_and() -> Result<(), String> {
        let root = windows_path('C', &["tmp", "a && b"]);
        let out = windows_path('C', &["out", "x y.json"]);
        let compound = format!(
            r#"ripr check --root "{root}" --json > "{out}" && ripr reports gap-ledger --out o.json"#
        );
        let parts = split_chain(&compound)?;
        assert_eq!(
            parts,
            vec![
                format!(r#"ripr check --root "{root}" --json > "{out}""#),
                "ripr reports gap-ledger --out o.json".to_string(),
            ]
        );
        Ok(())
    }

    #[test]
    fn step_from_display_reads_windows_console_double_quoted_paths() -> Result<(), String> {
        let root = windows_path('C', &["tmp", "probe win space"]);
        let out = windows_path('C', &["out", "a b.json"]);
        let display = format!(r#"ripr check --root "{root}" --json > "{out}""#);
        let step = step_from_display("repo exposure report", &display)?;
        assert_eq!(
            step.args,
            vec!["check", "--root", root.as_str(), "--json",]
        );
        assert_eq!(step.stdout_to, Some(PathBuf::from(out.clone())));
        assert_eq!(step.expected, PathBuf::from(out));
        Ok(())
    }

    #[test]
    fn step_from_display_doubles_a_trailing_backslash_before_the_closing_quote()
    -> Result<(), String> {
        let root = windows_path('C', &["repo", ""]);
        let out = windows_path('D', &["out", "file.json"]);
        let quoted_root = format!("\"{root}\\\"");
        let display = format!(r#"ripr check --root {quoted_root} --mode instant > "{out}""#);
        let step = step_from_display("repo exposure report", &display)?;
        assert_eq!(
            step.args,
            vec!["check", "--root", root.as_str(), "--mode", "instant",]
        );
        assert_eq!(step.stdout_to, Some(PathBuf::from(out)));
        Ok(())
    }

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

    #[cfg(unix)]
    fn sh_step(script: &str, stdout_to: Option<&Path>, expected: &Path) -> ChainStep {
        ChainStep {
            label: "gap ledger",
            display: format!("sh -c {script}"),
            args: vec![
                "-c".to_string(),
                script.to_string(),
                expected.to_string_lossy().to_string(),
            ],
            stdout_to: stdout_to.map(Path::to_path_buf),
            expected: expected.to_path_buf(),
            companions: Vec::new(),
            only_when_cards_needed: false,
        }
    }

    #[cfg(unix)]
    fn run_sh(step: ChainStep) -> Result<(), String> {
        let options = FirstPrOptions::default();
        run_steps_with(
            Path::new("/bin/sh"),
            Path::new("."),
            &options,
            &[step],
            None,
        )
    }

    #[cfg(unix)]
    fn run_sh_with_timeout(step: ChainStep, step_timeout: Duration) -> Result<(), String> {
        let options = FirstPrOptions::default();
        run_steps_with(
            Path::new("/bin/sh"),
            Path::new("."),
            &options,
            &[step],
            Some(step_timeout),
        )
    }

    #[cfg(unix)]
    #[test]
    fn a_step_that_exceeds_the_timeout_fails_closed_and_names_the_step() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-11720-timeout-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let fifo = dir.join("block.fifo");
        let script = format!(
            "rm -f '{}' && mkfifo '{}' && read -r _ < '{}'",
            fifo.display(),
            fifo.display(),
            fifo.display()
        );
        let target = dir.join("unused.json");
        let error =
            run_sh_with_timeout(sh_step(&script, None, &target), Duration::from_millis(200))
                .err()
                .unwrap_or_default();
        assert!(error.contains("timed out"), "{error}");
        assert!(error.contains("(gap ledger)"), "{error}");
        assert!(
            error.contains("no start-here packet was written"),
            "{error}"
        );
        assert!(!target.exists());
        std::fs::remove_dir_all(&dir).map_err(|err| err.to_string())
    }

    #[cfg(unix)]
    #[test]
    fn invalid_stdout_never_reaches_the_final_path_and_a_rerun_retries() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-11720-stdout-{}", std::process::id()));
        let target = dir.join("out.json");
        for bad in ["printf ''", "printf '{\"a\":'"] {
            let error = run_sh(sh_step(bad, Some(&target), &target))
                .err()
                .unwrap_or_default();
            assert!(error.contains("exited 0 but"), "{error}");
            assert!(!target.exists(), "a bad stdout was published");
        }
        // The path is still free, so a rerun plans and runs the step again.
        run_sh(sh_step("printf '{}'", Some(&target), &target))?;
        assert_eq!(
            std::fs::read_to_string(&target).map_err(|e| e.to_string())?,
            "{}"
        );
        std::fs::remove_dir_all(&dir).map_err(|err| err.to_string())
    }

    #[cfg(unix)]
    #[test]
    fn a_bad_file_the_step_just_created_is_taken_back_but_an_older_one_is_kept()
    -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-11720-created-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let target = dir.join("own.json");
        let writes_empty = "printf '' > \"$0\"";
        let error = run_sh(sh_step(writes_empty, None, &target))
            .err()
            .unwrap_or_default();
        assert!(error.contains("empty"), "{error}");
        assert!(!target.exists(), "the step's own bad file was left behind");
        // A file that existed before the step is not this chain's to remove.
        std::fs::write(&target, "").map_err(|err| err.to_string())?;
        assert!(run_sh(sh_step(writes_empty, None, &target)).is_err());
        assert!(target.exists());
        // A failing exit takes back what it created and names its last stderr line.
        let other = dir.join("fail.json");
        let error = run_sh(sh_step(
            "echo first >&2; echo boom >&2; printf x > \"$0\"; exit 3",
            None,
            &other,
        ))
        .err()
        .unwrap_or_default();
        assert!(error.contains("failed (exit 3); boom"), "{error}");
        assert!(!other.exists());
        std::fs::remove_dir_all(&dir).map_err(|err| err.to_string())
    }
}
