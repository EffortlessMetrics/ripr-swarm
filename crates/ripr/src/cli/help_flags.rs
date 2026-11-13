//! Per-command flag inventory for the machine catalog (`ripr help --json`,
//! #11599).
//!
//! The flags a command accepts are documented once, in the help body it
//! prints. This module projects that body into typed rows instead of keeping
//! a second list that could drift from the text: an agent no longer has to
//! scrape `ripr check --help --all` to learn that `--path` repeats or that
//! `--format` takes a closed set. Enumerated `values` come from the typed
//! owners (`OutputFormat`, the pilot format set), never from prose, and an
//! empty `values` means "not a closed set the catalog can state".
//!
//! Subcommand groups that document several subcommands in one shared body
//! (`policy`, `reports`, `baseline`, ...) are projected through
//! `help_leaf::leaf_help`, so each leaf row lists only its own usage line and
//! options section and is marked `flags_scope: "leaf_section"` (#14314). A
//! shared-body path with no section mapping would still be marked
//! `shared_help_body` and list the union.

use serde::Serialize;

/// One flag of one command.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlagJson {
    /// The long spelling, `--name`.
    pub(crate) name: String,
    /// Whether the flag consumes a value (`--root PATH`).
    pub(crate) takes_value: bool,
    /// The closed set of accepted values when a typed owner states one;
    /// empty otherwise.
    pub(crate) values: Vec<String>,
    /// Whether the usage line shows the flag outside square brackets.
    pub(crate) required: bool,
    /// Whether the help says the flag may be given more than once.
    pub(crate) repeatable: bool,
    /// The environment variable that is the documented twin of the flag.
    pub(crate) env: Option<&'static str>,
}

/// Documented flag-to-environment twins: `(command path or "*", flag, env)`.
const FLAG_ENVS: &[(&str, &str, &str)] = &[
    ("*", "--quiet", "RIPR_QUIET"),
    ("check", "--timeout", "RIPR_CHECK_TIMEOUT"),
    ("check", "--git-timeout", "RIPR_GIT_TIMEOUT"),
];

