//! Refusal and error messages name a next step (project goal: "every refusal or
//! abstention says why and gives the next step").
//!
//! Two halves, both honest about what they can see:
//!
//! 1. **Source floor.** Every string literal that feeds a `CommandError`,
//!    `Err(..)` or `map_err(..)` and reads like an error (it names a failure
//!    word) must also carry an action cue (a command, a flag, or an imperative
//!    clause such as "pass ..." or "run ..."). The existing literals that do not
//!    are listed, one fragment per source file, under
//!    `policy/refusal-next-step.d/`. A new message without a cue fails; a
//!    fragment entry whose message now has a cue (or was deleted) also fails, so
//!    fixing a message means deleting its line. This is a presence floor, not a
//!    judgment that the cue is the right one; the live half and review own that.
//! 2. **Live floor.** A table of real bad invocations of the built binary. Each
//!    must exit with its documented code, print on stderr, and carry the cue.
//!
//! Blessing never adds: `RIPR_PRUNE_REFUSAL_BASELINE=1` rewrites fragments to
//! drop entries that no longer offend, and nothing else.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const FRAGMENT_DIR: &str = "policy/refusal-next-step.d";
const KEY_LEN: usize = 110;

/// Words that mark a literal as describing a failure or refusal.
const REASON_CUES: &[&str] = &[
    "failed",
    "cannot",
    "can't",
    "could not",
    "couldn't",
    "not found",
    "requires",
    "invalid",
    "unknown",
    "unsupported",
    "refus",
    "must ",
    "missing",
    "does not",
    "doesn't",
    "is not",
    "isn't",
    "no ",
    "unavailable",
    "declin",
    "unreadable",
    "too large",
    "exceed",
    "rejected",
    "malformed",
    "stale",
    "expected",
];

/// Imperative openers. They count only at the start of a clause (after `;`,
/// `: `, `. `, ` - `, `, `, `(` or at the start), so "check root ... is not a
/// directory" does not satisfy the cue by its first word.
const ACTION_OPENERS: &[&str] = &[
    "run ",
    "rerun",
    "re-run",
    "pass ",
    "use ",
    "try ",
    "set ",
    "add ",
    "check ",
    "see ",
    "next",
    "fix",
    "give ",
    "name ",
    "install",
    "create",
    "provide",
    "remove",
    "choose",
    "select",
    "retry",
    "edit",
    "open ",
    "enable",
    "inspect",
    "specify",
    "supply",
    "ensure",
    "make sure",
    "commit",
    "stage",
    "fetch",
    "update",
    "regenerate",
    "delete",
    "rename",
    "move ",
    "point ",
    "upgrade",
    "restart",
    "wait",
    "raise",
    "lower",
    "increase",
    "reduce",
    "unset",
    "export",
    "start ",
    "re-create",
    "recreate",
    "re-generate",
    "refresh",
    "replace",
    "pick ",
    "supply ",
    "adjust",
    "reduce",
    "split",
    "narrow",
    "build ",
    "clear",
    "close",
    "grant",
    "keep ",
    "continue",
    "resume",
    "repair",
    "repeat",
    "report",
    "ask ",
    "read ",
    "write ",
    "make ",
    "confirm",
    "verify",
    "wire",
    "bind",
    "re-bind",
    "rebind",
    "approve",
    "accept",
    "tell ",
    "merge",
    "rebase",
    "reset",
    "stop ",
    "skip",
    "drop ",
    "save ",
    "send ",
    "share",
    "attach",
    "treat ",
    "prefer ",
    "if ",
    "when ",
    "or ",
    "instead",
];

/// Anywhere-in-text cues that already name the route.
const ANYWHERE_CUES: &[&str] = &[
    "`ripr ",
    "ripr --help",
    "--help",
    "instead",
    "next step",
    "next:",
];

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    crate_dir().join("../..")
}

fn is_clause_start(prefix: &str) -> bool {
    let trimmed = prefix.trim_end_matches(' ');
    if trimmed.is_empty() {
        return true;
    }
    let had_space = trimmed.len() != prefix.len();
    match trimmed.chars().last() {
        Some(';') | Some(':') | Some('(') | Some(',') | Some('.') | Some('—') => true,
        Some('-') => had_space,
        _ => false,
    }
}

