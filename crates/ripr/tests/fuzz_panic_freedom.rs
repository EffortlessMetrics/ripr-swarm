//! Panic and hang freedom for hostile input (#11439).
//!
//! Each test mutates a small valid seed with byte-level edits that target the
//! lexical hazards parsers get wrong (quotes, comment and string openers,
//! deep bracket runs, invalid UTF-8) and runs `ripr check --diff` as a child
//! process on the result (the CLI adapter enables the Python and TypeScript
//! preview languages, and a child contains a stack-overflow abort). An `Err` is a fine answer for
//! hostile input; a panic, an abort or a pass that exceeds the case deadline
//! is the finding. The mutation list is the proptest input, so a failure
//! shrinks to a short list of edits and prints the exact bytes.
//!
//! The default case count is small so the required test run stays cheap. The
//! scheduled and manual `fuzz.yml` workflow raises it with `PROPTEST_CASES`.
//! Run longer locally with `PROPTEST_CASES=2000 cargo test -p ripr --test
//! fuzz_panic_freedom`.

use proptest::prelude::*;
use proptest::test_runner::Config;
use ripr::ExposureClass;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// A pass over a source this small should take well under a second. Anything
/// near this deadline is a hang or a runaway pass, not a slow machine.
const CASE_DEADLINE: Duration = Duration::from_mins(1);
const MAX_INPUT_BYTES: usize = 400_000;
const DEFAULT_CASES: u32 = 12;

const RUST_SEED: &str = r##"use std::collections::BTreeMap;

/// Doc comment with a 'quote and a "string.
pub struct Account<'a, T: Clone + Default> where T: Send {
    name: &'a str,
    balance: T,
}

macro_rules! twice { ($e:expr) => { $e + $e }; }

