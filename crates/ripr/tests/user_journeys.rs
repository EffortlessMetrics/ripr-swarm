//! Whole user journeys, end to end, following only what ripr and its docs say
//! (#14794).
//!
//! Other harnesses pin one surface of first use each: `first_pr_run_chain`
//! (the missing-artifact chain), `agent_command_journey` (the first-action
//! funnel), `printed_command_paste` (shell quoting of every printed command),
//! `installed_python_typescript_journeys` (installed-binary fixture walks),
//! `task_first_facade` (the repair façade controls) and `documented_commands`
//! (spelling of documented invocations). None of them walks one journey from
//! a fresh change to green. These tests do, for each route a user is sent
//! down:
//!
//! - the `docs/how-to/first-rust-test.md` walkthrough, materialized from the
//!   markdown itself (its project files, patch and appended test) and run
//!   step by step, including the native `cargo test` and the documented
//!   wrong-behavior control;
//! - Rust by hand: bare `ripr check` → the printed `explain` → the printed
//!   stub `--write` command → fill the one value → the printed `Run it:`
//!   command → bare `ripr check` again;
//! - Rust agent loop: the printed `ripr repair` start → add the named test →
//!   the printed test command → the printed `ripr continue` → `ripr status`
//!   → bare `ripr check`;
//! - Python agent loop: the printed `ripr agent repair` start → add and
//!   commit the test → the printed `ripr continue` → bare `ripr check`;
//! - TypeScript by hand: the printed `explain` → add the test → bare
//!   `ripr check`.
//!
//! A step fails its journey when it dead-ends (a nonzero exit, no printed
//! follow-up, a follow-up whose program is not `ripr` or `cargo`, or one that
//! names another finding) or when the advice is wrong (the missing check or
//! drafted inputs are not the case that tells `>` from `>=`). Each route also
//! has a discriminating negative: a test that does not reach the boundary
//! leaves the gap open, so the green ending is not vacuous.
//!
//! Printed follow-ups run from a foreign working directory, so each one has
//! to carry its own root. Each is split with POSIX quoting rules, which is
//! how ripr quotes on Unix; the Windows paste route belongs to
//! `printed_command_paste`, so this file is Unix-only.
//!
//! Native test execution is real for Rust (the fixture crate has no
//! dependencies, so `cargo test` runs offline in its own target directory).
//! Python and TypeScript journeys stop at the static recheck: pytest and a
//! TypeScript runner are not guaranteed on CI hosts, and the real-pytest
//! control in `installed_python_typescript_journeys` stays the owner of that
//! evidence.

#![cfg(all(unix, feature = "lang-rust"))]

#[path = "common/printed_words.rs"]
mod printed_words;

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Every child is bounded: a hung step fails its journey instead of the CI job.
/// Nested `cargo test` compiles a dependency-free crate; analysis runs on a
/// two-file repository. Both budgets are generous for debug builds.
const STEP_BUDGET: Duration = Duration::from_mins(4);

const RIPR: &str = env!("CARGO_BIN_EXE_ripr");

// ---------------------------------------------------------------------------
// Scratch space and process helpers
// ---------------------------------------------------------------------------

