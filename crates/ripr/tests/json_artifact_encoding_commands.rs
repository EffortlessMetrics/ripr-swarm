//! #15936: every command that reads a ripr JSON artifact by path reads the
//! two spellings Windows PowerShell 5.1 writes as the plain UTF-8 file.
//!
//! `ripr <command> --json > artifact.json` in Windows PowerShell 5.1 writes
//! UTF-16LE with a byte-order mark and CRLF line ends; `-Encoding utf8`
//! writes a UTF-8 mark. #16142 converted `baseline`, `first-pr`, `gate` and
//! `review-comments --check-output`. These cases cover the remaining readers,
//! one case per command, grouped by the artifact family they read:
//!
//! - gap decision ledgers: `rerun --gap`, `swarm queue`, `agent packet`,
//!   `plus --gap-ledger`, `check --gap-ledger`, `receipt check --ledger`,
//!   `review-comments --gap-ledger`;
//! - repo snapshots: `rerun --before`, `plus --repo-exposure-summary`,
//!   `outcome`, `agent verify`, `agent receipt --verify-json`;
//! - report inputs: `pr-review front-panel`, `pr-comments plan`,
//!   `pr-comments summary`,
//!   `assistant-loop health`, `feedback export`;
//! - review and agent records: `annotations --comments`, `receipt check
//!   --path`, `swarm ingest --result`.
//!
//! Each case runs the command on the UTF-8 file (the control, which must
//! reach its expected exit) and on each PowerShell spelling, and requires the
//! same exit and the same output. The negative control is a UTF-16 file cut
//! by one byte: every case must refuse it and name the encoding. Where a
//! command hashes the input file, the hash covers the file as written, so
//! that field must differ between spellings and is compared that way.

#![cfg(feature = "lang-rust")]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;

static FIXTURE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The refusal text `bounded_input::decode_artifact_text` gives for UTF-16
/// that does not decode exactly.
const UTF16_REFUSAL: &str = "UTF-16 byte-order mark";

/// Directory, relative to the fixture root, that holds every artifact. It is
/// under the ignored `target/`, so the fixture worktree stays clean, and it
/// stays under `--root` for the commands that require that.
const ARTIFACTS: &str = "target/enc";

/// The repo's `[env] TEMP=target` redirect puts fixtures inside the outer git
/// worktree; remove the root on scope exit, success or failure.
struct FixtureGuard {
    root: PathBuf,
}

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .env("RIPR_QUIET", "1")
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?} failed: {error}"))
}

