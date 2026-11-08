//! In-process panic and hang freedom for the diff, config and language
//! front-ends (#11439).
//!
//! `fuzz_panic_freedom` runs `ripr check` as a child process per case, which
//! contains stack-overflow aborts but costs tens of milliseconds of process
//! start per case. This suite drives the same CLI adapter in-process
//! ([`ripr::cli::run`]) on a worker thread with the CLI's 8 MiB stack, so
//! more mutated inputs fit in the same wall-clock budget (3,000 cases per
//! property ran in about five minutes on a debug build). Rust analysis still
//! starts `cargo metadata`, so the case deadline stays generous.
//! The mutated diff, `ripr.toml` and Rust, Python and TypeScript sources are
//! the proptest input. An `Err` is a fine answer for hostile input; a panic,
//! a pass that exceeds the case deadline, or an abort (which ends the test
//! binary and names the failing test) is the finding.
//!
//! The default case count is small so the required test run stays cheap. The
//! scheduled and manual `fuzz.yml` workflow raises it with `PROPTEST_CASES`.
//! Run longer locally with `PROPTEST_CASES=5000 cargo test -p ripr --test
//! fuzz_inprocess`.

use proptest::prelude::*;
use proptest::test_runner::Config;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const CASE_DEADLINE: Duration = Duration::from_mins(3);
const WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;
const MAX_INPUT_BYTES: usize = 200_000;
const DEFAULT_CASES: u32 = 24;

const CARGO_TOML: &str =
    "[package]\nname = \"fuzz-subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n";
#[cfg(feature = "lang-python")]
const PYPROJECT_TOML: &str = "[project]\nname = \"fuzz-subject\"\nversion = \"0.0.0\"\n";
#[cfg(feature = "lang-typescript")]
const PACKAGE_JSON: &str = "{\"name\":\"fuzz-subject\",\"version\":\"0.0.0\"}\n";

const RUST_OLD: &str = "pub fn price(amount: u64, years: u32) -> u64 {\n    if years >= 5 && amount > 100 {\n        amount - amount * 5 / 100\n    } else {\n        match amount { 0 => 0, _ => amount }\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn discounts() {\n        assert_eq!(price(200, 5), 190);\n    }\n}\n";
const RUST_NEW: &str = "pub fn price(amount: u64, years: u32) -> u64 {\n    if years >= 4 && amount >= 100 {\n        amount - amount * 6 / 100\n    } else {\n        match amount { 0 => 0, _ => amount }\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn discounts() {\n        assert_eq!(price(200, 5), 190);\n    }\n}\n";

#[cfg(feature = "lang-python")]
const PYTHON_OLD: &str = "def price(amount, years):\n    if years >= 5 and amount > 100:\n        return amount - amount * 5 // 100\n    return amount\n\n\ndef test_discounts():\n    assert price(200, 5) == 190\n";
#[cfg(feature = "lang-python")]
const PYTHON_NEW: &str = "def price(amount, years):\n    if years >= 4 and amount >= 100:\n        return amount - amount * 6 // 100\n    return amount\n\n\ndef test_discounts():\n    assert price(200, 5) == 190\n";

#[cfg(feature = "lang-typescript")]
const TS_OLD: &str = "export function price(amount: number, years: number): number {\n  if (years >= 5 && amount > 100) {\n    return amount - amount * 0.05;\n  }\n  return amount;\n}\n\ntest(\"discounts\", () => {\n  expect(price(200, 5)).toBe(190);\n});\n";
#[cfg(feature = "lang-typescript")]
const TS_NEW: &str = "export function price(amount: number, years: number): number {\n  if (years >= 4 && amount >= 100) {\n    return amount - amount * 0.06;\n  }\n  return amount;\n}\n\ntest(\"discounts\", () => {\n  expect(price(200, 5)).toBe(190);\n});\n";

const CONFIG_SEED_TAIL: &str = "\n[analysis]\ninclude_unchanged_tests = true\n\n[suppressions]\npaths = [\"target/**\", \"**/generated/**\"]\n\n[output]\nformat = \"human\"\n";

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
    b"b'",
    b"\xe2\x80\xae",
    b"f\"{",
    b"${",
    b"\"\"\"",
    b"'''",
    b"@@ -1,1 +1,1 @@",
    b"@@ -0,0 +4294967295,9 @@",
    b"@@ -18446744073709551615,2 +1,1 @@",
    b"\n+++ b/x\n",
    b"\n--- a/x\n",
    b"\ndiff --git a/x b/x\nrename from ",
    b"\nBinary files a/x and b/x differ\n",
    b"\n\\ No newline at end of file\n",
    b"[[",
    b"]]",
    b"\"\"\"",
    b"= ",
    b"1e999",
    b"-9223372036854775809",
];

/// One edit: `(kind, position, token, repeat)`, all reduced modulo the input.
type Edit = (u8, u32, u8, u16);