/// True when `text` (already lowercase) names an action the reader can take.
fn has_action_cue(text: &str) -> bool {
    if ANYWHERE_CUES.iter().any(|cue| text.contains(cue)) {
        return true;
    }
    for opener in ACTION_OPENERS {
        let mut from = 0;
        while let Some(found) = text[from..].find(opener) {
            let at = from + found;
            // A message opens with the failure, so a cue at offset 0 restates it.
            if at != 0 && is_clause_start(&text[..at]) {
                return true;
            }
            from = at + opener.len();
            if from >= text.len() {
                break;
            }
        }
    }
    false
}

fn has_reason_cue(text: &str) -> bool {
    REASON_CUES.iter().any(|cue| text.contains(cue))
}

/// String literals starting at `start`, stopping at the first `;` outside a
/// string or after `window` bytes.
fn literals_from(source: &str, start: usize, window: usize) -> String {
    let bytes = source.as_bytes();
    let end = (start + window).min(bytes.len());
    let mut out = String::new();
    let mut at = start;
    let mut in_string = false;
    let mut current = String::new();
    while at < end {
        let byte = bytes[at];
        if in_string {
            if byte == b'\\' && at + 1 < end {
                current.push('\\');
                current.push(bytes[at + 1] as char);
                at += 2;
                continue;
            }
            if byte == b'"' {
                in_string = false;
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&current);
                current.clear();
            } else {
                // Keep UTF-8 intact: push the char starting here.
                if let Some(ch) = source[at..].chars().next() {
                    current.push(ch);
                    at += ch.len_utf8();
                    continue;
                }
            }
        } else if byte == b'"' {
            in_string = true;
        } else if byte == b';' {
            break;
        }
        at += 1;
    }
    out
}

const TRIGGERS: &[&str] = &[
    "CommandError::Failure(",
    "CommandError::Decision(",
    "Err(",
    "map_err(",
    "ok_or_else(",
    "ok_or(",
];

/// Triggers that hold the message right after them. A bare `Err(` or `ok_or(`
/// only counts when what follows is a string or a formatted string, so
/// `Err(error)` forwarding does not pick up unrelated literals nearby.
fn message_follows(trigger: &str, after: &str) -> bool {
    let rest = after.trim_start();
    let rest = if trigger == "map_err(" || trigger == "ok_or_else(" {
        // closures: `|error| format!(..)`, `|| "..".to_string()`
        match rest.strip_prefix('|') {
            Some(inner) => match inner.find('|') {
                Some(close) => inner[close + 1..].trim_start(),
                None => return false,
            },
            None => return false,
        }
    } else {
        rest
    };
    rest.starts_with('"')
        || rest.starts_with("format!(")
        || rest.starts_with("String::from(")
        || rest.starts_with("CommandError::")
        || trigger.starts_with("CommandError")
}

/// The `(file, key)` pairs whose message reads like an error but has no cue.
fn offenders_in(relative: &str, source: &str) -> BTreeSet<String> {
    let body = match source.find("#[cfg(test)]") {
        Some(at) => &source[..at],
        None => source,
    };
    let mut keys = BTreeSet::new();
    for trigger in TRIGGERS {
        let mut from = 0;
        while let Some(found) = body[from..].find(trigger) {
            let at = from + found;
            from = at + trigger.len();
            if !message_follows(trigger, &body[from..]) {
                continue;
            }
            let text = literals_from(body, from, 600);
            if text.chars().count() < 12 {
                continue;
            }
            let lower = text.to_lowercase();
            if !has_reason_cue(&lower) || has_action_cue(&lower) {
                continue;
            }
            let key: String = text
                .chars()
                .take(KEY_LEN)
                .collect::<String>()
                .trim_end()
                .to_string();
            keys.insert(format!("{relative}|{}", key.replace('\n', " ")));
        }
    }
    keys
}

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|error| format!("read {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read entry in {}: {error}", dir.display()))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if name == "tests" || name == "testing" {
                continue;
            }
            source_files(&path, out)?;
        } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn fragment_name(relative: &str) -> String {
    format!("{}.txt", relative.replace(['/', '\\'], "__"))
}