impl<'a, T: Clone + Default> Account<'a, T> {
    pub fn price(&self, amount: u64, years: u32) -> u64 {
        let c = 'x';
        let r = r#"raw "quoted" text"#;
        let _ = (c, r, b'a', '\n', '\'');
        /* block /* nested */ comment */
        if years >= 5 && amount > 100 {
            amount - amount * 5 / 100
        } else {
            match amount { 0 => 0, 1..=9 => 1, _ => twice!(amount) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn five_years_discounts() {
        assert_eq!(Account::<u8> { name: "a", balance: 0 }.price(200, 5), 190);
    }
}
"##;

const RUST_EDITED: &str = r##"use std::collections::BTreeMap;

/// Doc comment with a 'quote and a "string.
pub struct Account<'a, T: Clone + Default> where T: Send {
    name: &'a str,
    balance: T,
}

macro_rules! twice { ($e:expr) => { $e + $e }; }

impl<'a, T: Clone + Default> Account<'a, T> {
    pub fn price(&self, amount: u64, years: u32) -> u64 {
        let c = 'x';
        let r = r#"raw "quoted" text"#;
        let _ = (c, r, b'a', '\n', '\'');
        /* block /* nested */ comment */
        if years >= 4 && amount >= 100 {
            amount - amount * 6 / 100
        } else {
            match amount { 0 => 0, 1..=9 => 2, _ => twice!(amount) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn five_years_discounts() {
        assert_eq!(Account::<u8> { name: "a", balance: 0 }.price(200, 5), 190);
    }
}
"##;

#[cfg(feature = "lang-python")]
const PYTHON_SEED: &str = r#"import re
from dataclasses import dataclass

@dataclass
class Account:
    name: str
    balance: int = 0

    def price(self, amount: int, years: int) -> int:
        label = f"{self.name!r:>10} {amount:,}"
        pattern = re.compile(r'(\d+)\s*"x"')
        if years >= 5 and amount > 100:
            return amount - amount * 5 // 100
        return sum(x for x in [amount, 1, 2] if x) + len(label + str(pattern))

async def grab(session, url):
    async with session.get(url) as response:
        return [line async for line in response if line.strip()]
"#;

#[cfg(feature = "lang-python")]
const PYTHON_EDITED: &str = r#"import re
from dataclasses import dataclass

@dataclass
class Account:
    name: str
    balance: int = 0

    def price(self, amount: int, years: int) -> int:
        label = f"{self.name!r:>10} {amount:,}"
        pattern = re.compile(r'(\d+)\s*"x"')
        if years >= 4 and amount >= 100:
            return amount - amount * 6 // 100
        return sum(x for x in [amount, 1, 2] if x) + len(label + str(pattern))

async def grab(session, url):
    async with session.get(url) as response:
        return [line async for line in response if line.strip()]
"#;

#[cfg(feature = "lang-typescript")]
const TS_SEED: &str = r#"interface Account { name: string; balance: number }

export function price(account: Account, years: number): number {
  const label = `${account.name} ${account.balance > 100 ? "rich" : 'poor'}`;
  const pattern = /(\d+)\s*["']x["']/g;
  if (years >= 5 && account.balance > 100) {
    return account.balance - account.balance * 0.05;
  }
  return [1, 2, 3].map((x) => x * 2).length + label.length + String(pattern).length;
}

export const load = async (url: string): Promise<string[]> => {
  const response = await grab(url);
  return (await response.text()).split("\n").filter(Boolean);
};
"#;

#[cfg(feature = "lang-typescript")]
const TS_EDITED: &str = r#"interface Account { name: string; balance: number }

export function price(account: Account, years: number): number {
  const label = `${account.name} ${account.balance > 100 ? "rich" : 'poor'}`;
  const pattern = /(\d+)\s*["']x["']/g;
  if (years >= 4 && account.balance >= 100) {
    return account.balance - account.balance * 0.06;
  }
  return [1, 2, 3].map((x) => x * 2).length + label.length + String(pattern).length;
}

export const load = async (url: string): Promise<string[]> => {
  const response = await grab(url);
  return (await response.text()).split("\n").filter(Boolean);
};
"#;

const CONFIG_SEED: &str = r#"[analysis]
include_unchanged_tests = true

[suppressions]
paths = ["target/**", "**/generated/**"]

[output]
format = "human"
"#;

/// Tokens that most often split a parser's idea of "inside a string".
const TOKENS: &[&[u8]] = &[
    b"{",
    b"}",
    b"(",
    b")",
    b"[",
    b"]",
    b"<",
    b">",
    b"\"",
    b"'",
    b"`",
    b"r#\"",
    b"\"#",
    b"/*",
    b"*/",
    b"//",
    b"\\",
    b"\n",
    b"\r\n",
    b"\xff\xfe",
    b"\0",
    b"macro_rules!",
    b"#[",
    b"::",
    b"=>",
    b"..=",
    b"|",
    b"async ",
    b"'a",
    b"b'",
    b"\xe2\x80\xae",
    b"f\"{",
    b"${",
    b"\"\"\"",
    b"'''",
    b"@@ -1,1 +1,1 @@",
    b"\n+++ b/x\n",
    b"\n--- a/x\n",
    b"\\ No newline at end of file\n",
];

/// One edit: `(kind, position, token, repeat)`, all reduced modulo the input.
type Edit = (u8, u32, u8, u16);

fn edits() -> impl Strategy<Value = Vec<Edit>> {
    prop::collection::vec((0u8..7, any::<u32>(), any::<u8>(), 0u16..2_500), 1..6)
}

fn apply(seed: &[u8], edits: &[Edit]) -> Vec<u8> {
    let mut data = seed.to_vec();
    for &(kind, position, token, repeat) in edits {
        if data.is_empty() {
            data.push(b'x');
        }
        let at = position as usize % data.len();
        let token = TOKENS[token as usize % TOKENS.len()];
        match kind {
            0 => data[at] = token[0],
            1 => {
                let end = (at + 1 + repeat as usize % 200).min(data.len());
                data.drain(at..end);
            }
            2 => {
                data.splice(at..at, token.iter().copied());
            }
            3 => {
                let end = (at + 1 + repeat as usize % 300).min(data.len());
                let copy = data[at..end].to_vec();
                data.splice(at..at, copy);
            }
            4 => data.truncate(at),
            5 => {
                let run: Vec<u8> = token
                    .iter()
                    .copied()
                    .cycle()
                    .take(token.len() * (repeat as usize % 2_000 + 50))
                    .collect();
                data.splice(at..at, run);
            }
            _ => {
                let run = vec![b'('; repeat as usize % 4_000 + 100];
                data.splice(at..at, run);
            }
        }
        data.truncate(MAX_INPUT_BYTES);
    }
    data
}

/// Unified diff that replaces every line of `old` with every line of `new`.
fn replace_diff(path: &str, old: &str, new: &[u8]) -> Vec<u8> {
    let old_lines: Vec<&str> = old.lines().collect();
    let mut new_lines: Vec<&[u8]> = new.split(|byte| *byte == b'\n').collect();
    // A trailing newline ends the last line; it does not start an empty added
    // line past the end of the file, which no real diff carries.
    if new_lines.len() > 1 && new_lines.last().is_some_and(|last| last.is_empty()) {
        new_lines.pop();
    }
    let mut diff = format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,{} +1,{} @@\n",
        old_lines.len(),
        new_lines.len()
    )
    .into_bytes();
    for line in &old_lines {
        diff.push(b'-');
        diff.extend_from_slice(line.as_bytes());
        diff.push(b'\n');
    }
    for line in &new_lines {
        diff.push(b'+');
        diff.extend_from_slice(line);
        diff.push(b'\n');
    }
    diff
}

struct Workspace {
    root: PathBuf,
    out: PathBuf,
}

impl Workspace {
    fn create(files: &[(&str, &[u8])]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-fuzz-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        for (name, bytes) in files {
            let path = root.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {} failed: {error}", parent.display()))?;
            }
            std::fs::write(&path, bytes)
                .map_err(|error| format!("write {} failed: {error}", path.display()))?;
        }
        let out = root.with_extension("out");
        std::fs::create_dir_all(&out)
            .map_err(|error| format!("create {} failed: {error}", out.display()))?;
        Ok(Self { root, out })
    }

    fn command(&self) -> std::process::Command {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"));
        command
            .args([
                "check", "--quiet", "--format", "json", "--mode", "ready", "--root",
            ])
            .arg(&self.root)
            .arg("--diff")
            .arg(self.root.join("fuzz.diff"));
        command
    }

    /// Runs one `ripr check` as a child process and returns its exit status
    /// and stdout. The CLI adapter is what enables the Python and TypeScript
    /// preview languages, and a child turns a stack overflow or abort into a
    /// reportable signal instead of killing the test binary. A signal, a Rust
    /// panic exit (101) or a missed deadline is a finding; any other refusal
    /// is an acceptable answer to hostile input. Stdout goes to a file so a
    /// large report cannot block the child while the deadline loop polls.
    fn run_once(&self, run: u8) -> Result<(std::process::ExitStatus, Vec<u8>), String> {
        let stdout_path = self.out.join(format!("stdout-{run}.json"));
        let stdout = std::fs::File::create(&stdout_path)
            .map_err(|error| format!("create {} failed: {error}", stdout_path.display()))?;
        let mut child = self
            .command()
            .stdout(stdout)
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| format!("spawn ripr failed: {error}"))?;
        let started = std::time::Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() >= CASE_DEADLINE => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("no answer within {CASE_DEADLINE:?}"));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(error) => return Err(format!("wait failed: {error}")),
            }
        };
        let bytes = std::fs::read(&stdout_path)
            .map_err(|error| format!("read {} failed: {error}", stdout_path.display()))?;
        Ok((status, bytes))
    }

    /// Crash-freedom (`judge`), then output invariants (`judge_output`), then
    /// determinism on a second run over the same bytes (#12924).
    fn check_within_deadline(&self) -> Result<(), String> {
        let (status, stdout) = self.run_once(0)?;
        judge(status)?;
        judge_output(status.code(), &stdout)?;
        if rerun_for_determinism(&self.root) {
            let (second_status, second_stdout) = self.run_once(1)?;
            judge_repeat(
                (status.code(), &stdout),
                (second_status.code(), &second_stdout),
            )?;
        }
        Ok(())
    }
}

/// Exit codes `ripr check` may use for a run that ended normally
/// (`docs/EXIT_CODES.md`): 0 completed, 2 could not complete, 3 declined,
/// 130 interrupted.
const DOCUMENTED_EXIT_CODES: [i32; 4] = [0, 2, 3, 130];

/// Words that would claim a runtime mutation outcome. `ripr check` is static
/// evidence only (CLAUDE.md product contract). The seeds and mutation tokens
/// contain none of them, so a hit is ripr's own output, not echoed input.
const RUNTIME_MUTATION_WORDS: [&str; 3] = ["killed", "survived", "mutant"];

/// The class vocabulary comes from the domain owner. The exhaustive match
/// makes a new `ExposureClass` variant a compile error here instead of a
/// silently stale list.
fn class_vocabulary() -> Vec<ExposureClass> {
    let all = vec![
        ExposureClass::Exposed,
        ExposureClass::WeaklyExposed,
        ExposureClass::ReachableUnrevealed,
        ExposureClass::NoStaticPath,
        ExposureClass::InfectionUnknown,
        ExposureClass::PropagationUnknown,
        ExposureClass::StaticUnknown,
    ];
    for class in &all {
        match class {
            ExposureClass::Exposed
            | ExposureClass::WeaklyExposed
            | ExposureClass::ReachableUnrevealed
            | ExposureClass::NoStaticPath
            | ExposureClass::InfectionUnknown
            | ExposureClass::PropagationUnknown
            | ExposureClass::StaticUnknown => {}
        }
    }
    all
}

/// Invariants on whatever a normally ending `ripr check --format json`
/// printed: documented exit code, and for a completed run valid JSON whose
/// finding classes are in the vocabulary (unknown classes carry a stop
/// reason) with no runtime-mutation claim.
fn judge_output(code: Option<i32>, stdout: &[u8]) -> Result<(), String> {
    let Some(code) = code else {
        return Err("ripr check ended without an exit code or signal".to_string());
    };
    if !DOCUMENTED_EXIT_CODES.contains(&code) {
        return Err(format!(
            "exit code {code} is not in docs/EXIT_CODES.md {DOCUMENTED_EXIT_CODES:?}"
        ));
    }
    if code != 0 {
        return Ok(());
    }
    let text =
        std::str::from_utf8(stdout).map_err(|error| format!("stdout is not UTF-8: {error}"))?;
    let report: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("stdout is not JSON: {error}"))?;
    for key in [
        "schema_version",
        "tool",
        "mode",
        "root",
        "summary",
        "findings",
    ] {
        if report.get(key).is_none() {
            return Err(format!("report is missing required key `{key}`"));
        }
    }
    let findings = report
        .get("findings")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "`findings` is not an array".to_string())?;
    let vocabulary = class_vocabulary();
    for (index, finding) in findings.iter().enumerate() {
        let label = finding
            .get("classification")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("finding {index} has no string classification"))?;
        let Some(class) = vocabulary.iter().find(|class| class.as_str() == label) else {
            return Err(format!(
                "finding {index} classification `{label}` is not in the exposure vocabulary"
            ));
        };
        if class.requires_stop_reason() {
            let has_reason = finding
                .get("stop_reasons")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|reasons| !reasons.is_empty());
            if !has_reason {
                return Err(format!(
                    "finding {index} is `{label}` but carries no stop reason"
                ));
            }
        }
    }
    let lowered = text.to_ascii_lowercase();
    for word in RUNTIME_MUTATION_WORDS {
        if lowered.contains(word) {
            return Err(format!(
                "output contains runtime-mutation term `{word}` (static-language boundary)"
            ));
        }
    }
    Ok(())
}