/// A scratch directory holding the repository and a foreign launch directory.
/// Removed on drop; a failed removal leaves only temp-dir litter.
struct Scratch {
    base: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Result<Self, String> {
        static N: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().join(format!(
            "ripr-journey-{label}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        if base.exists() {
            fs::remove_dir_all(&base).map_err(|e| format!("clear {}: {e}", base.display()))?;
        }
        fs::create_dir_all(base.join("repo")).map_err(|e| e.to_string())?;
        fs::create_dir_all(base.join("elsewhere")).map_err(|e| e.to_string())?;
        Ok(Self { base })
    }

    fn repo(&self) -> PathBuf {
        self.base.join("repo")
    }

    /// Where printed follow-ups are pasted: not the repository, not a parent.
    fn elsewhere(&self) -> PathBuf {
        self.base.join("elsewhere")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn both(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn describe(label: &str, output: &Output) -> String {
    format!(
        "{label}: exit {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn succeed(label: &str, output: Output) -> Result<Output, String> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!("dead end at {}", describe(label, &output)))
    }
}

/// Run one child through the shared process owner with file-backed streams
/// (no pipe backpressure) in a fresh process group, so on budget overrun the
/// whole tree, including nested rustc and test binaries, is terminated and reaped.
fn spawn(mut command: Command, label: &str) -> Result<Output, String> {
    static N: AtomicU64 = AtomicU64::new(0);
    let stem = std::env::temp_dir().join(format!(
        "ripr-journey-stream-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let out_path = stem.with_extension("stdout");
    let err_path = stem.with_extension("stderr");
    let open =
        |path: &Path| fs::File::create(path).map_err(|e| format!("create {}: {e}", path.display()));
    command
        .stdin(Stdio::null())
        .stdout(open(&out_path)?)
        .stderr(open(&err_path)?);
    let result = (|| {
        let mut child = ripr::process_owner::OwnedProcess::spawn_isolated(command)
            .map_err(|e| format!("spawn {label}: {e}"))?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| format!("wait {label}: {e}"))? {
                break status;
            }
            if started.elapsed() >= STEP_BUDGET {
                child.terminate_tree()?;
                return Err(format!(
                    "{label} exceeded its {STEP_BUDGET:?} budget; terminated and reaped"
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        Ok(Output {
            status,
            stdout: fs::read(&out_path).map_err(|e| e.to_string())?,
            stderr: fs::read(&err_path).map_err(|e| e.to_string())?,
        })
    })();
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_file(&err_path);
    result
}

/// Git with a fixed identity, isolated from the developer's config and from
/// any repository the test process itself runs inside.
fn git_command(cwd: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Journey")
        .env("GIT_AUTHOR_EMAIL", "journey@example.com")
        .env("GIT_COMMITTER_NAME", "Journey")
        .env("GIT_COMMITTER_EMAIL", "journey@example.com")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR");
    command
}

fn git(cwd: &Path, args: &[&str]) -> Result<(), String> {
    let mut command = git_command(cwd);
    command.args(["-c", "commit.gpgSign=false"]).args(args);
    succeed(&format!("git {}", args.join(" ")), spawn(command, "git")?).map(|_| ())
}

fn ripr_command(cwd: &Path) -> Command {
    let mut command = Command::new(RIPR);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("RIPR_") {
            command.env_remove(key);
        }
    }
    command
        .current_dir(cwd)
        .env("RIPR_QUIET", "1")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    command
}

/// A nested `cargo` for the fixture crate: its own target directory, offline,
/// and without the outer build's flags or coverage instrumentation.
fn cargo_command(cwd: &Path, target_dir: &Path) -> Command {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command
        .current_dir(cwd)
        .env("CARGO_TARGET_DIR", target_dir)
        .env("CARGO_NET_OFFLINE", "true")
        .env("CARGO_TERM_COLOR", "never")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("CARGO_BUILD_RUSTFLAGS")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("RUSTDOCFLAGS")
        .env_remove("CARGO_ENCODED_RUSTDOCFLAGS")
        .env_remove("LLVM_PROFILE_FILE");
    command
}

/// Bare `ripr check` from the repository root: the first thing a user types.
fn bare_check(root: &Path) -> Result<String, String> {
    let mut command = ripr_command(root);
    command.arg("check");
    succeed("ripr check", spawn(command, "ripr check")?).map(|o| stdout(&o))
}

// ---------------------------------------------------------------------------
// Following printed commands
// ---------------------------------------------------------------------------

/// Split one command line from the walkthrough page. The page uses plain
/// words, single quotes and simple double-quoted messages, so this accepts
/// those and refuses anything whose meaning a shell would change: shell
/// operators, expansions, and backslashes inside double quotes. Printed
/// ripr follow-ups go through the shared `printed_words` decoder instead.
fn doc_words(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => current.push(inner),
                        None => return Err(format!("unterminated single quote in {line:?}")),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some(special @ ('\\' | '$' | '`')) => {
                            return Err(format!(
                                "walkthrough command uses {special:?} inside double quotes: {line:?}"
                            ));
                        }
                        Some(inner) => current.push(inner),
                        None => return Err(format!("unterminated double quote in {line:?}")),
                    }
                }
            }
            '\\' => {
                in_word = true;
                match chars.next() {
                    Some(escaped) => current.push(escaped),
                    None => return Err(format!("dangling escape in {line:?}")),
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            ';' | '&' | '|' | '>' | '<' | '`' | '$' => {
                return Err(format!(
                    "walkthrough command is not one plain command ({c:?}): {line:?}"
                ));
            }
            c => {
                in_word = true;
                current.push(c);
            }
        }
    }
    if in_word {
        words.push(current);
    }
    Ok(words)
}

/// The text after `prefix` on the first output line that, trimmed, starts
/// with it. `None` is the dead end "ripr printed no such follow-up".
fn printed_after(text: &str, prefix: &str) -> Option<String> {
    text.lines().map(str::trim).find_map(|line| {
        line.strip_prefix(prefix)
            .map(|rest| rest.trim().to_string())
    })
}

/// The first line after the line containing `heading`.
fn line_after(text: &str, heading: &str) -> Option<String> {
    let mut lines = text.lines();
    lines.by_ref().find(|line| line.contains(heading))?;
    lines.next().map(|line| line.trim().to_string())
}

fn need_printed(text: &str, prefix: &str, step: &str) -> Result<String, String> {
    printed_after(text, prefix).ok_or_else(|| {
        format!("dead end at {step}: ripr printed no `{prefix}` follow-up\n--- output ---\n{text}")
    })
}

fn need(text: &str, needle: &str, step: &str) -> Result<(), String> {
    if text.contains(needle) {
        Ok(())
    } else {
        Err(format!(
            "{step}: expected `{needle}`\n--- output ---\n{text}"
        ))
    }
}

fn refuse(text: &str, needle: &str, step: &str) -> Result<(), String> {
    if text.contains(needle) {
        Err(format!(
            "{step}: did not expect `{needle}`\n--- output ---\n{text}"
        ))
    } else {
        Ok(())
    }
}

/// Run one printed follow-up exactly as printed, from the foreign launch
/// directory. Only `ripr` and `cargo` are programs ripr should print.
fn run_printed(line: &str, scratch: &Scratch) -> Result<Output, String> {
    let words = printed_words::printed_words(line)?;
    if let Some(operator) = words.iter().find(|w| {
        matches!(
            w.as_str(),
            "&&" | "||" | ";" | "|" | ">" | ">>" | "<" | "2>" | "&"
        )
    }) {
        return Err(format!(
            "printed follow-up is not one plain command (`{operator}`): {line:?}"
        ));
    }
    let Some((program, args)) = words.split_first() else {
        return Err(format!("empty printed follow-up: {line:?}"));
    };
    let launch = scratch.elsewhere();
    let mut command = match program.as_str() {
        "ripr" => ripr_command(&launch),
        "cargo" => cargo_command(&launch, &scratch.base.join("cargo-target")),
        other => {
            return Err(format!(
                "dead end: printed follow-up runs `{other}`, not ripr or cargo: {line:?}"
            ));
        }
    };
    command.args(args);
    let output = spawn(command, line)?;
    let stray = fs::read_dir(&launch)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    if !stray.is_empty() {
        return Err(format!(
            "printed follow-up {line:?} wrote {stray:?} into the directory it was pasted from"
        ));
    }
    Ok(output)
}

fn follow(line: &str, scratch: &Scratch) -> Result<Output, String> {
    succeed(&format!("printed `{line}`"), run_printed(line, scratch)?)
}

/// A native libtest run selected exactly one test and it passed.
fn exactly_one_passed(output: &Output, label: &str) -> Result<(), String> {
    let text = stdout(output);
    let passed: u32 = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("test result: ok. "))
        .filter_map(|rest| rest.split(" passed").next()?.trim().parse::<u32>().ok())
        .sum();
    if output.status.success() && passed == 1 {
        Ok(())
    } else {
        Err(format!(
            "{label}: wanted exactly 1 passed test, saw {passed}\n{}",
            describe(label, output)
        ))
    }
}

/// The wrong-behavior control: libtest compiled and ran the one test, and it
/// failed on its assertion (not a compile error, lock or missing test).
fn exactly_one_failed_on(
    output: &Output,
    left: &str,
    right: &str,
    label: &str,
) -> Result<(), String> {
    let text = both(output);
    let ran_and_failed = text.contains("test result: FAILED. 0 passed; 1 failed");
    let on_assertion =
        text.contains(&format!("left: {left}")) && text.contains(&format!("right: {right}"));
    if !output.status.success() && ran_and_failed && on_assertion {
        Ok(())
    } else {
        Err(format!(
            "{label}: wanted the one test to fail on `left: {left}` / `right: {right}`\n{}",
            describe(label, output)
        ))
    }
}

fn append(path: &Path, text: &str) -> Result<(), String> {
    let mut body = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    body.push_str(text);
    fs::write(path, body).map_err(|e| format!("write {}: {e}", path.display()))
}

fn replace_once(path: &Path, from: &str, to: &str) -> Result<(), String> {
    let body = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if body.matches(from).count() != 1 {
        return Err(format!(
            "expected `{from}` exactly once in {}",
            path.display()
        ));
    }
    fs::write(path, body.replacen(from, to, 1)).map_err(|e| e.to_string())
}

fn write(root: &Path, relative: &str, body: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, body).map_err(|e| format!("write {}: {e}", path.display()))
}

/// `main` holds `files`; `feature` commits one edit of `changed` (`from` →
/// `to`). The same shape a user has after committing a change on a branch.
fn branch_repo(
    root: &Path,
    files: &[(&str, &str)],
    changed: &str,
    from: &str,
    to: &str,
) -> Result<(), String> {
    for (relative, body) in files {
        write(root, relative, body)?;
    }
    git(root, &["init", "-q", "-b", "main"])?;
    git(root, &["add", "-A"])?;
    git(root, &["commit", "-q", "-m", "base"])?;
    git(root, &["checkout", "-q", "-b", "feature"])?;
    replace_once(&root.join(changed), from, to)?;
    git(
        root,
        &["commit", "-q", "-am", "change the threshold comparison"],
    )
}

// ---------------------------------------------------------------------------
// Rust fixture shared by the two Rust routes
// ---------------------------------------------------------------------------

const RUST_LIB: &str = "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {
    if amount > discount_threshold {
        amount - 10
    } else {
        amount
    }
}
";

const RUST_TESTS: &str = "use journey::discounted_total;

#[test]
fn below_threshold_has_no_discount() {
    assert_eq!(discounted_total(50, 100), 50);
}

#[test]
fn far_above_threshold_discounts() {
    assert_eq!(discounted_total(10_000, 100), 9_990);
}
";

/// The requirement behind the change: the 10-unit discount applies at or
/// above the threshold, so `discounted_total(100, 100)` is `90`. The value is
/// derived from that requirement, never copied from ripr's draft.
const RUST_EQUAL_CASE_EXPECTED: &str = "90";

fn rust_repo(root: &Path) -> Result<(), String> {
    branch_repo(
        root,
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"journey\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
            ),
            (".gitignore", "/target/\n"),
            ("src/lib.rs", RUST_LIB),
            ("tests/pricing.rs", RUST_TESTS),
        ],
        "src/lib.rs",
        "amount > discount_threshold",
        "amount >= discount_threshold",
    )
}

