//! Documented `ripr` invocations stay valid against the shipped binary (#11567).
//!
//! The README and the user docs show well over a thousand `ripr ...`
//! invocations in shell fences and inline code. This test resolves each one
//! against the binary's own `ripr help --json` catalog: the command path must
//! exist, every `--flag` must be one that command's help documents, and a
//! literal value for a flag with a closed value set must be in that set. A
//! renamed command, a removed flag or a dropped format value then fails here
//! instead of reaching a reader as a command that does not run.
//!
//! Scope and limits: this checks spelling against the catalog, not behavior
//! or sample output. Commands whose catalog row has no help body carry no
//! flag inventory and are checked for the command path only. Placeholder
//! values (`PATH`, `<id>`, `human|json`) are not value-checked.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const REPO_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// Flags every command accepts; they are not repeated in each catalog row.
const GLOBAL_FLAGS: &[&str] = &["--help", "--verbose", "--version"];

/// Inline code that starts with `ripr` but is not an invocation of today's
/// binary. Each entry is `(file, exact span text, why)`.
const NOT_AN_INVOCATION: &[(&str, &str, &str)] = &[
    (
        "docs/OUTPUT_SCHEMA.md",
        "ripr analysis incomplete",
        "title of a GitHub annotation, not a command",
    ),
    (
        "docs/OUTPUT_SCHEMA.md",
        "ripr analysis limitations",
        "title of a GitHub annotation, not a command",
    ),
    (
        "docs/OUTPUT_SCHEMA.md",
        "ripr nothing analyzed",
        "title of a GitHub annotation, not a command",
    ),
    (
        "docs/BADGE_ADOPTION.md",
        "ripr badge verify",
        "roadmap item naming a possible future command",
    ),
    (
        "docs/BUN_UB_TYPESCRIPT_PREVIEW_RUNBOOK.md",
        "ripr check --profile",
        "the sentence says this flag does not exist",
    ),
    (
        "docs/IMPLEMENTATION_CAMPAIGNS.md",
        "ripr check --profile",
        "the sentence says this flag does not exist",
    ),
    (
        "docs/OUTPUT_SCHEMA.md",
        "ripr check --profile",
        "the sentence says this flag does not exist",
    ),
];

struct Flag {
    takes_value: bool,
    values: Vec<String>,
}

struct CommandRow {
    has_flag_inventory: bool,
    flags: BTreeMap<String, Flag>,
}

struct Catalog {
    commands: BTreeMap<String, CommandRow>,
}

fn ripr(args: &[&str]) -> Result<String, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .output()
        .map_err(|error| format!("failed to run ripr {args:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!("ripr {args:?} failed: {:?}", output.status));
    }
    String::from_utf8(output.stdout).map_err(|error| format!("non-utf8 output: {error}"))
}