fn edits() -> impl Strategy<Value = Vec<Edit>> {
    prop::collection::vec((0u8..7, any::<u32>(), any::<u8>(), 0u16..1_500), 1..6)
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
                    .take(token.len() * (repeat as usize % 1_000 + 20))
                    .collect();
                data.splice(at..at, run);
            }
            _ => {
                let run = vec![b'('; repeat as usize % 3_000 + 50];
                data.splice(at..at, run);
            }
        }
        data.truncate(MAX_INPUT_BYTES);
    }
    data
}

/// Unified diff that replaces `old` with `new` as one hunk.
fn replace_diff(path: &str, old: &str, new: &[u8]) -> Vec<u8> {
    let mut new_lines: Vec<&[u8]> = new.split(|byte| *byte == b'\n').collect();
    // A trailing newline ends the last line; it does not start an empty added
    // line past the end of the file, which no real diff carries.
    if new_lines.len() > 1 && new_lines.last().is_some_and(|last| last.is_empty()) {
        new_lines.pop();
    }
    let mut diff = format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,{} +1,{} @@\n",
        old.lines().count(),
        new_lines.len()
    )
    .into_bytes();
    for line in old.lines() {
        diff.push(b'-');
        diff.extend_from_slice(line.as_bytes());
        diff.push(b'\n');
    }
    for line in new_lines {
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
            "ripr-fuzz-inprocess-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let workspace = Self { root };
        for (name, content) in files {
            let path = workspace.root.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {} failed: {error}", parent.display()))?;
            }
            std::fs::write(&path, content)
                .map_err(|error| format!("write {} failed: {error}", path.display()))?;
        }
        Ok(workspace)
    }

    /// The `ripr check` command line. Driving the CLI adapter in-process is
    /// what loads `ripr.toml` and enables the Python and TypeScript preview
    /// languages; the plain library entry point does neither.
    fn args(&self) -> Vec<String> {
        let root = self.root.display().to_string();
        let diff = self.root.join("fuzz.diff").display().to_string();
        ["ripr", "check", "--quiet", "--mode", "ready", "--root"]
            .iter()
            .map(|arg| (*arg).to_string())
            .chain([root, "--diff".to_string(), diff])
            .collect()
    }

    /// Runs one check on a worker thread. `Ok(())` covers both a result and a
    /// clean error; a panic or a missed deadline is `Err`. On a missed
    /// deadline the worker is left detached; it cannot keep the test binary
    /// alive, and the failing input bytes are already in the message.
    fn check_within_deadline(&self) -> Result<(), String> {
        let args = self.args();
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::Builder::new()
            .stack_size(WORKER_STACK_BYTES)
            .spawn(move || {
                let _ = sender.send(ripr::cli::run(args).map_err(|error| error.to_string()));
            })
            .map_err(|error| format!("spawn worker failed: {error}"))?;
        match receiver.recv_timeout(CASE_DEADLINE) {
            Ok(_) => {
                let _ = worker.join();
                Ok(())
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(format!(
                "check exceeded {}s (hang or runaway pass)",
                CASE_DEADLINE.as_secs()
            )),
            // The sender dropped without a result: the worker panicked.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => match worker.join() {
                Err(payload) => Err(format!(
                    "check panicked: {}",
                    panic_message(payload.as_ref())
                )),
                Ok(()) => Err("worker ended without a result".to_string()),
            },
        }
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".to_string())
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

/// `ripr.toml` enabling every language this build compiles in. Naming a
/// language the build lacks is a config error, so a restricted build would
/// otherwise refuse every case before any parser ran.
fn languages_toml() -> String {
    let mut enabled = vec!["\"rust\""];
    if cfg!(feature = "lang-python") {
        enabled.push("\"python\"");
    }
    if cfg!(feature = "lang-typescript") {
        enabled.push("\"typescript\"");
    }
    format!("[languages]\nenabled = [{}]\n", enabled.join(", "))
}

fn source_case(
    manifest: (&str, &str),
    path: &str,
    old: &str,
    new: &str,
    edits: &[Edit],
) -> Result<(), TestCaseError> {
    let mutated = apply(new.as_bytes(), edits);
    let diff = replace_diff(path, old, &mutated);
    let workspace = Workspace::create(&[
        (manifest.0, manifest.1.as_bytes()),
        (path, &mutated),
        ("ripr.toml", languages_toml().as_bytes()),
        ("fuzz.diff", &diff),
    ])
    .map_err(TestCaseError::fail)?;
    workspace
        .check_within_deadline()
        .map_err(|error| TestCaseError::fail(format!("{error}\nsource bytes: {mutated:?}")))
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn rust_sources_never_panic_or_hang(edits in edits()) {
        source_case(("Cargo.toml", CARGO_TOML), "src/lib.rs", RUST_OLD, RUST_NEW, &edits)?;
    }

    #[cfg(feature = "lang-python")]
    #[test]
    fn python_sources_never_panic_or_hang(edits in edits()) {
        source_case(("pyproject.toml", PYPROJECT_TOML), "pkg/account.py", PYTHON_OLD, PYTHON_NEW, &edits)?;
    }

    #[cfg(feature = "lang-typescript")]
    #[test]
    fn typescript_sources_never_panic_or_hang(edits in edits()) {
        source_case(("package.json", PACKAGE_JSON), "src/index.ts", TS_OLD, TS_NEW, &edits)?;
    }

    #[test]
    fn hostile_diffs_never_panic_or_hang(edits in edits()) {
        let clean = replace_diff("src/lib.rs", RUST_OLD, RUST_NEW.as_bytes());
        let diff = apply(&clean, &edits);
        let workspace = Workspace::create(&[
            ("Cargo.toml", CARGO_TOML.as_bytes()),
            ("src/lib.rs", RUST_NEW.as_bytes()),
            ("ripr.toml", languages_toml().as_bytes()),
            ("fuzz.diff", &diff),
        ]).map_err(TestCaseError::fail)?;
        workspace
            .check_within_deadline()
            .map_err(|error| TestCaseError::fail(format!("{error}\ndiff bytes: {diff:?}")))?;
    }

    #[test]
    fn hostile_config_never_panics_or_hangs(edits in edits()) {
        let config_seed = format!("{}{CONFIG_SEED_TAIL}", languages_toml());
        let config_bytes = apply(config_seed.as_bytes(), &edits);
        let diff = replace_diff("src/lib.rs", RUST_OLD, RUST_NEW.as_bytes());
        let workspace = Workspace::create(&[
            ("Cargo.toml", CARGO_TOML.as_bytes()),
            ("src/lib.rs", RUST_NEW.as_bytes()),
            ("ripr.toml", &config_bytes),
            ("fuzz.diff", &diff),
        ]).map_err(TestCaseError::fail)?;
        workspace
            .check_within_deadline()
            .map_err(|error| TestCaseError::fail(format!("{error}\nconfig bytes: {config_bytes:?}")))?;
    }
}

/// Each unmutated seed must reach its language front-end through the same
/// CLI path the properties use and produce at least one finding for that
/// language and file, and a broken config must be refused. Otherwise every
/// property above could pass by failing, or returning early, before any
/// parser ran, or without the config ever being read. Findings are read from
/// the `--write-artifact` file because the report itself goes to stdout.
#[test]
fn unmutated_seeds_reach_analysis_and_config_is_consumed() -> Result<(), String> {
    let cases: Vec<(&str, &str, &str, &str, &str, &str)> = vec![
        (
            "rust",
            "Cargo.toml",
            CARGO_TOML,
            "src/lib.rs",
            RUST_OLD,
            RUST_NEW,
        ),
        #[cfg(feature = "lang-python")]
        (
            "python",
            "pyproject.toml",
            PYPROJECT_TOML,
            "pkg/account.py",
            PYTHON_OLD,
            PYTHON_NEW,
        ),
        #[cfg(feature = "lang-typescript")]
        (
            "typescript",
            "package.json",
            PACKAGE_JSON,
            "src/index.ts",
            TS_OLD,
            TS_NEW,
        ),
    ];
    for (language, manifest, manifest_body, path, old, new) in cases {
        let diff = replace_diff(path, old, new.as_bytes());
        let workspace = Workspace::create(&[
            (manifest, manifest_body.as_bytes()),
            (path, new.as_bytes()),
            ("ripr.toml", languages_toml().as_bytes()),
            ("fuzz.diff", &diff),
        ])?;
        let artifact = workspace.root.join("artifact.json");
        let mut args = workspace.args();
        args.push("--write-artifact".to_string());
        args.push(artifact.display().to_string());
        ripr::cli::run(args).map_err(|error| format!("{path}: seed refused: {error}"))?;
        let text = std::fs::read_to_string(&artifact)
            .map_err(|error| format!("{path}: read check artifact failed: {error}"))?;
        let report: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| format!("{path}: check artifact is not JSON: {error}"))?;
        let findings = report["findings"].as_array().ok_or_else(|| {
            format!(
                "{path}: check artifact has no findings array (schema_version {})",
                report["schema_version"]
            )
        })?;
        let reached = findings.iter().any(|finding| {
            finding["language"].as_str() == Some(language)
                && finding["probe"]["location"]["file"]
                    .as_str()
                    .is_some_and(|file| file.replace('\\', "/").ends_with(path))
        });
        if !reached {
            return Err(format!(
                "{path}: seed produced no {language} finding for the changed file ({} findings)",
                findings.len()
            ));
        }
    }

    let diff = replace_diff("src/lib.rs", RUST_OLD, RUST_NEW.as_bytes());
    let workspace = Workspace::create(&[
        ("Cargo.toml", CARGO_TOML.as_bytes()),
        ("src/lib.rs", RUST_NEW.as_bytes()),
        ("ripr.toml", b"[languages\nenabled = ["),
        ("fuzz.diff", &diff),
    ])?;
    if ripr::cli::run(workspace.args()).is_ok() {
        return Err("a malformed ripr.toml was accepted".to_string());
    }
    Ok(())
}