/// Two runs over the same bytes must agree on exit code and stdout.
fn judge_repeat(first: (Option<i32>, &[u8]), second: (Option<i32>, &[u8])) -> Result<(), String> {
    if first.0 != second.0 {
        return Err(format!(
            "nondeterministic exit code: {:?} then {:?}",
            first.0, second.0
        ));
    }
    if first.1 != second.1 {
        return Err(format!(
            "nondeterministic stdout: {} bytes then {} bytes",
            first.1.len(),
            second.1.len()
        ));
    }
    Ok(())
}

/// The determinism rerun doubles a case's child time. The weekly lane
/// (`PROPTEST_CASES` set) reruns every case; the default lane reruns one case
/// in eight, chosen from the workspace path hash so it stays cheap (#12924
/// asks for at most +20% default-lane runtime).
fn rerun_for_determinism(root: &std::path::Path) -> bool {
    if std::env::var_os("PROPTEST_CASES").is_some() {
        return true;
    }
    let mut hash: u32 = 2_166_136_261;
    for byte in root.to_string_lossy().bytes() {
        hash = (hash ^ u32::from(byte)).wrapping_mul(16_777_619);
    }
    hash.is_multiple_of(8)
}

fn judge(status: std::process::ExitStatus) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return Err(format!("ripr check died from signal {signal}"));
        }
    }
    #[cfg(windows)]
    {
        // Windows has no signals: a stack overflow or abort surfaces as an
        // NTSTATUS exit code (0xC00000FD stack overflow, 0xC0000409 fast-fail).
        if let Some(code) = status.code() {
            let code = u32::from_ne_bytes(code.to_ne_bytes());
            if code & 0xF000_0000 == 0xC000_0000 {
                return Err(format!("ripr check crashed with status {code:#x}"));
            }
        }
    }
    if status.code() == Some(101) {
        return Err("ripr check panicked (exit 101)".to_string());
    }
    Ok(())
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
        let _ = std::fs::remove_dir_all(&self.out);
    }
}