fn catalog() -> Result<Catalog, String> {
    let document: serde_json::Value =
        serde_json::from_str(&ripr(&["help", "--json"])?).map_err(|error| error.to_string())?;
    let rows = document["commands"].as_array().ok_or("commands missing")?;
    let mut commands = BTreeMap::new();
    for row in rows {
        let path = row["path"].as_str().ok_or("command without a path")?;
        let mut flags = BTreeMap::new();
        for flag in row["flags"].as_array().ok_or("flags missing")? {
            let name = flag["name"].as_str().ok_or("flag without a name")?;
            let values = flag["values"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            flags.insert(
                name.to_string(),
                Flag {
                    takes_value: flag["takes_value"].as_bool().unwrap_or(false),
                    values,
                },
            );
        }
        commands.insert(
            path.to_string(),
            CommandRow {
                has_flag_inventory: row["flags_scope"] != "no_help_body",
                flags,
            },
        );
    }
    Ok(Catalog { commands })
}

fn is_word(token: &str) -> bool {
    let mut chars = token.chars();
    chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// `--name` or `--name=value`, split into the name and the inline value.
fn long_flag(token: &str) -> Option<(&str, Option<&str>)> {
    let (name, value) = match token.split_once('=') {
        Some((name, value)) => (name, Some(value)),
        None => (token, None),
    };
    let body = name.strip_prefix("--")?;
    is_word(body).then_some((name, value))
}

/// Cut an invocation at the first shell operator or trailing comment.
fn command_part(invocation: &str) -> &str {
    let mut end = invocation.len();
    for separator in [" | ", " > ", " >> ", " 2>", " && ", " || ", "; ", " # "] {
        if let Some(index) = invocation.find(separator) {
            end = end.min(index);
        }
    }
    invocation.get(..end).unwrap_or(invocation).trim()
}

struct Invocation {
    line: usize,
    text: String,
    inline: bool,
}

/// `ripr ...` invocations in shell fences (with `\` continuations joined) and
/// in inline code spans outside fences.
fn invocations(markdown: &str) -> Vec<Invocation> {
    let mut found = Vec::new();
    let mut fence: Option<bool> = None;
    let mut pending: Option<(usize, String)> = None;
    let mut wrapped: Option<(usize, String)> = None;
    for (index, raw) in markdown.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if let Some(info) = trimmed
            .strip_prefix("```")
            .or_else(|| trimmed.strip_prefix("~~~"))
        {
            fence = match fence {
                Some(_) => None,
                None => Some(matches!(
                    info.trim().to_ascii_lowercase().as_str(),
                    "bash" | "sh" | "shell" | "zsh" | "console" | "powershell" | "pwsh"
                )),
            };
            pending = None;
            wrapped = None;
            continue;
        }
        match fence {
            Some(true) => {
                let (start, mut text) = match pending.take() {
                    Some((start, text)) => (start, format!("{text} {trimmed}")),
                    None => (line, trimmed.trim_start_matches("$ ").to_string()),
                };
                if let Some(joined) = text.strip_suffix('\\') {
                    pending = Some((start, joined.trim_end().to_string()));
                    continue;
                }
                if text.starts_with("ripr ") {
                    text.truncate(command_part(&text).len());
                    found.push(Invocation {
                        line: start,
                        text,
                        inline: false,
                    });
                }
            }
            Some(false) => {}
            None => {
                // A span may wrap across lines; its text is joined until it
                // closes or the paragraph ends.
                if trimmed.is_empty() {
                    wrapped = None;
                    continue;
                }
                let segments: Vec<&str> = raw.split('`').collect();
                let mut inside = wrapped.is_some();
                for (position, segment) in segments.iter().enumerate() {
                    if position > 0 {
                        inside = !inside;
                    }
                    if !inside {
                        continue;
                    }
                    let (start, text) = match wrapped.take() {
                        Some((start, head)) => (start, format!("{head} {}", segment.trim())),
                        None => (line, (*segment).to_string()),
                    };
                    if position + 1 == segments.len() {
                        wrapped = Some((start, text));
                    } else if text.starts_with("ripr ") {
                        found.push(Invocation {
                            line: start,
                            text: command_part(&text).to_string(),
                            inline: true,
                        });
                    }
                }
            }
        }
    }
    found
}

#[derive(Default)]
struct Tally {
    invocations: usize,
    resolved: usize,
    flags_checked: usize,
    values_checked: usize,
    problems: Vec<String>,
    exceptions_used: BTreeSet<usize>,
}

fn check_markdown(catalog: &Catalog, file: &str, markdown: &str, tally: &mut Tally) {
    for invocation in invocations(markdown) {
        tally.invocations += 1;
        if let Some(index) = NOT_AN_INVOCATION
            .iter()
            .position(|(path, text, _)| *path == file && *text == invocation.text)
        {
            tally.exceptions_used.insert(index);
            continue;
        }
        let tokens: Vec<&str> = invocation.text.split_whitespace().skip(1).collect();
        let mut path = String::new();
        let mut consumed = 0;
        for token in &tokens {
            let candidate = if path.is_empty() {
                (*token).to_string()
            } else {
                format!("{path} {token}")
            };
            if !catalog.commands.contains_key(&candidate) {
                break;
            }
            path = candidate;
            consumed += 1;
        }
        let at = format!("{file}:{}: `{}`", invocation.line, invocation.text);
        let Some(row) = catalog.commands.get(&path) else {
            // Inline code is also used for prose such as `ripr reads ...`;
            // only a one- or two-word span, or one carrying a flag, reads as a
            // command.
            let first = tokens.first().copied().unwrap_or("");
            let command_like = !invocation.inline
                || tokens.len() <= 2
                || tokens.iter().any(|token| long_flag(token).is_some());
            if is_word(first) && command_like {
                tally
                    .problems
                    .push(format!("{at}: `{first}` is not a ripr command"));
            }
            continue;
        };
        tally.resolved += 1;
        if !row.has_flag_inventory {
            continue;
        }
        let rest = tokens.get(consumed..).unwrap_or(&[]);
        for (index, token) in rest.iter().enumerate() {
            let Some((name, inline_value)) = long_flag(token) else {
                continue;
            };
            if GLOBAL_FLAGS.contains(&name) {
                continue;
            }
            tally.flags_checked += 1;
            let Some(flag) = row.flags.get(name) else {
                tally.problems.push(format!(
                    "{at}: `ripr help {path}` documents no `{name}` flag"
                ));
                continue;
            };
            let value = inline_value.or_else(|| {
                flag.takes_value
                    .then(|| rest.get(index + 1).copied())
                    .flatten()
            });
            if let Some(value) = value
                && !flag.values.is_empty()
                && is_word(value)
            {
                tally.values_checked += 1;
                if !flag.values.iter().any(|allowed| allowed == value) {
                    tally.problems.push(format!(
                        "{at}: `{name} {value}` is not one of {:?}",
                        flag.values
                    ));
                }
            }
        }
    }
}

fn markdown_in(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("cannot list {}: {error}", directory.display()))?;
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.extension().is_some_and(|extension| extension == "md") {
            files.push(path);
        }
    }
    Ok(())
}