fn current_offenders() -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let src = crate_dir().join("src");
    let mut files = Vec::new();
    source_files(&src, &mut files)?;
    files.sort();
    let mut by_file = BTreeMap::new();
    for path in files {
        let relative = format!(
            "crates/ripr/src/{}",
            path.strip_prefix(&src)
                .map_err(|error| format!("relative path: {error}"))?
                .to_string_lossy()
                .replace('\\', "/")
        );
        let bytes =
            std::fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        let source = String::from_utf8_lossy(&bytes).to_string();
        let keys = offenders_in(&relative, &source);
        if !keys.is_empty() {
            by_file.insert(relative, keys);
        }
    }
    Ok(by_file)
}

fn baseline() -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let dir = workspace_root().join(FRAGMENT_DIR);
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(out),
    };
    for entry in entries {
        let entry = entry.map_err(|error| format!("read {FRAGMENT_DIR}: {error}"))?;
        let text = std::fs::read_to_string(entry.path())
            .map_err(|error| format!("read {}: {error}", entry.path().display()))?;
        for line in text.lines() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((file, _)) = line.split_once('|') else {
                return Err(format!(
                    "{} has a line without `file|message`: {line}",
                    entry.path().display()
                ));
            };
            out.entry(file.to_string())
                .or_default()
                .insert(line.to_string());
        }
    }
    Ok(out)
}

#[test]
fn error_messages_name_a_next_step_or_are_baselined() -> Result<(), String> {
    let current = current_offenders()?;
    let accepted = baseline()?;
    let prune = std::env::var("RIPR_PRUNE_REFUSAL_BASELINE").is_ok_and(|value| value == "1");

    let mut added = Vec::new();
    let mut stale = Vec::new();
    for (file, keys) in &current {
        let known = accepted.get(file);
        for key in keys {
            if !known.is_some_and(|set| set.contains(key)) {
                added.push(key.clone());
            }
        }
    }
    for (file, keys) in &accepted {
        let now = current.get(file);
        for key in keys {
            if !now.is_some_and(|set| set.contains(key)) {
                stale.push(key.clone());
            }
        }
    }

    if prune && added.is_empty() {
        let dir = workspace_root().join(FRAGMENT_DIR);
        for (file, keys) in &accepted {
            let kept: Vec<&String> = keys
                .iter()
                .filter(|key| current.get(file).is_some_and(|set| set.contains(*key)))
                .collect();
            let path = dir.join(fragment_name(file));
            if kept.is_empty() {
                let _ = std::fs::remove_file(&path);
            } else {
                let body: String = kept.iter().map(|key| format!("{key}\n")).collect();
                std::fs::write(&path, body)
                    .map_err(|error| format!("write {}: {error}", path.display()))?;
            }
        }
        return Ok(());
    }

    if added.is_empty() && stale.is_empty() {
        return Ok(());
    }
    let mut report = String::new();
    if !added.is_empty() {
        report.push_str(
            "These error messages describe a failure but name no next step. Say what the\nreader should do (pass a path, run a command, set a flag), then re-run:\n",
        );
        for key in &added {
            report.push_str(&format!("  + {key}\n"));
        }
    }
    if !stale.is_empty() {
        report.push_str(&format!(
            "These baseline lines no longer offend (fixed or deleted). Remove them from\n{FRAGMENT_DIR}/, or run with RIPR_PRUNE_REFUSAL_BASELINE=1:\n"
        ));
        for key in &stale {
            report.push_str(&format!("  - {key}\n"));
        }
    }
    Err(report)
}

struct Case {
    args: &'static [&'static str],
    exit: i32,
    /// Distinctive text proving the refusal under test fired.
    reason: &'static str,
}