fn config() -> Config {
    Config {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_CASES),
        failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Off)),
        ..Config::default()
    }
}

fn source_case(
    manifest: &str,
    manifest_name: &str,
    ripr_toml: &str,
    path: &str,
    seed: &str,
    edited: &str,
    edits: &[Edit],
) -> Result<(), TestCaseError> {
    let mutated = apply(edited.as_bytes(), edits);
    let diff = replace_diff(path, seed, &mutated);
    let workspace = Workspace::create(&[
        (manifest_name, manifest.as_bytes()),
        (path, &mutated),
        ("ripr.toml", ripr_toml.as_bytes()),
        ("fuzz.diff", &diff),
    ])
    .map_err(TestCaseError::fail)?;
    workspace
        .check_within_deadline()
        .map_err(|error| TestCaseError::fail(format!("{error}\nsource bytes: {mutated:?}")))
}

// Preview languages analyze only when enabled; without this the Python and
// TypeScript suites would exercise nothing.
const RUST_TOML: &str = "[languages]\nenabled = [\"rust\"]\n";
#[cfg(feature = "lang-python")]
const PYTHON_TOML: &str = "[languages]\nenabled = [\"python\"]\n";
#[cfg(feature = "lang-typescript")]
const TS_TOML: &str = "[languages]\nenabled = [\"typescript\"]\n";