const RUST_GAP_ID: &str =
    "gap:rust:src/lib.rs:discounted_total:predicate_boundary:predicate:amount==discount_threshold";

/// The same crate with its library at `src/100%.rs`: a literal `%` in a
/// source file name. Paths in ripr's identities spell it `%25`.
fn rust_percent_repo(root: &Path) -> Result<(), String> {
    branch_repo(
        root,
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"journey\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/100%.rs\"\n\n[workspace]\n",
            ),
            (".gitignore", "/target/\n"),
            ("src/100%.rs", RUST_LIB),
            ("tests/pricing.rs", RUST_TESTS),
        ],
        "src/100%.rs",
        "amount > discount_threshold",
        "amount >= discount_threshold",
    )
}

const RUST_PERCENT_GAP_ID: &str = "gap:rust:src/100%25.rs:discounted_total:predicate_boundary:predicate:amount==discount_threshold";

/// The first check on the Rust change names the right gap and the right
/// missing case. Wrong advice (another predicate, another case) fails here.
fn assert_rust_first_check(check: &str, gap_id: &str) -> Result<(), String> {
    need(
        check,
        "State: a test gap to inspect or repair",
        "first check",
    )?;
    need(
        check,
        "Changed behavior: amount >= discount_threshold",
        "first check",
    )?;
    need(
        check,
        "Missing check: amount == discount_threshold",
        "first check",
    )?;
    need(check, gap_id, "first check")
}