const LIVE_CASES: &[Case] = &[
    Case {
        args: &["check", "--diff", "/ripr-missing.patch"],
        exit: 2,
        reason: "failed to read diff file",
    },
    Case {
        args: &["bogus"],
        exit: 2,
        reason: "unknown command",
    },
    Case {
        args: &["check", "--root", "Cargo.toml"],
        exit: 2,
        reason: "is not a directory",
    },
    Case {
        args: &["check", "--base", "no-such-ref-xyz"],
        exit: 2,
        reason: "does not resolve",
    },
    Case {
        args: &["check", "--format", "nope"],
        exit: 2,
        reason: "unknown format",
    },
    Case {
        args: &["check", "--mode", "nope"],
        exit: 2,
        reason: "unknown mode",
    },
    Case {
        args: &["check", "--unknown-flag"],
        exit: 2,
        reason: "unknown check argument",
    },
    Case {
        args: &["gate", "evaluate"],
        exit: 2,
        reason: "requires at least one",
    },
    Case {
        args: &["init", "--ci", "nope"],
        exit: 2,
        reason: "unknown init --ci provider",
    },
    Case {
        args: &["pilot", "--root", "/ripr-missing-root"],
        exit: 2,
        reason: "is not a directory",
    },
    Case {
        args: &["repair", "nope"],
        exit: 3,
        reason: "names no seam",
    },
    Case {
        args: &["agent", "card", "--seam-id", "nope"],
        exit: 3,
        reason: "was not found",
    },
    Case {
        args: &["agent", "stub", "--seam-id", "nope"],
        exit: 3,
        reason: "was not found",
    },
    Case {
        args: &["agent", "packet", "--seam-id", "nope"],
        exit: 2,
        reason: "requires --json",
    },
    Case {
        args: &["agent", "verify"],
        exit: 2,
        reason: "requires --before",
    },
    Case {
        args: &["agent", "verify-execute"],
        exit: 2,
        reason: "requires --json",
    },
    Case {
        args: &["status", "--attempt", "repair-attempt-short"],
        exit: 2,
        reason: "suffix must contain",
    },
    Case {
        args: &[
            "agent",
            "receipt",
            "--seam-id",
            "x",
            "--verify-json",
            "v.json",
            "--json",
            "--attempt",
            "bogus",
        ],
        exit: 2,
        reason: "must start with",
    },
    Case {
        args: &["status", "--attempt", "nope"],
        exit: 2,
        reason: "must start with",
    },
];