const CARGO_TOML: &str =
    "[package]\nname = \"fuzz-subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n";

proptest! {
    #![proptest_config(config())]

    #[test]
    fn rust_sources_never_panic_or_hang(edits in edits()) {
        source_case(CARGO_TOML, "Cargo.toml", RUST_TOML, "src/lib.rs", RUST_SEED, RUST_EDITED, &edits)?;
    }

    #[cfg(feature = "lang-python")]
    #[test]
    fn python_sources_never_panic_or_hang(edits in edits()) {
        source_case(
            "[project]\nname = \"fuzz-subject\"\nversion = \"0.0.0\"\n",
            "pyproject.toml",
            PYTHON_TOML,
            "pkg/account.py",
            PYTHON_SEED,
            PYTHON_EDITED,
            &edits,
        )?;
    }

    #[cfg(feature = "lang-typescript")]
    #[test]
    fn typescript_sources_never_panic_or_hang(edits in edits()) {
        source_case(
            "{\"name\":\"fuzz-subject\",\"version\":\"0.0.0\"}",
            "package.json",
            TS_TOML,
            "src/account.ts",
            TS_SEED,
            TS_EDITED,
            &edits,
        )?;
    }

    #[test]
    fn hostile_diffs_never_panic_or_hang(edits in edits()) {
        let clean = replace_diff("src/lib.rs", RUST_SEED, RUST_EDITED.as_bytes());
        let diff = apply(&clean, &edits);
        let workspace = Workspace::create(&[
            ("Cargo.toml", CARGO_TOML.as_bytes()),
            ("src/lib.rs", RUST_EDITED.as_bytes()),
            ("fuzz.diff", &diff),
        ])
        .map_err(TestCaseError::fail)?;
        workspace
            .check_within_deadline()
            .map_err(|error| TestCaseError::fail(format!("{error}\ndiff bytes: {diff:?}")))?;
    }

    #[test]
    fn hostile_config_never_panics_or_hangs(edits in edits()) {
        let config_bytes = apply(CONFIG_SEED.as_bytes(), &edits);
        let diff = replace_diff("src/lib.rs", RUST_SEED, RUST_EDITED.as_bytes());
        let workspace = Workspace::create(&[
            ("Cargo.toml", CARGO_TOML.as_bytes()),
            ("src/lib.rs", RUST_EDITED.as_bytes()),
            ("ripr.toml", &config_bytes),
            ("fuzz.diff", &diff),
        ])
        .map_err(TestCaseError::fail)?;
        workspace
            .check_within_deadline()
            .map_err(|error| TestCaseError::fail(format!("{error}\nconfig bytes: {config_bytes:?}")))?;
    }
}

