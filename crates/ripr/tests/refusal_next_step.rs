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
//!    The scan covers all production code in a file (each `#[cfg(test)]` item
//!    is skipped, not everything after the first one) and reads a message from
//!    inside its own call, so it cannot borrow a neighbour's cue (#14163).
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

/// Length in bytes of a `'x'` or `'\x'` char literal at `at`, or 0 when the
/// quote there opens a lifetime instead.
fn char_literal_len(bytes: &[u8], at: usize) -> usize {
    if bytes.get(at) != Some(&b'\'') {
        return 0;
    }
    if bytes.get(at + 1) == Some(&b'\\') {
        // `'\n'`, `'\''`, `'\u{1f}'`: closes at the next quote after the escape.
        let mut end = at + 3;
        while end < bytes.len() && end < at + 12 {
            if bytes[end] == b'\'' {
                return end + 1 - at;
            }
            end += 1;
        }
        return 0;
    }
    // One char then the closing quote. Only a non-ASCII char is wider than a
    // byte, so `<'a,'b>` is two lifetimes, not a char literal.
    let widths = match bytes.get(at + 1) {
        Some(first) if !first.is_ascii() => 2..=4,
        _ => 1..=1,
    };
    for width in widths {
        if bytes.get(at + 1 + width) == Some(&b'\'') {
            return width + 2;
        }
    }
    0
}

/// End (exclusive) of the string literal whose opening `"` is at `at`. `hashes`
/// is `Some(n)` for a raw string `r#..#"` with `n` hashes.
fn string_end(bytes: &[u8], at: usize, hashes: Option<usize>) -> usize {
    let mut i = at + 1;
    while i < bytes.len() {
        match hashes {
            None => {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    return i + 1;
                }
            }
            Some(count) => {
                if bytes[i] == b'"'
                    && bytes.len() >= i + 1 + count
                    && bytes[i + 1..i + 1 + count].iter().all(|b| *b == b'#')
                {
                    return i + 1 + count;
                }
            }
        }
        i += 1;
    }
    bytes.len()
}

/// When a raw string opens at `at` (`r"`, `r#"`, `br##"`), its hash count and
/// the offset of its opening quote.
fn raw_string_open(bytes: &[u8], at: usize) -> Option<(usize, usize)> {
    if bytes.get(at) != Some(&b'r') {
        return None;
    }
    if at > 0
        && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_')
        && bytes[at - 1] != b'b'
    {
        return None;
    }
    let mut i = at + 1;
    while bytes.get(i) == Some(&b'#') {
        i += 1;
    }
    (bytes.get(i) == Some(&b'"')).then_some((i - at - 1, i))
}

/// One step of a lexical walk over Rust source: what sits at `at`, and where
/// the walk continues. Strings, chars and comments are skipped as units so a
/// brace, paren or quote inside them is never counted as code.
enum Lexeme {
    /// A string literal; the range is its content without the quotes.
    Str(usize, usize),
    /// A comment or char literal.
    Skip,
    /// One byte of code.
    Code(u8),
}

fn lex_at(bytes: &[u8], at: usize) -> (Lexeme, usize) {
    let byte = bytes[at];
    if byte == b'/' && bytes.get(at + 1) == Some(&b'/') {
        let mut end = at;
        while end < bytes.len() && bytes[end] != b'\n' {
            end += 1;
        }
        return (Lexeme::Skip, end);
    }
    if byte == b'/' && bytes.get(at + 1) == Some(&b'*') {
        // Rust block comments nest: `/* a /* b */ c */` is one comment.
        let mut depth = 1usize;
        let mut end = at + 2;
        while end < bytes.len() && depth > 0 {
            if bytes[end] == b'/' && bytes.get(end + 1) == Some(&b'*') {
                depth += 1;
                end += 2;
            } else if bytes[end] == b'*' && bytes.get(end + 1) == Some(&b'/') {
                depth -= 1;
                end += 2;
            } else {
                end += 1;
            }
        }
        return (Lexeme::Skip, end.min(bytes.len()));
    }
    if let Some((hashes, quote)) = raw_string_open(bytes, at) {
        let end = string_end(bytes, quote, Some(hashes));
        return (
            Lexeme::Str(quote + 1, end.saturating_sub(1 + hashes).max(quote + 1)),
            end,
        );
    }
    if byte == b'"' {
        let end = string_end(bytes, at, None);
        return (Lexeme::Str(at + 1, end.saturating_sub(1).max(at + 1)), end);
    }
    let char_len = char_literal_len(bytes, at);
    if char_len > 0 {
        return (Lexeme::Skip, at + char_len);
    }
    (Lexeme::Code(byte), at + 1)
}