fn assert_rust_closed(check: &str, step: &str) -> Result<(), String> {
    need(check, "State: no test gap found", step)?;
    need(check, "Static exposure: exposed", step)?;
    need(check, RUST_GAP_ID, step)
}

// ---------------------------------------------------------------------------
// Journey: Rust by hand (check → explain → stub --write → run → recheck)
// ---------------------------------------------------------------------------

#[test]
fn rust_check_explain_stub_run_recheck_reaches_noticed() -> Result<(), String> {
    let scratch = Scratch::new("rust-hand")?;
    let root = scratch.repo();
    rust_repo(&root)?;

    let check = bare_check(&root)?;
    assert_rust_first_check(&check, RUST_GAP_ID)?;

    // The printed `explain` names the same gap and the same weak evidence.
    let explain_line = line_after(&check, "(finding id, for explain/context)")
        .ok_or_else(|| format!("dead end: no explain follow-up\n{check}"))?;
    if !explain_line.starts_with("ripr explain ") {
        return Err(format!(
            "expected a `ripr explain` follow-up, got {explain_line:?}"
        ));
    }
    let explain = stdout(&follow(&explain_line, &scratch)?);
    need(&explain, RUST_GAP_ID, "printed explain")?;
    need(&explain, "weakly_exposed", "printed explain")?;

    // "Write a test for it:" prints the stub preview; its own `Write it:`
    // line is the command that writes the stub.
    let stub_line = line_after(&check, "Write a test for it:")
        .ok_or_else(|| format!("dead end: no stub follow-up\n{check}"))?;
    let preview = stdout(&follow(&stub_line, &scratch)?);
    // Advice check: the drafted inputs are the equality case.
    need(
        &preview,
        "amount = 100, discount_threshold = 100",
        "stub preview",
    )?;
    let write_line = need_printed(&preview, "Write it:", "stub preview")?;
    let written = stdout(&follow(&write_line, &scratch)?);
    let run_line = need_printed(&written, "Run it:", "stub write")?;

    // Fill the one value the stub asks for, and drop the allow its comment
    // says to delete with the last todo!().
    let lib = root.join("src/lib.rs");
    let body = fs::read_to_string(&lib).map_err(|e| e.to_string())?;
    let todo_start = body
        .find("todo!(\"")
        .ok_or_else(|| format!("stub wrote no todo!() to fill\n{body}"))?;
    let todo_end = body[todo_start..]
        .find(");")
        .map(|offset| todo_start + offset + 1)
        .ok_or_else(|| format!("unterminated todo!() in stub\n{body}"))?;
    let mut filled = body.clone();
    filled.replace_range(todo_start..todo_end, RUST_EQUAL_CASE_EXPECTED);
    let filled = filled
        .lines()
        .filter(|line| !line.contains("delete with the last todo!()"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(&lib, filled).map_err(|e| e.to_string())?;

    // The printed run command selects exactly the new test, and it passes.
    let run = run_printed(&run_line, &scratch)?;
    exactly_one_passed(&run, "printed Run it")?;

    // Wrong-behavior control: the filled test fails on the old comparison.
    replace_once(
        &lib,
        "if amount >= discount_threshold {",
        "if amount > discount_threshold {",
    )?;
    exactly_one_failed_on(
        &run_printed(&run_line, &scratch)?,
        "100",
        RUST_EQUAL_CASE_EXPECTED,
        "filled stub test on the old `>` comparison",
    )?;
    replace_once(
        &lib,
        "if amount > discount_threshold {",
        "if amount >= discount_threshold {",
    )?;

    // The recheck runs after the fill. It does not by itself prove the fill:
    // the unfilled `todo!()` stub already reads exposed today (#14810), so the
    // native run and its control above are what hold the written value.
    assert_rust_closed(&bare_check(&root)?, "recheck after the stub test")
}

// ---------------------------------------------------------------------------
// Journey: Rust agent loop (check → repair → test → continue → status)
// ---------------------------------------------------------------------------

struct RustRepairStart {
    test_file: PathBuf,
    test_name: String,
    run_line: String,
    continue_line: String,
}

fn start_rust_repair(scratch: &Scratch, gap_id: &str) -> Result<RustRepairStart, String> {
    let root = scratch.repo();
    let check = bare_check(&root)?;
    assert_rust_first_check(&check, gap_id)?;
    let repair_line = line_after(&check, "(spot id, for repair)")
        .ok_or_else(|| format!("dead end: no repair start printed\n{check}"))?;
    if !repair_line.starts_with("ripr repair ") {
        return Err(format!(
            "expected a `ripr repair` start, got {repair_line:?}"
        ));
    }
    let started = both(&follow(&repair_line, scratch)?);
    need(
        &started,
        "missing discriminator: discount_threshold (equality boundary)",
        "repair",
    )?;
    // Advice check: the drafted assertion is the equality case with the value
    // the requirement gives, not merely the right inputs.
    need(
        &started,
        &format!(
            "assertion shape: assert_eq!(discounted_total(100, 100), {RUST_EQUAL_CASE_EXPECTED})"
        ),
        "repair",
    )?;

    // "edit one test file: tests/pricing.rs (suggested test `NAME`)"
    let edit = need_printed(&started, "edit one test file:", "repair")?;
    let (file, rest) = edit
        .split_once(" (suggested test `")
        .ok_or_else(|| format!("repair named no suggested test: {edit:?}"))?;
    let name = rest
        .split_once('`')
        .map(|(name, _)| name.to_string())
        .ok_or_else(|| format!("unterminated suggested test name: {edit:?}"))?;
    Ok(RustRepairStart {
        test_file: root.join(file),
        test_name: name,
        run_line: need_printed(&started, "Run the new test:", "repair")?,
        continue_line: need_printed(&started, "Next, after the test edit:", "repair")?,
    })
}

#[test]
fn rust_repair_test_continue_status_reaches_closed() -> Result<(), String> {
    let scratch = Scratch::new("rust-repair")?;
    let root = scratch.repo();
    rust_repo(&root)?;
    let start = start_rust_repair(&scratch, RUST_GAP_ID)?;

    append(
        &start.test_file,
        &format!(
            "\n#[test]\nfn {}() {{\n    assert_eq!(discounted_total(100, 100), {RUST_EQUAL_CASE_EXPECTED});\n}}\n",
            start.test_name
        ),
    )?;
    exactly_one_passed(
        &run_printed(&start.run_line, &scratch)?,
        "printed test command",
    )?;

    let finished = both(&follow(&start.continue_line, &scratch)?);
    need(&finished, "weak -> exposed", "continue")?;

    let mut status = ripr_command(&scratch.elsewhere());
    status.args(["status", "--root"]).arg(&root);
    let status = stdout(&succeed("ripr status", spawn(status, "ripr status")?)?);
    need(&status, "Status: complete", "status")?;

    assert_rust_closed(&bare_check(&root)?, "recheck after the repair")
}

/// A `%` in the changed file's name: the check screen offers the repair
/// start (it used to refuse with "classifying the finding's spots failed",
/// because the classifier opened `src/100%25.rs`), and the loop closes.
#[test]
fn rust_repair_closes_when_the_source_file_name_has_a_percent() -> Result<(), String> {
    let scratch = Scratch::new("rust-repair-percent")?;
    let root = scratch.repo();
    rust_percent_repo(&root)?;
    let start = start_rust_repair(&scratch, RUST_PERCENT_GAP_ID)?;

    append(
        &start.test_file,
        &format!(
            "\n#[test]\nfn {}() {{\n    assert_eq!(discounted_total(100, 100), {RUST_EQUAL_CASE_EXPECTED});\n}}\n",
            start.test_name
        ),
    )?;
    exactly_one_passed(
        &run_printed(&start.run_line, &scratch)?,
        "printed test command",
    )?;
    let finished = both(&follow(&start.continue_line, &scratch)?);
    need(&finished, "weak -> exposed", "continue")?;

    let check = bare_check(&root)?;
    need(
        &check,
        "State: no test gap found",
        "recheck after the repair",
    )?;
    need(&check, RUST_PERCENT_GAP_ID, "recheck after the repair")
}

#[test]
fn rust_repair_with_a_test_off_the_boundary_does_not_close() -> Result<(), String> {
    let scratch = Scratch::new("rust-repair-neg")?;
    let root = scratch.repo();
    rust_repo(&root)?;
    let start = start_rust_repair(&scratch, RUST_GAP_ID)?;

    // Passes on both `>` and `>=`: it cannot tell them apart.
    append(
        &start.test_file,
        &format!(
            "\n#[test]\nfn {}() {{\n    assert_eq!(discounted_total(200, 100), 190);\n}}\n",
            start.test_name
        ),
    )?;
    let finished = both(&run_printed(&start.continue_line, &scratch)?);
    need(
        &finished,
        "weak -> weak",
        "continue with an off-boundary test",
    )?;
    refuse(
        &finished,
        "-> exposed",
        "continue with an off-boundary test",
    )?;

    let check = bare_check(&root)?;
    need(&check, "State: a test gap to inspect or repair", "recheck")?;
    need(
        &check,
        "Missing check: amount == discount_threshold",
        "recheck",
    )
}

// ---------------------------------------------------------------------------
// Journey: Python agent loop (check → agent repair → commit test → continue)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-python")]
fn python_repo(root: &Path) -> Result<(), String> {
    branch_repo(
        root,
        &[
            (
                "pyproject.toml",
                "[project]\nname = \"journey\"\nversion = \"0.1.0\"\n",
            ),
            (
                "src/pricing.py",
                "def discount(total):\n    if total > 100:\n        return total - 10\n    return total\n",
            ),
            (
                "tests/test_pricing.py",
                "from src.pricing import discount\n\n\ndef test_small():\n    assert discount(50) == 50\n\n\ndef test_large():\n    assert discount(1000) == 990\n",
            ),
        ],
        "src/pricing.py",
        "total > 100",
        "total >= 100",
    )
}

#[cfg(feature = "lang-python")]
const PYTHON_GAP_ID: &str =
    "gap:python:src/pricing.py:discount:predicate_boundary:predicate:total>=100";

/// Bare check → the printed repair start → its printed `continue` command.
#[cfg(feature = "lang-python")]
fn start_python_repair(scratch: &Scratch) -> Result<(PathBuf, String), String> {
    let root = scratch.repo();
    let check = bare_check(&root)?;
    need(&check, PYTHON_GAP_ID, "first check")?;
    need(&check, "Static exposure: weak", "first check")?;
    need(&check, "`total == 100`", "first check")?;
    let start_line = line_after(&check, "Recommended follow-through (start the repair):")
        .ok_or_else(|| format!("dead end: no repair start printed\n{check}"))?;
    if !start_line.starts_with("ripr agent repair ") {
        return Err(format!(
            "expected a `ripr agent repair` start, got {start_line:?}"
        ));
    }
    let started = both(&follow(&start_line, scratch)?);
    let edit = need_printed(&started, "edit one test file:", "agent repair")?;
    let file = edit
        .split_once(" (")
        .map_or(edit.as_str(), |(file, _)| file)
        .to_string();
    let continue_line = need_printed(&started, "Next, after the test edit:", "agent repair")?;
    Ok((root.join(file), continue_line))
}

#[cfg(feature = "lang-python")]
#[test]
fn python_agent_repair_commit_continue_reaches_exposed() -> Result<(), String> {
    let scratch = Scratch::new("python-repair")?;
    let root = scratch.repo();
    python_repo(&root)?;
    let (test_file, continue_line) = start_python_repair(&scratch)?;

    // The start says to add the test and commit it.
    append(
        &test_file,
        "\n\ndef test_at_threshold():\n    assert discount(100) == 90\n",
    )?;
    git(
        &root,
        &["commit", "-q", "-am", "test the threshold boundary"],
    )?;

    let finished = both(&follow(&continue_line, &scratch)?);
    need(&finished, "closed: weakly_exposed -> exposed", "continue")?;

    let check = bare_check(&root)?;
    need(&check, PYTHON_GAP_ID, "recheck")?;
    need(&check, "Static exposure: exposed", "recheck")
}

#[cfg(feature = "lang-python")]
#[test]
fn python_agent_repair_with_a_test_off_the_boundary_stays_retryable() -> Result<(), String> {
    let scratch = Scratch::new("python-repair-neg")?;
    let root = scratch.repo();
    python_repo(&root)?;
    let (test_file, continue_line) = start_python_repair(&scratch)?;

    append(
        &test_file,
        "\n\ndef test_more():\n    assert discount(500) == 490\n",
    )?;
    git(&root, &["commit", "-q", "-am", "an off-boundary test"])?;

    let refused = run_printed(&continue_line, &scratch)?;
    if refused.status.success() {
        return Err(describe("continue accepted an off-boundary test", &refused));
    }
    let text = both(&refused);
    need(&text, "disposition=retryable", "continue refusal")?;
    need(
        &text,
        "`total == 100`",
        "continue refusal names the missing case",
    )?;
    need(&bare_check(&root)?, "Static exposure: weak", "recheck")
}

// ---------------------------------------------------------------------------
// Journey: TypeScript by hand (check → explain → add test → recheck)
// ---------------------------------------------------------------------------

#[cfg(feature = "lang-typescript")]
fn typescript_repo(root: &Path) -> Result<(), String> {
    branch_repo(
        root,
        &[
            (
                "package.json",
                "{\"name\":\"journey\",\"version\":\"0.1.0\",\"private\":true}\n",
            ),
            (
                "src/pricing.ts",
                "export function discount(total: number): number {\n  if (total > 100) {\n    return total - 10;\n  }\n  return total;\n}\n",
            ),
            (
                "tests/pricing.test.ts",
                "import { discount } from \"../src/pricing\";\n\ntest(\"small\", () => {\n  expect(discount(50)).toBe(50);\n});\n\ntest(\"large\", () => {\n  expect(discount(1000)).toBe(990);\n});\n",
            ),
        ],
        "src/pricing.ts",
        "total > 100",
        "total >= 100",
    )
}

/// The first TypeScript check names the boundary and, with no repair route
/// for this shape, says to add the test by hand and rerun check. The
/// printed explain must run. Returns nothing to follow beyond that: the
/// by-hand advice is the journey.
#[cfg(feature = "lang-typescript")]
fn typescript_first_steps(scratch: &Scratch) -> Result<(), String> {
    let check = bare_check(&scratch.repo())?;
    need(
        &check,
        "Changed behavior: if (total >= 100) {",
        "first check",
    )?;
    need(&check, "Static exposure: weak", "first check")?;
    need(&check, "`total == 100`", "first check")?;
    need(
        &check,
        "add or strengthen a test by hand, then rerun `ripr check`",
        "first check",
    )?;
    let explain_line = line_after(&check, "(finding id, for explain/context)")
        .ok_or_else(|| format!("dead end: no explain follow-up\n{check}"))?;
    let explain = stdout(&follow(&explain_line, scratch)?);
    need(&explain, "src/pricing.ts", "printed explain")
}

#[cfg(feature = "lang-typescript")]
#[test]
fn typescript_check_explain_add_test_recheck_reaches_exposed() -> Result<(), String> {
    let scratch = Scratch::new("ts-hand")?;
    let root = scratch.repo();
    typescript_repo(&root)?;
    typescript_first_steps(&scratch)?;

    append(
        &root.join("tests/pricing.test.ts"),
        "\ntest(\"at threshold\", () => {\n  expect(discount(100)).toBe(90);\n});\n",
    )?;
    let check = bare_check(&root)?;
    need(&check, "Changed behavior: if (total >= 100) {", "recheck")?;
    need(&check, "Static exposure: exposed", "recheck")
}

#[cfg(feature = "lang-typescript")]
#[test]
fn typescript_test_off_the_boundary_leaves_the_gap_open() -> Result<(), String> {
    let scratch = Scratch::new("ts-hand-neg")?;
    let root = scratch.repo();
    typescript_repo(&root)?;
    typescript_first_steps(&scratch)?;

    append(
        &root.join("tests/pricing.test.ts"),
        "\ntest(\"well above\", () => {\n  expect(discount(500)).toBe(490);\n});\n",
    )?;
    let check = bare_check(&root)?;
    need(&check, "Static exposure: weak", "recheck")?;
    refuse(&check, "Static exposure: exposed", "recheck")
}

// ---------------------------------------------------------------------------
// Journey: docs/how-to/first-rust-test.md, executed from the markdown
// ---------------------------------------------------------------------------

const WALKTHROUGH: &str = include_str!("../../../docs/how-to/first-rust-test.md");

/// Fenced blocks of the walkthrough, with the line that precedes each fence
/// (skipping blank lines) and the fence's info string.
struct Fence {
    lead: String,
    info: String,
    body: String,
    section: String,
}

fn fences(markdown: &str) -> Vec<Fence> {
    let mut out = Vec::new();
    let mut section = String::new();
    let mut lead = String::new();
    let mut open: Option<(String, String, String)> = None;
    for line in markdown.lines() {
        if let Some((info, block_lead, mut body)) = open.take() {
            if line.trim_start().starts_with("```") {
                out.push(Fence {
                    lead: block_lead,
                    info,
                    body,
                    section: section.clone(),
                });
            } else {
                body.push_str(line);
                body.push('\n');
                open = Some((info, block_lead, body));
            }
            continue;
        }
        if let Some(info) = line.trim_start().strip_prefix("```") {
            open = Some((info.trim().to_string(), lead.clone(), String::new()));
        } else if let Some(heading) = line.strip_prefix("## ") {
            section = heading.trim().to_string();
        } else if !line.trim().is_empty() {
            lead = line.trim().to_string();
        }
    }
    out
}

fn fence_in<'a>(
    all: &'a [Fence],
    section_prefix: &str,
    info: &str,
    contains: &str,
) -> Result<&'a Fence, String> {
    all.iter()
        .find(|f| f.section.starts_with(section_prefix) && f.info == info && f.body.contains(contains))
        .ok_or_else(|| {
            format!(
                "first-rust-test.md: no `{info}` block containing {contains:?} in section {section_prefix:?}"
            )
        })
}