/// The mutation operator must actually change the seed and stay bounded.
/// Without this, an inert `apply` would let the suites above pass while
/// testing the unmodified seed.
#[test]
fn mutations_change_the_input_and_stay_bounded() {
    let seed = RUST_EDITED.as_bytes();
    let deep = apply(seed, &[(6, 0, 0, 3_999)]);
    assert!(
        deep.len() > seed.len() + 4_000,
        "bracket run was not inserted"
    );
    assert_ne!(
        apply(seed, &[(0, 10, 9, 0)]),
        seed,
        "substitution changed nothing"
    );
    assert!(
        apply(seed, &[(4, 5, 0, 0)]).len() <= 5,
        "truncate kept the tail"
    );
    let huge = apply(
        seed,
        &[(5, 0, 0, 1_999), (5, 1, 1, 1_999), (5, 2, 2, 1_999)],
    );
    assert!(huge.len() <= MAX_INPUT_BYTES, "input exceeded the size cap");
}

/// Canary: every unmutated seed must analyze successfully with findings.
/// Without this, a permanent early refusal would let every suite above pass
/// without reaching analysis.
#[test]
fn unmutated_seeds_reach_analysis() -> Result<(), String> {
    let cases: Vec<(&str, &str, &str, &str, &str, &str)> = vec![
        (
            CARGO_TOML,
            "Cargo.toml",
            RUST_TOML,
            "src/lib.rs",
            RUST_SEED,
            RUST_EDITED,
        ),
        #[cfg(feature = "lang-python")]
        (
            "[project]\nname = \"fuzz-subject\"\nversion = \"0.0.0\"\n",
            "pyproject.toml",
            PYTHON_TOML,
            "pkg/account.py",
            PYTHON_SEED,
            PYTHON_EDITED,
        ),
        #[cfg(feature = "lang-typescript")]
        (
            "{\"name\":\"fuzz-subject\",\"version\":\"0.0.0\"}",
            "package.json",
            TS_TOML,
            "src/account.ts",
            TS_SEED,
            TS_EDITED,
        ),
    ];
    for (manifest, manifest_name, ripr_toml, path, seed, edited) in cases {
        let diff = replace_diff(path, seed, edited.as_bytes());
        let workspace = Workspace::create(&[
            (manifest_name, manifest.as_bytes()),
            (path, edited.as_bytes()),
            ("ripr.toml", ripr_toml.as_bytes()),
            ("fuzz.diff", &diff),
        ])?;
        let output = workspace
            .command()
            .output()
            .map_err(|error| format!("spawn ripr failed: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "{path}: baseline refused: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let report = String::from_utf8_lossy(&output.stdout);
        if !report.contains("\"findings\": [\n    {") {
            return Err(format!("{path}: baseline produced no findings"));
        }
    }
    Ok(())
}

fn good_report() -> String {
    r#"{"schema_version":"0.1","tool":"ripr","mode":"ready","root":".","summary":{},"findings":[{"classification":"weakly_exposed","stop_reasons":[]},{"classification":"static_unknown","stop_reasons":["opaque"]}]}"#
        .to_string()
}

/// The oracle must reject deliberately bad outputs (#12924), or the suites
/// above would pass whatever `ripr check` printed. Each case is one planted
/// fault against a known-good report, which must itself be accepted.
#[test]
fn output_oracle_rejects_planted_faults() {
    assert_eq!(judge_output(Some(0), good_report().as_bytes()), Ok(()));
    let good = good_report();
    let bad_cases: Vec<(&str, Option<i32>, String)> = vec![
        ("invalid json", Some(0), "{\"findings\": [".to_string()),
        ("missing key", Some(0), good.replace("\"summary\":{},", "")),
        (
            "unknown class",
            Some(0),
            good.replace("weakly_exposed", "mostly_exposed"),
        ),
        (
            "unknown class without stop reason",
            Some(0),
            good.replace("[\"opaque\"]", "[]"),
        ),
        (
            "runtime mutation word",
            Some(0),
            good.replace(
                "\"tool\":\"ripr\"",
                "\"tool\":\"ripr\",\"note\":\"mutant survived\"",
            ),
        ),
        ("undocumented exit code", Some(1), good.clone()),
        ("no exit code", None, good.clone()),
    ];
    for (name, code, body) in bad_cases {
        assert!(
            judge_output(code, body.as_bytes()).is_err(),
            "oracle accepted planted fault: {name}"
        );
    }
    assert!(judge_output(Some(0), b"\xff\xfe").is_err());
    assert_eq!(judge_output(Some(2), b"not json at all"), Ok(()));
}

#[test]
fn determinism_oracle_rejects_differing_runs() {
    let a = good_report();
    let b = a.replace("opaque", "opaqu3");
    assert_eq!(
        judge_repeat((Some(0), a.as_bytes()), (Some(0), a.as_bytes())),
        Ok(())
    );
    assert!(judge_repeat((Some(0), a.as_bytes()), (Some(0), b.as_bytes())).is_err());
    assert!(judge_repeat((Some(0), a.as_bytes()), (Some(2), a.as_bytes())).is_err());
}

/// The vocabulary the oracle checks against is the domain owner's, not a
/// second spelling.
#[test]
fn class_vocabulary_matches_domain_owner() {
    let vocabulary = class_vocabulary();
    let labels: Vec<&str> = vocabulary.iter().map(ExposureClass::as_str).collect();
    assert_eq!(labels.len(), 7);
    assert!(labels.contains(&"static_unknown"));
    let needing_reason = vocabulary
        .iter()
        .filter(|class| class.requires_stop_reason())
        .count();
    assert_eq!(needing_reason, 3);
}