/// The string literals inside the call whose opening paren ends just before
/// `start`, joined with spaces. The walk stops at the matching close paren, so
/// a message in expression position (`Err("..".to_string())` as a match arm or
/// tail expression) is judged on its own text and cannot borrow a cue from the
/// next statement. `window` bounds a call that never closes.
fn literals_from(source: &str, start: usize, window: usize) -> String {
    let bytes = source.as_bytes();
    let end = (start + window).min(bytes.len());
    let mut out = String::new();
    let mut depth = 1usize;
    let mut at = start;
    while at < end {
        let (lexeme, next) = lex_at(bytes, at);
        match lexeme {
            Lexeme::Str(from, to) => {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&String::from_utf8_lossy(&bytes[from..to.min(bytes.len())]));
            }
            Lexeme::Skip => {}
            Lexeme::Code(b'(') => depth += 1,
            Lexeme::Code(b')') => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Lexeme::Code(_) => {}
        }
        at = next;
    }
    out
}

/// For each byte of `source`, whether it is code (not inside a string, char
/// literal or comment).
fn code_positions(source: &str) -> Vec<bool> {
    let bytes = source.as_bytes();
    let mut in_code = vec![false; bytes.len()];
    let mut at = 0;
    while at < bytes.len() {
        let (lexeme, next) = lex_at(bytes, at);
        if matches!(lexeme, Lexeme::Code(_)) {
            in_code[at] = true;
        }
        at = next;
    }
    in_code
}