/// Closed value sets, `(command path, flag)`. Typed owners only.
fn closed_values(path: &str, flag: &str) -> Vec<String> {
    match (path, flag) {
        ("check", "--format") => crate::OutputFormat::accepted_cli_names()
            .into_iter()
            .map(str::to_string)
            .collect(),
        // Mirrors `Mode` and the `analysis.mode` config diagnostic.
        ("check", "--mode") => ["instant", "draft", "fast", "deep", "ready"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        // Mirrors `parse_pilot_format`.
        ("pilot", "--format") => vec!["human".to_string(), "json".to_string()],
        _ => Vec::new(),
    }
}

/// `true` for the value placeholder of an option line: an all-uppercase
/// word (`PATH`, `FILE:LINE`, `PATH[::TEST_NODE]`) or an angle-bracket name.
fn is_metavar(token: &str) -> bool {
    if token.starts_with('<') && token.ends_with('>') {
        return true;
    }
    // A closed lowercase choice list (`before|after|verify`) is a value too.
    if token.contains('|')
        && token.split('|').all(|choice| {
            !choice.is_empty()
                && choice
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
    {
        return true;
    }
    let mut saw_upper = false;
    for ch in token.chars() {
        match ch {
            'A'..='Z' => saw_upper = true,
            '0'..='9' | '_' | '-' | ':' | '[' | ']' | '|' | '.' | '/' | '=' => {}
            _ => return false,
        }
    }
    saw_upper
}

/// `true` when an option line opens with two or more comma-joined long flags
/// (`--root, --label, ...`). `--help, -h` is one flag with a short form.
fn is_flag_reference_list(rest: &str) -> bool {
    let mut tokens = rest.split_whitespace();
    let first = tokens.next().unwrap_or_default();
    first.starts_with("--")
        && first.ends_with(',')
        && tokens.next().is_some_and(|next| next.starts_with("--"))
}

struct ParsedOption {
    name: String,
    takes_value: bool,
    literal_value: Option<String>,
    description: String,
}

/// The option lines of a help body: two-space indent, then `--name`
/// (optionally with a `-x,` short form first), then an optional value
/// placeholder, then prose and indented continuation lines.
fn parse_options(body: &str) -> Vec<ParsedOption> {
    let mut options: Vec<ParsedOption> = Vec::new();
    for line in body.lines() {
        let Some(rest) = line.strip_prefix("  ") else {
            continue;
        };
        let continuation = rest.starts_with(' ');
        if continuation {
            if let Some(last) = options.last_mut() {
                last.description.push(' ');
                last.description.push_str(rest.trim());
            }
            continue;
        }
        let short_first = rest.len() > 2
            && rest.as_bytes()[0] == b'-'
            && rest.as_bytes()[1].is_ascii_alphabetic()
            && rest[2..].starts_with(',');
        if !(rest.starts_with("--") || short_first) {
            // A non-option line ends the continuation of the previous one.
            if !rest.trim().is_empty() {
                options.push(ParsedOption {
                    name: String::new(),
                    takes_value: false,
                    literal_value: None,
                    description: String::new(),
                });
            }
            continue;
        }
        // `--root, --label, --out` followed by "As for <sibling>" refers to
        // flags defined on their own lines elsewhere in the body. Reading the
        // list as a definition would report its first member as a valueless
        // switch and shadow the real row (#14086).
        if is_flag_reference_list(rest) {
            options.push(ParsedOption {
                name: String::new(),
                takes_value: false,
                literal_value: None,
                description: String::new(),
            });
            continue;
        }
        let mut tokens = rest.split_whitespace().peekable();
        let mut name = String::new();
        // `--name=VALUE` spells a value-taking flag without a separate token;
        // no shipped help body does, but fail closed rather than report a
        // boolean switch.
        let mut inline_value = false;
        while let Some(token) = tokens.peek().copied() {
            let flag = token.trim_end_matches(',');
            if !flag.starts_with('-') {
                break;
            }
            if flag.starts_with("--") && name.is_empty() {
                inline_value = flag.contains('=');
                name = flag.split('=').next().unwrap_or(flag).to_string();
            }
            tokens.next();
        }
        if name.is_empty() {
            continue;
        }
        let mut takes_value = inline_value;
        let mut literal_value = None;
        if let Some(token) = tokens.peek().copied() {
            if is_metavar(token) {
                takes_value = true;
                tokens.next();
            } else if name == "--format"
                && token.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
                && rest.contains(&format!("{token}  "))
            {
                // `--format json  Explicit JSON output.`: a literal value.
                takes_value = true;
                literal_value = Some(token.to_string());
                tokens.next();
            }
        }
        options.push(ParsedOption {
            name,
            takes_value,
            literal_value,
            description: tokens.collect::<Vec<_>>().join(" "),
        });
    }
    options.retain(|option| !option.name.is_empty());
    options
}

/// Flags the usage line shows outside square brackets.
fn required_in_usage(body: &str, name: &str) -> bool {
    let mut usage = String::new();
    let mut in_usage = false;
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix("Usage:") {
            in_usage = true;
            usage.push_str(rest);
            usage.push(' ');
        } else if in_usage && !line.trim().is_empty() {
            usage.push_str(line);
            usage.push(' ');
        } else if in_usage {
            break;
        }
    }
    let mut depth = 0i32;
    let bytes = usage.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'[' => depth += 1,
            b']' => depth -= 1,
            b'-' if depth == 0 && usage[index..].starts_with(name) => {
                let after = usage[index + name.len()..].chars().next();
                if after.is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '-')) {
                    return true;
                }
            }
            _ => {}
        }
        index += 1;
    }
    false
}