/// README plus the top-level, interop and status docs. `CLI_REFERENCE.md` is
/// generated from the binary by `cli_reference_doc.rs`, so it is not re-read.
fn documented_files() -> Result<Vec<(String, PathBuf)>, String> {
    let root = Path::new(REPO_ROOT);
    let mut files = vec![root.join("README.md")];
    for directory in ["docs", "docs/interop", "docs/status"] {
        markdown_in(&root.join(directory), &mut files)?;
    }
    files.sort();
    let mut named = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if relative != "docs/CLI_REFERENCE.md" {
            named.push((relative, path));
        }
    }
    Ok(named)
}

#[test]
fn documented_invocations_resolve_against_the_command_catalog() -> Result<(), String> {
    let catalog = catalog()?;
    let mut tally = Tally::default();
    for (name, path) in documented_files()? {
        let markdown = std::fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {name}: {error}"))?;
        check_markdown(&catalog, &name, &markdown, &mut tally);
    }
    assert!(
        tally.problems.is_empty(),
        "documented commands the binary does not accept:\n{}",
        tally.problems.join("\n")
    );
    // Guard against a vacuous pass: the scan must actually find and check
    // the bulk of the documented surface.
    assert!(
        tally.invocations >= 1000 && tally.resolved >= 1000,
        "only {} invocations found, {} resolved",
        tally.invocations,
        tally.resolved
    );
    assert!(
        tally.flags_checked >= 500 && tally.values_checked >= 50,
        "only {} flags and {} values were checked",
        tally.flags_checked,
        tally.values_checked
    );
    for (index, (file, text, _)) in NOT_AN_INVOCATION.iter().enumerate() {
        assert!(
            tally.exceptions_used.contains(&index),
            "stale exception: {file} no longer needs `{text}`"
        );
    }
    Ok(())
}

/// The checker must discriminate: a wrong command, flag or value is reported,
/// and the equivalent correct invocation is not.
#[test]
fn the_checker_reports_a_wrong_command_flag_and_value() -> Result<(), String> {
    let catalog = catalog()?;
    let markdown = "\
Run `ripr check --format json` or `ripr doctor --json`.

```bash
$ ripr check --base origin/main \\
    --format human-full | tee out.txt
ripr explain --diff change.diff some-finding
```

Then `ripr chekc`, `ripr check --no-such-flag` and `ripr check --format yaml`.
A span that wraps, such as `the badge that
ripr emits`, is prose and `ripr doctor` after it is still read, as is `ripr check
--bogus`.

```bash
ripr check \\
  --formta json
```

```text
ripr frobnicate --loudly
```
";
    let mut tally = Tally::default();
    check_markdown(&catalog, "sample.md", markdown, &mut tally);
    assert_eq!(tally.invocations, 10, "{:?}", tally.problems);
    let expected = [
        "sample.md:9: `ripr chekc`: `chekc` is not a ripr command",
        "sample.md:9: `ripr check --no-such-flag`: `ripr help check` documents no `--no-such-flag` flag",
        "sample.md:11: `ripr check --bogus`: `ripr help check` documents no `--bogus` flag",
        "sample.md:15: `ripr check --formta json`: `ripr help check` documents no `--formta` flag",
    ];
    for problem in expected {
        assert!(
            tally.problems.iter().any(|found| found == problem),
            "missing {problem:?} in {:?}",
            tally.problems
        );
    }
    assert!(
        tally
            .problems
            .iter()
            .any(|found| found
                .starts_with("sample.md:9: `ripr check --format yaml`: `--format yaml`")),
        "{:?}",
        tally.problems
    );
    assert_eq!(tally.problems.len(), 5, "{:?}", tally.problems);
    Ok(())
}

/// Every flag a command's `Usage:` line shows must be in that command's
/// catalog flag inventory, which is built from the option list below it. A
/// flag accepted and shown in usage but never described is invisible to
/// `ripr help --json` consumers and to the documented-invocation check above.
#[test]
fn usage_line_flags_are_in_the_catalog_inventory() -> Result<(), String> {
    let catalog = catalog()?;
    let mut checked = 0;
    let mut usage_flags = 0;
    let mut problems = Vec::new();
    for (path, row) in &catalog.commands {
        if !row.has_flag_inventory {
            continue;
        }
        let mut args = vec!["help"];
        args.extend(path.split(' '));
        let help = ripr(&args)?;
        let Some((_, after)) = help.split_once("Usage:") else {
            continue;
        };
        let usage = after.split("\n\n").next().unwrap_or("");
        checked += 1;
        for token in usage.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
            let Some((name, _)) = long_flag(token) else {
                continue;
            };
            usage_flags += 1;
            if !GLOBAL_FLAGS.contains(&name) && !row.flags.contains_key(name) {
                problems.push(format!(
                    "`ripr help {path}` shows `{name}` in Usage but does not describe it"
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(
        checked >= 40 && usage_flags >= 200,
        "only {checked} usage lines and {usage_flags} usage flags were checked"
    );
    Ok(())
}