/// One literal command line from the walkthrough, run in the project root
/// with ripr and cargo resolved to the candidate binary and the nested cargo.
fn run_doc_line(line: &str, root: &Path, cargo_target: &Path) -> Result<Output, String> {
    let words = doc_words(line)?;
    let Some((program, args)) = words.split_first() else {
        return Err("empty walkthrough command".into());
    };
    let mut command = match program.as_str() {
        "ripr" => ripr_command(root),
        "cargo" => cargo_command(root, cargo_target),
        "git" => git_command(root),
        "mkdir" => {
            let mut mkdir = Command::new("mkdir");
            mkdir.current_dir(root);
            mkdir
        }
        other => {
            return Err(format!(
                "walkthrough runs unexpected program `{other}`: {line:?}"
            ));
        }
    };
    command.args(args);
    spawn(command, line)
}

fn run_doc_ok(line: &str, root: &Path, cargo_target: &Path) -> Result<Output, String> {
    succeed(
        &format!("first-rust-test.md `{line}`"),
        run_doc_line(line, root, cargo_target)?,
    )
}

/// The lines of a Bash block joined by `&&`, as separate commands.
fn bash_steps(body: &str) -> Vec<String> {
    body.replace("&&\n", "&&")
        .split("&&")
        .map(|step| step.trim().to_string())
        .filter(|step| !step.is_empty())
        .collect()
}