/// The flag inventory of the command at `path`, from its help body, or an
/// empty list when the path prints no body of its own (a group header).
pub(crate) fn flags_for(path: &str, body: &str) -> Vec<FlagJson> {
    let mut seen = std::collections::BTreeSet::new();
    let mut flags = Vec::new();
    for option in parse_options(body) {
        if !seen.insert(option.name.clone()) {
            continue;
        }
        let repeatable = {
            let lowered = option.description.to_ascii_lowercase();
            lowered.contains("repeatable") || lowered.contains("may be repeated")
        };
        let mut values = closed_values(path, &option.name);
        if values.is_empty()
            && let Some(literal) = &option.literal_value
        {
            values.push(literal.clone());
        }
        let env = FLAG_ENVS
            .iter()
            .find(|(scope, flag, _)| (*scope == "*" || *scope == path) && *flag == option.name)
            .map(|(_, _, env)| *env);
        flags.push(FlagJson {
            takes_value: option.takes_value,
            required: required_in_usage(body, &option.name),
            name: option.name,
            values,
            repeatable,
            env,
        });
    }
    flags.sort_by(|left, right| left.name.cmp(&right.name));
    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "Do a thing.\n\nUsage: ripr thing --need VALUE [--root PATH] [--path GLOB] [--json]\n\nOptions:\n  --root PATH      Workspace root. Defaults to the\n                   current directory.\n  --need VALUE     A required value.\n  --path GLOB      Analyze matching files (repeatable).\n  --format FORMAT  Output format.\n  --json           Print JSON.\n  -q, --quiet      Silence progress.\n  --help, -h       Print this help.\n  - a bullet that is not an option\n";

    fn flag<'a>(flags: &'a [FlagJson], name: &str) -> Result<&'a FlagJson, String> {
        flags
            .iter()
            .find(|candidate| candidate.name == name)
            .ok_or_else(|| format!("flag {name} not parsed: {flags:?}"))
    }

    /// No shipped help body writes `--name=VALUE`, but a value-taking flag
    /// spelled that way must not read as a boolean switch.
    #[test]
    fn an_equals_spelled_flag_takes_a_value() -> Result<(), String> {
        let body = "Do a thing.\n\nUsage: ripr thing [--depth=N] [--json]\n\nOptions:\n  --depth=N        How deep.\n  --json           Print JSON.\n";
        let flags = flags_for("thing", body);
        assert!(flag(&flags, "--depth")?.takes_value, "{flags:?}");
        assert!(!flag(&flags, "--json")?.takes_value, "{flags:?}");
        Ok(())
    }

    /// `--phase before|after|verify` documents a value, not a switch, so
    /// `--phase=after` is split like `--phase after` (#14292 review).
    #[test]
    fn a_lowercase_choice_list_takes_a_value() -> Result<(), String> {
        let body = "Do a thing.\n\nUsage: ripr thing [--phase before|after] [--json]\n\nOptions:\n  --phase before|after   Which phase.\n  --json                 Print JSON.\n";
        let flags = flags_for("thing", body);
        assert!(flag(&flags, "--phase")?.takes_value, "{flags:?}");
        assert!(!flag(&flags, "--json")?.takes_value, "{flags:?}");
        let production = flags_for(
            "agent repair",
            crate::cli::help::help_text_for("agent repair").ok_or("agent repair help")?,
        );
        assert!(flag(&production, "--phase")?.takes_value, "{production:?}");
        Ok(())
    }

    #[test]
    fn parses_value_required_repeatable_and_env() -> Result<(), String> {
        let flags = flags_for("thing", BODY);
        let root = flag(&flags, "--root")?;
        if !root.takes_value || root.required || root.repeatable {
            return Err(format!("--root: {root:?}"));
        }
        let need = flag(&flags, "--need")?;
        if !need.takes_value || !need.required {
            return Err(format!("--need must be required with a value: {need:?}"));
        }
        let path = flag(&flags, "--path")?;
        if !path.repeatable || path.required {
            return Err(format!("--path must be repeatable and optional: {path:?}"));
        }
        let json = flag(&flags, "--json")?;
        if json.takes_value || json.required {
            return Err(format!("--json is an optional switch: {json:?}"));
        }
        let quiet = flag(&flags, "--quiet")?;
        if quiet.takes_value || quiet.env != Some("RIPR_QUIET") {
            return Err(format!("--quiet carries RIPR_QUIET: {quiet:?}"));
        }
        // Negative: a bullet line is not an option, and each flag appears once.
        if flags.iter().any(|flag| !flag.name.starts_with("--")) {
            return Err(format!("only long flags are listed: {flags:?}"));
        }
        if flags.len() != 7 {
            return Err(format!("expected 7 flags, got {}: {flags:?}", flags.len()));
        }
        Ok(())
    }

    /// #14086: a `--a, --b, --c` line refers to flags defined elsewhere in
    /// the body. It must neither define its first member as a valueless
    /// switch nor shadow the real definition that follows.
    #[test]
    fn a_flag_reference_list_defines_nothing() -> Result<(), String> {
        let body = "Do a thing.\n\nUsage: ripr thing [--root PATH]\n\nFirst options:\n  --mode MODE      A mode.\n  --root, --label, --out\n                   As for the other one.\n\nSecond options:\n  --root PATH      Workspace root.\n  --label LABEL    Repeatable label.\n  --help, -h       Print this help.\n";
        let flags = flags_for("thing", body);
        let root = flag(&flags, "--root")?;
        if !root.takes_value {
            return Err(format!("the list shadowed --root PATH: {root:?}"));
        }
        let label = flag(&flags, "--label")?;
        if !label.takes_value || !label.repeatable {
            return Err(format!("--label keeps its own row: {label:?}"));
        }
        if flag(&flags, "--mode")?.repeatable {
            return Err("the list's description leaked into --mode".to_string());
        }
        // `--out` has no defining row, so it is not invented; `--help, -h`
        // is one flag with a short form, not a list.
        if flags.iter().any(|flag| flag.name == "--out") {
            return Err(format!("--out was invented from the list: {flags:?}"));
        }
        if flag(&flags, "--help")?.takes_value {
            return Err("--help is a switch".to_string());
        }
        Ok(())
    }

    #[test]
    fn the_gate_body_reports_value_taking_flags_as_such() -> Result<(), String> {
        let body = crate::cli::help::help_text_for("gate check").ok_or("gate check has a body")?;
        let flags = flags_for("gate check", body);
        for name in [
            "--base",
            "--mode",
            "--root",
            "--gap-ledger",
            "--agent-verify",
            "--out-md",
        ] {
            if !flag(&flags, name)?.takes_value {
                return Err(format!("gate {name} takes a value: {flags:?}"));
            }
        }
        if !flag(&flags, "--label")?.repeatable {
            return Err("gate --label is repeatable".to_string());
        }
        Ok(())
    }

    #[test]
    fn closed_values_come_from_typed_owners_only() -> Result<(), String> {
        let check = flags_for("check", BODY);
        let format = flag(&check, "--format")?;
        let expected: Vec<String> = crate::OutputFormat::accepted_cli_names()
            .into_iter()
            .map(str::to_string)
            .collect();
        if format.values != expected {
            return Err(format!("check --format values: {:?}", format.values));
        }
        let thing = flags_for("thing", BODY);
        if !flag(&thing, "--format")?.values.is_empty() {
            return Err("an unknown command gets no invented value set".to_string());
        }
        Ok(())
    }

    #[test]
    fn production_bodies_yield_the_documented_flags() -> Result<(), String> {
        let body = crate::cli::help::help_text_for("check").ok_or("check has a help body")?;
        let flags = flags_for("check", body);
        for (name, takes_value, repeatable, env) in [
            ("--root", true, false, None),
            ("--base", true, false, None),
            ("--path", true, true, None),
            ("--exclude", true, true, None),
            ("--worktree", false, false, None),
            ("--quiet", false, false, Some("RIPR_QUIET")),
            ("--timeout", true, false, Some("RIPR_CHECK_TIMEOUT")),
        ] {
            let parsed = flag(&flags, name)?;
            if parsed.takes_value != takes_value
                || parsed.repeatable != repeatable
                || parsed.env != env
            {
                return Err(format!("check {name}: {parsed:?}"));
            }
        }
        let mode = flag(&flags, "--mode")?;
        if mode.values != ["instant", "draft", "fast", "deep", "ready"] {
            return Err(format!("check --mode values: {:?}", mode.values));
        }
        Ok(())
    }
}