/// Run `ripr`, require success, and return its stdout.
fn ripr_ok(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = ripr(root, args)?;
    if !output.status.success() {
        return Err(format!(
            "ripr {args:?} failed: {}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| format!("ripr {args:?} stdout: {error}"))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}

fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))
}

const WEAK_TEST: &str = "use pricing::discounted_total;\n\n#[test]\nfn far_above_threshold_discounts() {\n    assert_eq!(discounted_total(10_000, 100), 9_990);\n}\n";

const BOUNDARY_TEST: &str = "use pricing::discounted_total;\n\n#[test]\nfn far_above_threshold_discounts() {\n    assert_eq!(discounted_total(10_000, 100), 9_990);\n    assert_eq!(discounted_total(100, 100), 90);\n    assert_eq!(discounted_total(99, 100), 99);\n}\n";

/// A committed one-function package whose test misses the `>=` boundary,
/// with its repo-exposure snapshot and the gap ledger derived from it, all
/// written by ripr as UTF-8.
fn fixture() -> Result<PathBuf, String> {
    let call = FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "ripr-json-encoding-commands-{}-{call}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    for dir in ["src", "tests", ARTIFACTS] {
        fs::create_dir_all(root.join(dir)).map_err(|error| format!("mkdir {dir}: {error}"))?;
    }
    fixture_git_ok(&root, &["init", "-q", "--initial-branch=main"])?;
    fixture_git_ok(&root, &["config", "user.email", "encoding@t.st"])?;
    fixture_git_ok(&root, &["config", "user.name", "encoding"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
    write(
        &root.join("Cargo.toml"),
        b"[package]\nname = \"pricing\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    write(&root.join(".gitignore"), b"/target/\n")?;
    write(
        &root.join("src/lib.rs"),
        b"pub fn discounted_total(amount: i32, threshold: i32) -> i32 {\n    if amount >= threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n",
    )?;
    write(&root.join("tests/pricing.rs"), WEAK_TEST.as_bytes())?;
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "base"])?;

    let exposure = ripr_ok(
        &root,
        &[
            "check",
            "--root",
            ".",
            "--format",
            "repo-exposure-json",
            "--quiet",
        ],
    )?;
    write(
        &root.join(ARTIFACTS).join("before.json"),
        exposure.as_bytes(),
    )?;
    ripr_ok(
        &root,
        &[
            "reports",
            "gap-ledger",
            "--root",
            ".",
            "--repo-exposure",
            &format!("{ARTIFACTS}/before.json"),
            "--out",
            &format!("{ARTIFACTS}/ledger.json"),
            "--out-md",
            &format!("{ARTIFACTS}/ledger.md"),
        ],
    )?;
    Ok(root)
}

/// The first gap the fixture's ledger records.
fn first_gap(root: &Path) -> Result<String, String> {
    let ledger: Value =
        serde_json::from_str(&read_text(&root.join(ARTIFACTS).join("ledger.json"))?)
            .map_err(|error| format!("ledger is not JSON: {error}"))?;
    ledger
        .pointer("/records/0/canonical_gap_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("the fixture ledger records no gap: {ledger}"))
}

/// `text` as UTF-8 with a byte-order mark (`Out-File -Encoding utf8`).
fn utf8_with_mark(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend(text.as_bytes());
    bytes
}

/// `text` as UTF-16LE with a byte-order mark, line ends as given.
fn utf16le(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    bytes
}

/// What a Windows PowerShell 5.1 `>` redirect writes: UTF-16LE with a mark,
/// every line ended by CRLF.
fn powershell_redirect(text: &str) -> Vec<u8> {
    utf16le(&text.replace('\n', "\r\n"))
}

/// Drop the digits after each run-local stamp (`unix_ms:<n>`, the analyzer
/// build's `process:<pid>:<nanos>`) so two runs over the same input compare.
fn scrub(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = ["unix_ms:", "process:"]
        .iter()
        .filter_map(|marker| rest.find(marker).map(|index| (index, marker.len())))
        .min()
    {
        let (index, len) = at;
        out.push_str(&rest[..index + len]);
        rest = rest[index + len..].trim_start_matches(|c: char| c.is_ascii_digit() || c == ':');
    }
    out.push_str(rest);
    out
}

/// One command reading one artifact by path.
struct Case<'a> {
    name: &'a str,
    /// The UTF-8 artifact the command reads, relative to the fixture root.
    source: &'a str,
    /// The command line; `{in}` is replaced by the input path.
    args: Vec<String>,
    /// A file the command writes whose content is compared too.
    out: Option<&'a str>,
    /// The control's exit code.
    exit: i32,
    /// JSON pointers into stdout that hash the input file, each with the
    /// digest it must equal over the bytes written for that run. They are
    /// checked exactly and left out of the cross-spelling comparison.
    byte_hashes: &'a [ByteHash],
}

/// The exit, stdout and written file a run produced, scrubbed, plus the
/// values at the case's byte-hash pointers.
#[derive(Debug, PartialEq)]
struct Observed {
    exit: Option<i32>,
    stdout: String,
    written: Option<String>,
    hashes: Vec<Value>,
}

/// A named way to write an artifact's text to disk.
type Spelling = (&'static str, fn(&str) -> Vec<u8>);

/// A JSON pointer to a digest field and the digest it must hold.
type ByteHash = (&'static str, fn(&[u8]) -> String);

/// `rerun`'s `stable_input_hash`: FNV-1a 64 as 16 lowercase hex digits.
fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The provenance digest form `sha256:<hex>`.
fn sha256_prefixed(bytes: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

const INPUT: &str = "target/enc/input.json";

fn run_case(root: &Path, case: &Case<'_>, bytes: &[u8]) -> Result<(Observed, String), String> {
    write(&root.join(INPUT), bytes)?;
    if let Some(out) = case.out {
        let _ = fs::remove_file(root.join(out));
    }
    let args = case
        .args
        .iter()
        .map(|arg| arg.replace("{in}", INPUT))
        .collect::<Vec<_>>();
    let argv = args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = ripr(root, &argv)?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let mut hashes = Vec::new();
    let stdout = if case.byte_hashes.is_empty() {
        scrub(&stdout)
    } else {
        // A refusal prints no JSON document; it has no hashes to set aside.
        let Ok(mut document) = serde_json::from_str::<Value>(&stdout) else {
            return Ok(observed_refusal(
                case,
                output.status.code(),
                &stdout,
                &stderr,
            ));
        };
        for (pointer, _) in case.byte_hashes {
            // A refused run prints no such field; the comparison still
            // sees the difference through the rest of the output.
            hashes.push(
                document
                    .pointer_mut(pointer)
                    .map(Value::take)
                    .unwrap_or(Value::Null),
            );
        }
        scrub(&document.to_string())
    };
    let written = match case.out {
        Some(out) => fs::read_to_string(root.join(out))
            .ok()
            .map(|text| scrub(&text)),
        None => None,
    };
    // A report command that discloses an unreadable input inside the file
    // it writes says so there, not on stdout.
    let said = format!(
        "{stdout}\n{stderr}\n{}",
        written.as_deref().unwrap_or_default()
    );
    Ok((
        Observed {
            exit: output.status.code(),
            stdout,
            written,
            hashes,
        },
        said,
    ))
}

fn observed_refusal(
    case: &Case<'_>,
    exit: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> (Observed, String) {
    let stdout = scrub(stdout);
    let said = format!("{stdout}\n{stderr}");
    (
        Observed {
            exit,
            stdout,
            written: None,
            hashes: vec![Value::Null; case.byte_hashes.len()],
        },
        said,
    )
}

/// Each byte-hash field must equal the digest of the exact bytes on disk,
/// not of the decoded text.
fn hashes_cover_bytes(
    case: &Case<'_>,
    spelling: &str,
    observed: &Observed,
    bytes: &[u8],
    said: &str,
) -> Result<(), String> {
    for ((pointer, digest), got) in case.byte_hashes.iter().zip(&observed.hashes) {
        let expected = digest(bytes);
        if got.as_str() != Some(expected.as_str()) {
            return Err(format!(
                "{} [{spelling}]: {pointer} is {got}, expected {expected} over the bytes as written\n{said}",
                case.name
            ));
        }
    }
    Ok(())
}

/// Run every case on the UTF-8 control, on each named spelling and on the
/// truncated UTF-16 negative control; return every failure.
fn check_cases(root: &Path, cases: &[Case<'_>], spellings: &[Spelling]) -> Vec<String> {
    let mut failures = Vec::new();
    for case in cases {
        if let Err(error) = check_case(root, case, spellings) {
            failures.push(error);
        }
    }
    failures
}

fn check_case(root: &Path, case: &Case<'_>, spellings: &[Spelling]) -> Result<(), String> {
    let text = read_text(&root.join(case.source))?;
    let (control, said) = run_case(root, case, text.as_bytes())?;
    hashes_cover_bytes(case, "UTF-8", &control, text.as_bytes(), &said)?;
    if control.exit != Some(case.exit) {
        return Err(format!(
            "{}: the UTF-8 control exited {:?}, expected {}\n{said}",
            case.name, control.exit, case.exit
        ));
    }
    if case.out.is_some() && control.written.is_none() {
        return Err(format!(
            "{}: the UTF-8 control wrote no output file\n{said}",
            case.name
        ));
    }
    let mut failures = Vec::new();
    for (spelling, encode) in spellings {
        let bytes = encode(&text);
        let (observed, said) = run_case(root, case, &bytes)?;
        if let Err(error) = hashes_cover_bytes(case, spelling, &observed, &bytes, &said) {
            failures.push(error);
        }
        let comparable = Observed {
            hashes: control.hashes.clone(),
            ..observed
        };
        if comparable != control {
            failures.push(format!(
                "{} [{spelling}]: differs from the UTF-8 control\ncontrol: {control:?}\ngot: {comparable:?}\n{said}",
                case.name
            ));
        }
    }
    let mut truncated = powershell_redirect(&text);
    truncated.pop();
    let (_, said) = run_case(root, case, &truncated)?;
    if !said.contains(UTF16_REFUSAL) {
        failures.push(format!(
            "{}: a truncated UTF-16 file was not refused by name\n{said}",
            case.name
        ));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

const POWERSHELL_SPELLINGS: &[Spelling] = &[
    ("UTF-8 with a byte-order mark", utf8_with_mark),
    ("UTF-16LE redirect", powershell_redirect),
];

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_string()).collect()
}

fn finish(failures: Vec<String>) -> Result<(), String> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n\n"))
    }
}

#[test]
fn gap_ledger_readers_accept_powershell_encodings() -> Result<(), String> {
    let root = fixture()?;
    let _guard = FixtureGuard { root: root.clone() };
    let gap = first_gap(&root)?;
    ripr_ok(
        &root,
        &[
            "receipt",
            "write",
            "--gap",
            &gap,
            "--verify-command",
            "cargo test",
            "--status",
            "passed",
            "--out",
            &format!("{ARTIFACTS}/receipt.json"),
        ],
    )?;
    let receipt = format!("{ARTIFACTS}/receipt.json");
    let ledger = "target/enc/ledger.json";
    let cases = [
        Case {
            name: "rerun --gap-ledger",
            source: ledger,
            args: args(&[
                "rerun",
                "--root",
                ".",
                "--gap",
                &gap,
                "--gap-ledger",
                "{in}",
                "--json",
            ]),
            out: None,
            exit: 0,
            byte_hashes: &[("/cache/input_fingerprint/selector_ledger_hash", fnv1a64_hex)],
        },
        Case {
            name: "swarm queue --gap-ledger",
            source: ledger,
            args: args(&[
                "swarm",
                "queue",
                "--root",
                ".",
                "--gap-ledger",
                "{in}",
                "--format",
                "json",
            ]),
            out: None,
            exit: 0,
            byte_hashes: &[],
        },
        Case {
            name: "agent packet --gap-ledger",
            source: ledger,
            args: args(&[
                "agent",
                "packet",
                "--root",
                ".",
                "--gap-ledger",
                "{in}",
                "--gap-id",
                &gap,
                "--json",
            ]),
            out: None,
            exit: 0,
            byte_hashes: &[],
        },
        Case {
            name: "plus --gap-ledger",
            source: ledger,
            args: args(&["plus", "--gap-ledger", "{in}"]),
            out: None,
            exit: 0,
            byte_hashes: &[],
        },
        Case {
            name: "check --gap-ledger",
            source: ledger,
            args: args(&[
                "check",
                "--root",
                ".",
                "--format",
                "repo-badge-json",
                "--gap-ledger",
                "{in}",
            ]),
            out: None,
            exit: 0,
            byte_hashes: &[],
        },
        Case {
            name: "receipt check --ledger",
            source: ledger,
            args: args(&[
                "receipt", "check", "--path", &receipt, "--ledger", "{in}", "--json",
            ]),
            out: None,
            exit: 0,
            byte_hashes: &[],
        },
        Case {
            name: "review-comments --gap-ledger",
            source: ledger,
            args: args(&[
                "review-comments",
                "--root",
                ".",
                "--base",
                "HEAD",
                "--head",
                "HEAD",
                "--gap-ledger",
                "{in}",
                "--out",
                "target/enc/comments-out.json",
            ]),
            out: Some("target/enc/comments-out.json"),
            exit: 0,
            byte_hashes: &[],
        },
    ];
    finish(check_cases(&root, &cases, POWERSHELL_SPELLINGS))
}

/// A ledger keeps the path of the snapshot it was built from, and
/// `agent packet` and `swarm queue` re-read that snapshot to check it is
/// current before they assign a gap. A ledger built from a re-encoded
/// snapshot must assign exactly what the UTF-8 snapshot's ledger assigns.
/// (UTF-16 with CRLF would change the snapshot's committed text, so it is
/// not a spelling these readers can accept; see the snapshot test.)
#[test]
fn a_ledger_from_a_reencoded_snapshot_assigns_like_the_utf8_one() -> Result<(), String> {
    let root = fixture()?;
    let _guard = FixtureGuard { root: root.clone() };
    let gap = first_gap(&root)?;
    let text = read_text(&root.join(ARTIFACTS).join("before.json"))?;
    let consumers: [Vec<String>; 2] = [
        args(&[
            "agent",
            "packet",
            "--root",
            ".",
            "--gap-ledger",
            "target/enc/derived.json",
            "--gap-id",
            &gap,
            "--json",
        ]),
        args(&[
            "swarm",
            "queue",
            "--root",
            ".",
            "--gap-ledger",
            "target/enc/derived.json",
            "--format",
            "json",
        ]),
    ];
    let run_chain = |bytes: &[u8]| -> Result<Vec<(Option<i32>, String)>, String> {
        write(&root.join(ARTIFACTS).join("source.json"), bytes)?;
        ripr_ok(
            &root,
            &[
                "reports",
                "gap-ledger",
                "--root",
                ".",
                "--repo-exposure",
                "target/enc/source.json",
                "--out",
                "target/enc/derived.json",
                "--out-md",
                "target/enc/derived.md",
            ],
        )?;
        consumers
            .iter()
            .map(|argv| {
                let argv = argv.iter().map(String::as_str).collect::<Vec<_>>();
                let output = ripr(&root, &argv)?;
                Ok((
                    output.status.code(),
                    scrub(&format!(
                        "{}\n{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    )),
                ))
            })
            .collect()
    };
    let control = run_chain(text.as_bytes())?;
    if control.iter().any(|(exit, _)| *exit != Some(0)) {
        return Err(format!("the UTF-8 chain did not succeed: {control:?}"));
    }
    let mut failures = Vec::new();
    for (spelling, encode) in [
        (
            "UTF-8 with a byte-order mark",
            utf8_with_mark as fn(&str) -> Vec<u8>,
        ),
        ("UTF-16LE, LF line ends", utf16le),
    ] {
        let observed = run_chain(&encode(&text))?;
        if observed != control {
            failures.push(format!(
                "[{spelling}]: the consumers differ from the UTF-8 chain\ncontrol: {control:?}\ngot: {observed:?}"
            ));
        }
    }
    finish(failures)
}

#[test]
fn snapshot_readers_accept_powershell_encodings() -> Result<(), String> {
    let root = fixture()?;
    let _guard = FixtureGuard { root: root.clone() };
    let summary = ripr_ok(
        &root,
        &[
            "check",
            "--root",
            ".",
            "--format",
            "repo-exposure-summary-json",
            "--quiet",
        ],
    )?;
    write(
        &root.join(ARTIFACTS).join("summary.json"),
        summary.as_bytes(),
    )?;
    // Pin the boundary in a second commit, so a before/after pair moves.
    write(&root.join("tests/pricing.rs"), BOUNDARY_TEST.as_bytes())?;
    fixture_git_ok(&root, &["commit", "-q", "-am", "pin the boundary"])?;
    let after = ripr_ok(
        &root,
        &[
            "check",
            "--root",
            ".",
            "--format",
            "repo-exposure-json",
            "--quiet",
        ],
    )?;
    write(&root.join(ARTIFACTS).join("after.json"), after.as_bytes())?;
    let verify = ripr_ok(
        &root,
        &[
            "agent",
            "verify",
            "--root",
            ".",
            "--before",
            "target/enc/before.json",
            "--after",
            "target/enc/after.json",
            "--json",
        ],
    )?;
    write(&root.join(ARTIFACTS).join("verify.json"), verify.as_bytes())?;
    let verify: Value =
        serde_json::from_str(&verify).map_err(|error| format!("verify is not JSON: {error}"))?;
    let seam = verify
        .pointer("/changed_seams/0/seam_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("agent verify recorded no changed seam: {verify}"))?
        .to_string();

    let cases = [
        Case {
            name: "rerun --before",
            source: "target/enc/before.json",
            args: args(&[
                "rerun",
                "--root",
                ".",
                "--changed-test",
                "tests/pricing.rs",
                "--before",
                "{in}",
                "--json",
            ]),
            out: None,
            exit: 0,
            byte_hashes: &[],
        },
        Case {
            name: "plus --repo-exposure-summary",
            source: "target/enc/summary.json",
            args: args(&["plus", "--repo-exposure-summary", "{in}"]),
            out: None,
            exit: 0,
            byte_hashes: &[],
        },
        Case {
            name: "outcome --before",
            source: "target/enc/before.json",
            args: args(&[
                "outcome",
                "--before",
                "{in}",
                "--after",
                "target/enc/after.json",
                "--format",
                "json",
            ]),
            out: None,
            exit: 0,
            byte_hashes: &[],
        },
    ];
    let mut failures = check_cases(&root, &cases, POWERSHELL_SPELLINGS);

    // `agent verify` checks each snapshot's own sha256 content commitment
    // over its text, and `agent receipt` requires the verify JSON to be the
    // canonical text `agent verify` printed. A UTF-8 mark, or UTF-16 that
    // keeps the producer's LF line ends, decodes to that exact text and is
    // accepted. A `>` redirect also rewrites LF to CRLF, which changes the
    // text itself, so it is refused by that integrity check and not by the
    // encoding.
    let integrity_cases = [
        (
            Case {
                name: "agent verify --before",
                source: "target/enc/before.json",
                args: args(&[
                    "agent",
                    "verify",
                    "--root",
                    ".",
                    "--before",
                    "{in}",
                    "--after",
                    "target/enc/after.json",
                    "--json",
                ]),
                out: None,
                exit: 0,
                byte_hashes: &[],
            },
            "content commitment mismatch",
        ),
        (
            Case {
                name: "agent receipt --verify-json",
                source: "target/enc/verify.json",
                args: args(&[
                    "agent",
                    "receipt",
                    "--root",
                    ".",
                    "--verify-json",
                    "{in}",
                    "--seam-id",
                    &seam,
                    "--json",
                ]),
                out: None,
                exit: 0,
                byte_hashes: &[("/provenance/verify_artifact/sha256", sha256_prefixed)],
            },
            "not canonical output from ripr agent verify",
        ),
    ];
    for (case, crlf_refusal) in &integrity_cases {
        failures.extend(check_cases(
            &root,
            std::slice::from_ref(case),
            &[
                ("UTF-8 with a byte-order mark", utf8_with_mark),
                ("UTF-16LE, LF line ends", utf16le),
            ],
        ));
        let text = read_text(&root.join(case.source))?;
        let (observed, said) = run_case(&root, case, &powershell_redirect(&text))?;
        if observed.exit == Some(0) || !said.contains(crlf_refusal) {
            failures.push(format!(
                "{} [UTF-16LE redirect]: expected the CRLF rewrite to be refused with `{crlf_refusal}`\n{said}",
                case.name
            ));
        }
    }
    finish(failures)
}

#[test]
fn report_and_record_readers_accept_powershell_encodings() -> Result<(), String> {
    let root = fixture()?;
    let _guard = FixtureGuard { root: root.clone() };
    let gap = first_gap(&root)?;
    ripr_ok(
        &root,
        &[
            "review-comments",
            "--root",
            ".",
            "--base",
            "HEAD",
            "--head",
            "HEAD",
            "--gap-ledger",
            "target/enc/ledger.json",
            "--out",
            "target/enc/comments.json",
        ],
    )?;
    ripr_ok(
        &root,
        &[
            "assistant-loop",
            "proof",
            "--root",
            ".",
            "--pr-guidance",
            "target/enc/comments.json",
            "--out",
            "target/enc/proof.json",
            "--out-md",
            "target/enc/proof.md",
        ],
    )?;
    ripr_ok(
        &root,
        &[
            "receipt",
            "write",
            "--gap",
            &gap,
            "--verify-command",
            "cargo test",
            "--status",
            "passed",
            "--out",
            "target/enc/receipt.json",
        ],
    )?;
    write(
        &root.join(ARTIFACTS).join("agent-result.json"),
        br#"{
  "packet": {
    "gap_id": "gap:pricing",
    "canonical_gap_id": "rust:src/lib.rs:discounted_total:predicate_boundary:amount>=threshold",
    "allowed_files": ["tests/pricing.rs"],
    "forbidden_files": ["src/lib.rs"]
  },
  "attempt": {
    "status": "completed",
    "edited_files": ["tests/pricing.rs"],
    "verify": {"status": "passed", "exit_code": 0}
  },
  "receipt": {"provenance": {"movement": "resolved"}}
}
"#,
    )?;
    write(&root.join(ARTIFACTS).join("route-quality.json"), b"{}\n")?;

    let cases = [
        Case {
            name: "pr-review front-panel --pr-guidance",
            source: "target/enc/comments.json",
            args: args(&[
                "pr-review",
                "front-panel",
                "--root",
                ".",
                "--pr-guidance",
                "{in}",
                "--out",
                "target/enc/panel.json",
                "--out-md",
                "target/enc/panel.md",
            ]),
            out: Some("target/enc/panel.json"),
            exit: 0,
            byte_hashes: &[],
        },
        Case {
            name: "pr-comments plan --pr-guidance",
            source: "target/enc/comments.json",
            args: args(&[
                "pr-comments",
                "plan",
                "--mode",
                "plan",
                "--pr-guidance",
                "{in}",
                "--out",
                "target/enc/plan.json",
                "--out-md",
                "target/enc/plan.md",
            ]),
            out: Some("target/enc/plan.json"),
            exit: 0,
            byte_hashes: &[],
        },
        Case {
            name: "pr-comments summary --pr-guidance",
            source: "target/enc/comments.json",
            args: args(&[
                "pr-comments",
                "summary",
                "--root",
                ".",
                "--pull-request",
                "7",
                "--head-sha",
                "0123456789abcdef0123456789abcdef01234567",
                "--pr-guidance",
                "{in}",
                "--output-dir",
                "target/enc/summary",
            ]),
            out: Some("target/enc/summary/summary-request.json"),
            exit: 0,
            byte_hashes: &[],
        },
        Case {
            name: "assistant-loop health --proof",
            source: "target/enc/proof.json",
            args: args(&[
                "assistant-loop",
                "health",
                "--proof",
                "{in}",
                "--out",
                "target/enc/health.json",
                "--out-md",
                "target/enc/health.md",
            ]),
            out: Some("target/enc/health.json"),
            exit: 0,
            byte_hashes: &[],
        },
        Case {
            name: "feedback export --route-quality",
            source: "target/enc/route-quality.json",
            args: args(&[
                "feedback",
                "export",
                "--root",
                ".",
                "--route-quality",
                "{in}",
                "--json",
            ]),
            out: None,
            exit: 0,
            byte_hashes: &[],
        },
        Case {
            name: "annotations --comments",
            source: "target/enc/comments.json",
            args: args(&[
                "annotations",
                "--comments",
                "{in}",
                "--out",
                "target/enc/annotations.txt",
            ]),
            out: Some("target/enc/annotations.txt"),
            exit: 0,
            byte_hashes: &[],
        },
        Case {
            name: "receipt check --path",
            source: "target/enc/receipt.json",
            args: args(&["receipt", "check", "--path", "{in}", "--json"]),
            out: None,
            exit: 0,
            byte_hashes: &[],
        },
        Case {
            name: "swarm ingest --result",
            source: "target/enc/agent-result.json",
            args: args(&[
                "swarm", "ingest", "--root", ".", "--result", "{in}", "--format", "json",
            ]),
            out: None,
            exit: 0,
            byte_hashes: &[],
        },
    ];
    finish(check_cases(&root, &cases, POWERSHELL_SPELLINGS))
}