fn walkthrough_finding(json: &str) -> Result<(String, String), String> {
    let report: Value = serde_json::from_str(json).map_err(|e| format!("check JSON: {e}"))?;
    let findings = report["findings"]
        .as_array()
        .ok_or_else(|| format!("check JSON has no findings array: {json}"))?;
    let finding = findings
        .iter()
        .find(|f| {
            f["canonical_gap_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("gap:rust:src/lib.rs:discounted_total:"))
        })
        .ok_or_else(|| format!("no finding for src/lib.rs discounted_total: {json}"))?;
    Ok((
        finding["canonical_gap_id"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        finding["classification"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    ))
}

#[test]
fn first_rust_test_walkthrough_runs_as_written() -> Result<(), String> {
    let scratch = Scratch::new("walkthrough")?;
    let root = scratch.repo();
    let cargo_target = scratch.base.join("cargo-target");
    let blocks = fences(WALKTHROUGH);

    // 1. The page's Bash setup creates the directory, runs `git init` and makes
    // `src` and `tests`; the scratch directory stands in for `mkdir` + `cd`.
    let setup = fence_in(&blocks, "1.", "bash", "git init")?;
    let setup_steps = bash_steps(&setup.body);
    if setup_steps.get(2).map(String::as_str) != Some("git init")
        || setup_steps.get(3).map(String::as_str) != Some("mkdir src tests")
    {
        return Err(format!("walkthrough step 1 setup changed: {setup_steps:?}"));
    }
    for step in &setup_steps[2..] {
        run_doc_ok(step, &root, &cargo_target)?;
    }
    // The four project files, exactly as the page shows them.
    for name in ["Cargo.toml", ".gitignore", "src/lib.rs", "tests/pricing.rs"] {
        let lead = format!("**`{name}`**");
        let block = blocks
            .iter()
            .find(|f| f.lead == lead)
            .ok_or_else(|| format!("first-rust-test.md shows no block for {name}"))?;
        write(&root, name, &block.body)?;
    }
    let baseline_test = fence_in(&blocks, "1.", "text", "cargo test --test pricing")?;
    let tests_before = run_doc_line(baseline_test.body.trim(), &root, &cargo_target)?;
    let summary = stdout(&tests_before);
    if !tests_before.status.success()
        || !summary.contains("test result: ok. 2 passed; 0 failed; 0 ignored")
    {
        return Err(describe(
            "walkthrough baseline tests (want `2 passed`)",
            &tests_before,
        ));
    }
    let commit = fence_in(&blocks, "1.", "text", "git commit")?;
    for line in commit.body.lines().filter(|l| !l.trim().is_empty()) {
        run_doc_ok(line.trim(), &root, &cargo_target)?;
    }

    // 2. The one-character production change, from the page's diff block.
    let diff = fence_in(&blocks, "2.", "diff", "discount_threshold")?;
    let old = diff
        .body
        .lines()
        .find_map(|l| l.strip_prefix('-'))
        .ok_or("walkthrough diff has no removed line")?;
    let new = diff
        .body
        .lines()
        .find_map(|l| l.strip_prefix('+'))
        .ok_or("walkthrough diff has no added line")?;
    replace_once(&root.join("src/lib.rs"), old.trim(), new.trim())?;

    // 3. Save the production patch and analyze it (the Bash block), then the
    // JSON form the page uses to read the classification.
    let analyze = fence_in(&blocks, "3.", "bash", "production.diff")?;
    let steps = bash_steps(&analyze.body);
    for step in &steps {
        run_doc_ok(step, &root, &cargo_target)?;
    }
    let json_line = fence_in(&blocks, "3.", "text", "--json")?
        .body
        .trim()
        .to_string();
    let first = stdout(&run_doc_ok(&json_line, &root, &cargo_target)?);
    let (gap_id, before) = walkthrough_finding(&first)?;
    if before != "weakly_exposed" || !gap_id.ends_with("amount==discount_threshold") {
        return Err(format!(
            "walkthrough step 3 promises a weakly_exposed equality-boundary finding; got {before} for {gap_id}"
        ));
    }

    // 4. Append the equality test and run exactly it.
    let new_test = fence_in(&blocks, "4.", "rust", "equality_boundary_discounts")?;
    append(
        &root.join("tests/pricing.rs"),
        &format!("\n{}", new_test.body),
    )?;
    let focused = fence_in(&blocks, "4.", "text", "-- --exact")?
        .body
        .trim()
        .to_string();
    exactly_one_passed(
        &run_doc_line(&focused, &root, &cargo_target)?,
        "walkthrough focused test",
    )?;

    // The page's wrong-behavior control: back to `>`, the test fails.
    replace_once(&root.join("src/lib.rs"), new.trim(), old.trim())?;
    // The page says the test "should fail because it observes `100` where the
    // requirement expects `90`"; hold it to exactly that.
    need(
        WALKTHROUGH,
        "observes `100` where the\nrequirement expects `90`",
        "walkthrough control text",
    )?;
    exactly_one_failed_on(
        &run_doc_line(&focused, &root, &cargo_target)?,
        "100",
        "90",
        "walkthrough control on the old comparison",
    )?;
    replace_once(&root.join("src/lib.rs"), old.trim(), new.trim())?;

    // 5. The same analysis again: the same subject now reads exposed.
    let recheck = fence_in(&blocks, "5.", "text", "production.diff")?
        .body
        .trim()
        .to_string();
    if steps.last() != Some(&recheck) {
        return Err(format!(
            "walkthrough step 5 must rerun step 3's analysis; step 3 ran {:?}, step 5 runs {recheck:?}",
            steps.last()
        ));
    }
    run_doc_ok(&recheck, &root, &cargo_target)?;
    let second = stdout(&run_doc_ok(&json_line, &root, &cargo_target)?);
    let (gap_after, after) = walkthrough_finding(&second)?;
    if gap_after != gap_id || after != "exposed" {
        return Err(format!(
            "walkthrough step 5 promises {gap_id} moves to exposed; got {after} for {gap_after}"
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Harness self-checks
// ---------------------------------------------------------------------------

#[test]
fn doc_words_refuses_what_a_shell_would_reinterpret() -> Result<(), String> {
    assert_eq!(
        doc_words("ripr agent repair --gap-id 'gap:py:a>=1' --root \"/a b\" x\\ y")?,
        vec![
            "ripr",
            "agent",
            "repair",
            "--gap-id",
            "gap:py:a>=1",
            "--root",
            "/a b",
            "x y"
        ]
    );
    for refused in [
        "ripr check > out.txt",
        "ripr check && rm -rf x",
        "ripr 'unterminated",
        "git commit -m \"a\\b\"",
        "git commit -m \"$HOME\"",
    ] {
        if let Ok(words) = doc_words(refused) {
            return Err(format!("{refused:?} should be refused, split as {words:?}"));
        }
    }
    Ok(())
}

#[test]
fn exactly_one_passed_rejects_zero_selected_tests() -> Result<(), String> {
    let scratch = Scratch::new("zero-selected")?;
    let root = scratch.repo();
    write(
        &root,
        "Cargo.toml",
        "[package]\nname = \"zero\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write(&root, "src/lib.rs", "#[test]\nfn present() {}\n")?;
    let mut command = cargo_command(&root, &scratch.base.join("cargo-target"));
    command.args(["test", "--lib", "absent", "--", "--exact"]);
    let output = spawn(command, "cargo test absent")?;
    if !output.status.success() {
        return Err(describe("zero-selection control should exit 0", &output));
    }
    if exactly_one_passed(&output, "zero selected").is_ok() {
        return Err("a zero-test run was accepted as one passed test".into());
    }
    Ok(())
}