/// `source` with every `#[cfg(test)]` item blanked out: a `mod tests { .. }`
/// block, a test-only function, or a test-only `use ..;`. Production code
/// after such an item stays visible. Cutting the file at the first
/// `#[cfg(test)]` instead hid everything below a test-only import near the top
/// of a file (#14163).
fn without_test_items(source: &str) -> String {
    const MARKER: &str = "#[cfg(test)]";
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut copied = 0;
    let mut at = 0;
    while at < bytes.len() {
        let (lexeme, next) = lex_at(bytes, at);
        if !matches!(lexeme, Lexeme::Code(b'#')) || !source[at..].starts_with(MARKER) {
            at = next;
            continue;
        }
        // Walk to the end of the item: the first `;` outside every brace,
        // paren and bracket (so `[u8; 4]` in a signature does not end it), or
        // the close of the first top-level `{ .. }` block.
        let mut depth = 0usize;
        let mut nest = 0usize;
        let mut end = at + MARKER.len();
        let mut closed = false;
        while end < bytes.len() {
            let (lexeme, next) = lex_at(bytes, end);
            end = next;
            match lexeme {
                Lexeme::Code(b'(' | b'[') => nest += 1,
                Lexeme::Code(b')' | b']') => nest = nest.saturating_sub(1),
                Lexeme::Code(b'{') => depth += 1,
                Lexeme::Code(b'}') => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 && nest == 0 {
                        closed = true;
                        break;
                    }
                }
                Lexeme::Code(b';') if depth == 0 && nest == 0 => {
                    closed = true;
                    break;
                }
                _ => {}
            }
        }
        out.push_str(&source[copied..at]);
        if !closed {
            // An item that never closes: drop the rest, as the old cut did.
            copied = bytes.len();
            break;
        }
        copied = end;
        at = end;
    }
    out.push_str(&source[copied..]);
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
    let production = without_test_items(source);
    let body = production.as_str();
    let in_code = code_positions(body);
    let mut keys = BTreeSet::new();
    for trigger in TRIGGERS {
        let mut from = 0;
        while let Some(found) = body[from..].find(trigger) {
            let at = from + found;
            from = at + trigger.len();
            // `strip_prefix("Err(")` and a comment naming `map_err(` are text
            // about a trigger, not a trigger.
            if !in_code[at] {
                continue;
            }
            if !message_follows(trigger, &body[from..]) {
                continue;
            }
            let text = literals_from(body, from, 2000);
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

/// The directory that holds the files of `path`'s child modules:
/// `a/mod.rs`, `lib.rs` and `main.rs` own their own directory, `a/b.rs`
/// owns `a/b/`.
fn child_module_dir(path: &Path) -> PathBuf {
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default();
    if matches!(stem.as_str(), "mod" | "lib" | "main") {
        parent
    } else {
        parent.join(stem)
    }
}

/// Names of out-of-line child modules declared under `#[cfg(test)]`, such as
/// `#[cfg(test)] mod rows;` or `#[cfg(test)] #[allow(x)] pub(crate) mod rows;`.
/// A module with a `#[path]` attribute is left to the per-item cut.
fn test_only_child_modules(source: &str) -> Vec<String> {
    const MARKER: &str = "#[cfg(test)]";
    let in_code = code_positions(source);
    let mut names = Vec::new();
    let mut search = 0;
    while let Some(found) = source[search..].find(MARKER) {
        let at = search + found;
        search = at + MARKER.len();
        if !in_code[at] {
            continue;
        }
        let mut rest = source[search..].trim_start();
        // Skip further attributes on the same item.
        let mut has_path = false;
        while let Some(attr) = rest.strip_prefix("#[") {
            let Some(close) = attr.find(']') else {
                break;
            };
            has_path |= attr[..close].trim_start().starts_with("path");
            rest = attr[close + 1..].trim_start();
        }
        if has_path {
            continue;
        }
        if let Some(after) = rest.strip_prefix("pub") {
            rest = after.trim_start();
            if let Some(scoped) = rest.strip_prefix('(') {
                let Some(close) = scoped.find(')') else {
                    continue;
                };
                rest = scoped[close + 1..].trim_start();
            }
        }
        let Some(after) = rest.strip_prefix("mod ") else {
            continue;
        };
        let after = after.trim_start();
        let name: String = after
            .chars()
            .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
            .collect();
        if !name.is_empty() && after[name.len()..].trim_start().starts_with(';') {
            names.push(name);
        }
    }
    names
}

fn fragment_name(relative: &str) -> String {
    format!("{}.txt", relative.replace(['/', '\\'], "__"))
}

fn current_offenders() -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let src = crate_dir().join("src");
    let mut files = Vec::new();
    source_files(&src, &mut files)?;
    files.sort();
    let mut test_only = Vec::new();
    for path in &files {
        let source = std::fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let dir = child_module_dir(path);
        for name in test_only_child_modules(&source) {
            test_only.push(dir.join(&name));
            test_only.push(dir.join(format!("{name}.rs")));
        }
    }
    let mut by_file = BTreeMap::new();
    for path in files {
        // A file whose `mod` declaration is `#[cfg(test)]` is test-only as a
        // whole, and so is everything below its directory.
        if test_only.iter().any(|module| path.starts_with(module)) {
            continue;
        }
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
        args: &["check", "--staged"],
        exit: 2,
        reason: "`git diff --cached | ripr check --diff -`",
    },
    Case {
        args: &["check", "--cached"],
        exit: 2,
        reason: "source is still read from the working tree",
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
        reason: "names no gap",
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
    // An unresolvable pr-evidence revision names the flag that carried it. It
    // used to print `bad base/head revision` with a Debug-formatted git argv.
    Case {
        args: &["pr-evidence", "--base", "no-such-ref-xyz", "--head", "HEAD"],
        exit: 2,
        reason: "pr-evidence --base `no-such-ref-xyz` does not resolve to a commit",
    },
    Case {
        args: &["pr-evidence", "--base", "HEAD", "--head", "no-such-ref-xyz"],
        exit: 2,
        reason: "pr-evidence --head `no-such-ref-xyz` does not resolve to a commit",
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

/// Keys reported for a synthetic source file, without the `file|` prefix.
fn offender_messages(source: &str) -> Vec<String> {
    offenders_in("x.rs", source)
        .into_iter()
        .map(|key| key.trim_start_matches("x.rs|").to_string())
        .collect()
}

#[test]
fn scan_sees_production_code_below_a_test_only_item() {
    // #14163: the scan used to stop at the first `#[cfg(test)]`, so a
    // test-only import near the top of a file hid every message below it.
    let source = r#"
#[cfg(test)]
use crate::testing::helper;

fn parse() -> Result<(), String> {
    return Err("widget requires --size <n>".to_string());
}

#[cfg(test)]
mod tests {
    fn brace_in_text() -> Result<(), String> {
        let _open = '{';
        let _text = "}} not the end of the module {";
        Err("test-only message is not found".to_string())
    }
}

fn after_tests() -> Result<(), String> {
    Err("gadget is not a directory".to_string())
}
"#;
    assert_eq!(
        offender_messages(source),
        vec![
            "gadget is not a directory".to_string(),
            "widget requires --size <n>".to_string(),
        ]
    );
}

#[test]
fn scan_hides_a_test_only_item_whose_signature_holds_a_semicolon() {
    // A `;` inside `[u8; 4]` or a call's parens is not the end of the item,
    // so the test-only body below it stays hidden from the production scan.
    let source = r#"
#[cfg(test)]
fn helper(_bytes: [u8; 4]) -> Result<(), String> {
    Err("test-only helper message is not found".to_string())
}

#[cfg(test)]
const TABLE: [&str; 2] = [
    "test-only table entry",
    { let _x = 1; "test-only block entry" },
];

fn after() -> Result<(), String> {
    Err("gadget is not a directory".to_string())
}
"#;
    assert_eq!(
        offender_messages(source),
        vec!["gadget is not a directory".to_string()]
    );
}

#[test]
fn scan_treats_a_nested_block_comment_as_one_comment() {
    // Rust block comments nest, so the inner `*/` does not end the outer one.
    let source = r#"
/* outer /* inner */
fn retired() -> Result<(), String> {
    Err("commented-out message is not found".to_string())
}
*/
fn live() -> Result<(), String> {
    Err("gadget is not a directory".to_string())
}
"#;
    assert_eq!(
        offender_messages(source),
        vec!["gadget is not a directory".to_string()]
    );
}

#[test]
fn test_only_child_modules_names_out_of_line_test_modules() {
    let source = r##"
#[cfg(test)]
mod rows;
#[cfg(test)]
#[doc(hidden)]
pub(crate) mod helpers ;
#[cfg(test)]
mod inline_tests {}
#[cfg(test)]
#[path = "elsewhere.rs"]
mod moved;
mod production;
// #[cfg(test)] mod commented;
const NOTE: &str = "#[cfg(test)] mod quoted;";
"##;
    assert_eq!(
        test_only_child_modules(source),
        vec!["rows".to_string(), "helpers".to_string()]
    );
    assert_eq!(
        child_module_dir(Path::new("src/a/b.rs")),
        PathBuf::from("src/a/b")
    );
    assert_eq!(
        child_module_dir(Path::new("src/a/mod.rs")),
        PathBuf::from("src/a")
    );
    assert_eq!(
        child_module_dir(Path::new("src/lib.rs")),
        PathBuf::from("src")
    );
}

#[test]
fn scan_judges_an_expression_position_message_on_its_own_text() {
    // #14163: reading on to the next `;` let a message with no next step
    // borrow the cue of the statement after it.
    let source = r#"
fn pick(kind: u8) -> Result<u8, String> {
    match kind {
        0 => Err("first kind is not supported".to_string()),
        1 => Err("second kind is not supported; pass `--kind 2`".to_string()),
        other => Ok(other),
    }
}
"#;
    assert_eq!(
        offender_messages(source),
        vec!["first kind is not supported".to_string()]
    );
}

#[test]
fn scan_ignores_a_trigger_named_inside_a_string_or_comment() {
    let source = r#"
fn strip(text: &str) -> Option<&str> {
    // map_err("this comment is not found by the scan")
    text.strip_prefix("Err(").map(|missing_value_text| "")
}
"#;
    assert_eq!(offender_messages(source), Vec::<String>::new());
}

#[test]
fn scan_keeps_a_cued_message_and_a_lifetime_does_not_open_a_char() {
    let source = r#"
fn load<'a,'b>(left: &'a str, right: &'b str) -> Result<&'a str, String> {
    if left == right {
        return Err(format!("{left} is not a directory; pass `--root <dir>`"));
    }
    Err(format!("{right} is not readable and no route is named"))
}
"#;
    assert_eq!(
        offender_messages(source),
        vec!["{right} is not readable and no route is named".to_string()]
    );
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