fn scratch_repo(tag: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!(
        "ripr-refusal-next-step-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).map_err(|error| format!("create scratch: {error}"))?;
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"scratch\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .map_err(|error| format!("write manifest: {error}"))?;
    std::fs::write(
        dir.join("src/lib.rs"),
        "pub fn f(x: i32) -> i32 {\n    x + 1\n}\n",
    )
    .map_err(|error| format!("write lib: {error}"))?;
    for args in [
        vec!["init", "-q", "."],
        vec!["config", "user.email", "t@example.com"],
        vec!["config", "user.name", "t"],
        vec!["add", "-A"],
        vec!["commit", "-q", "-m", "init"],
    ] {
        let status = Command::new("git")
            .args(&args)
            .current_dir(&dir)
            .status()
            .map_err(|error| format!("git {args:?}: {error}"))?;
        if !status.success() {
            return Err(format!("git {args:?} failed in scratch repo"));
        }
    }
    Ok(dir)
}

#[test]
fn live_refusals_name_a_next_step() -> Result<(), String> {
    let repo = scratch_repo("live")?;
    let mut failures = Vec::new();
    for case in LIVE_CASES {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(case.args)
            .current_dir(&repo)
            .output()
            .map_err(|error| format!("run ripr {:?}: {error}", case.args))?;
        let label = format!("ripr {}", case.args.join(" "));
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let lower = stderr.to_lowercase();
        if output.status.code() != Some(case.exit) {
            failures.push(format!(
                "{label}: exit {:?}, wanted {}",
                output.status.code(),
                case.exit
            ));
            continue;
        }
        if !lower.contains(case.reason) {
            failures.push(format!(
                "{label}: stderr lost the reason `{}` (instrument check): {stderr}",
                case.reason
            ));
            continue;
        }
        // The cue is judged on the refusal itself, not on progress lines.
        let refusal: String = stderr
            .lines()
            .filter(|line| !line.starts_with("ripr progress:"))
            .collect::<Vec<_>>()
            .join("\n")
            .to_lowercase();
        if !has_action_cue(&refusal) {
            failures.push(format!("{label}: no next step in: {stderr}"));
        }
    }
    let _ = std::fs::remove_dir_all(&repo);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn action_cue_requires_a_clause_start_not_a_stray_word() {
    // The cue must not be satisfied by a failure sentence that merely begins
    // with a cue-shaped word.
    assert!(!has_action_cue("check root cargo.toml is not a directory"));
    assert!(!has_action_cue("if the root is missing, nothing runs"));
    assert!(has_action_cue(
        "root is not a directory; pass the directory that contains it"
    ));
    assert!(has_action_cue(
        "no finding matched x: run `ripr check` to list them"
    ));
    assert!(has_action_cue("base does not resolve. check the spelling"));
}

/// Structured remedy (#11790): the machine-readable refusal documents carry a
/// `remedy` derived from the same prose the guard above checks. Each case runs
/// the built binary on a real bad invocation and asserts the remedy is
/// grounded in the document's own message: a stated command is a verbatim
/// substring of it, and `none` carries no command, tool or flags.
struct JsonCase {
    args: &'static [&'static str],
    exit: i32,
    /// JSON pointer of the refusal prose and of its remedy in the document.
    message_pointer: &'static str,
    remedy_pointer: &'static str,
}

const JSON_CASES: &[JsonCase] = &[
    JsonCase {
        args: &["check", "--base", "no-such-ref-xyz", "--format", "json"],
        exit: 2,
        message_pointer: "/run_limitations/0/message",
        remedy_pointer: "/run_limitations/0/remedy",
    },
    JsonCase {
        args: &["agent", "card", "--seam-id", "nope", "--json"],
        exit: 3,
        message_pointer: "/error/message",
        remedy_pointer: "/error/remedy",
    },
    JsonCase {
        args: &["agent", "stub", "--seam-id", "nope", "--json"],
        exit: 3,
        message_pointer: "/refusal/reason",
        remedy_pointer: "/refusal/remedy",
    },
];

#[test]
fn json_refusals_carry_a_grounded_structured_remedy() -> Result<(), String> {
    let repo = scratch_repo("json")?;
    let mut failures = Vec::new();
    for case in JSON_CASES {
        let label = format!("ripr {}", case.args.join(" "));
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(case.args)
            .current_dir(&repo)
            .output()
            .map_err(|error| format!("run {label}: {error}"))?;
        if output.status.code() != Some(case.exit) {
            failures.push(format!(
                "{label}: exit {:?}, wanted {}",
                output.status.code(),
                case.exit
            ));
            continue;
        }
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let document: serde_json::Value = match serde_json::from_str(&stdout) {
            Ok(document) => document,
            Err(error) => {
                failures.push(format!("{label}: stdout is not JSON ({error}): {stdout}"));
                continue;
            }
        };
        let (Some(message), Some(remedy)) = (
            document
                .pointer(case.message_pointer)
                .and_then(|value| value.as_str()),
            document.pointer(case.remedy_pointer),
        ) else {
            failures.push(format!(
                "{label}: missing {} or {}: {stdout}",
                case.message_pointer, case.remedy_pointer
            ));
            continue;
        };
        let kind = remedy.get("kind").and_then(|value| value.as_str());
        let command = remedy.get("command").and_then(|value| value.as_str());
        let tool = remedy.get("tool").and_then(|value| value.as_str());
        let flags = remedy
            .get("flags")
            .and_then(|value| value.as_array())
            .map(Vec::len);
        let grounded = match kind {
            Some("command") => command.is_some_and(|command| message.contains(command)),
            Some("mcp_tool") => tool.is_some_and(|tool| message.contains(tool)),
            Some("flags") => flags.is_some_and(|count| count > 0),
            Some("none") => command.is_none() && tool.is_none() && flags == Some(0),
            _ => false,
        };
        let has_fields = [
            "alternatives",
            "fill_in",
            "retry_stated",
            "env",
            "conditional",
            "prose_has_more",
        ]
        .iter()
        .all(|field| remedy.get(*field).is_some());
        if !grounded || !has_fields {
            failures.push(format!(
                "{label}: ungrounded remedy {remedy} for: {message}"
            ));
        }
    }
    let _ = std::fs::remove_dir_all(&repo);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}
