//! Panic and hang freedom for hostile input (#11439).
//!
//! Each test mutates a small valid seed with byte-level edits that target the
//! lexical hazards parsers get wrong (quotes, comment and string openers,
//! deep bracket runs, invalid UTF-8) and drives the public `check_workspace`
//! entry point end to end on the result. An `Err` is a fine answer for
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
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
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
    let new_lines: Vec<&[u8]> = new.split(|byte| *byte == b'\n').collect();
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
        Ok(Self { root })
    }

    /// Runs one check on its own thread so a panic or a hang is reported as a
    /// finding with the case input, not as a stuck or aborted test binary.
    fn check_within_deadline(&self) -> Result<(), String> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let input = CheckInput {
            root: self.root.clone(),
            diff_file: Some(self.root.join("fuzz.diff")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            ..CheckInput::default()
        };
        let worker = std::thread::spawn(move || {
            let _ = sender.send(check_workspace(input).map(|_| ()));
        });
        match receiver.recv_timeout(CASE_DEADLINE) {
            // Ok or a refusal: both are acceptable answers to hostile input.
            Ok(_) => worker
                .join()
                .map_err(|_payload| "worker panicked after answering".to_string()),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                Err(format!("no answer within {CASE_DEADLINE:?}"))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                Err("check_workspace panicked (worker ended without an answer)".to_string())
            }
        }
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
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
        ("fuzz.diff", &diff),
    ])
    .map_err(TestCaseError::fail)?;
    workspace
        .check_within_deadline()
        .map_err(|error| TestCaseError::fail(format!("{error}\nsource bytes: {mutated:?}")))
}

const CARGO_TOML: &str =
    "[package]\nname = \"fuzz-subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n";

proptest! {
    #![proptest_config(config())]

    #[test]
    fn rust_sources_never_panic_or_hang(edits in edits()) {
        source_case(CARGO_TOML, "Cargo.toml", "src/lib.rs", RUST_SEED, RUST_EDITED, &edits)?;
    }

    #[test]
    fn python_sources_never_panic_or_hang(edits in edits()) {
        source_case(
            "[project]\nname = \"fuzz-subject\"\nversion = \"0.0.0\"\n",
            "pyproject.toml",
            "pkg/account.py",
            PYTHON_SEED,
            PYTHON_EDITED,
            &edits,
        )?;
    }

    #[test]
    fn typescript_sources_never_panic_or_hang(edits in edits()) {
        source_case(
            "{\"name\":\"fuzz-subject\",\"version\":\"0.0.0\"}",
            "package.json",
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
