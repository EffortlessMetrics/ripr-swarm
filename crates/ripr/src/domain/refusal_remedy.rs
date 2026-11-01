//! Structured remedy of a refusal, derived from its prose (#11790, #11584).
//!
//! Every ripr refusal message already names its next step in prose (#11539).
//! Machine consumers (CLI `--json` refusal documents, the agent protocol, MCP
//! failures) need that step as data: which command to run, which flags to
//! pass, and whether the message itself says a retry is expected to work.
//!
//! [`RefusalRemedy::from_prose`] is the one place that reads it. The prose is
//! the source of truth, so the derivation is deliberately conservative and
//! fail-closed:
//!
//! - a command is only what the message wrote as a backticked `ripr ...`
//!   span, copied verbatim; nothing is composed, completed or guessed;
//! - a flag is only a `--flag` the message tells the reader to pass, add,
//!   use or repeat;
//! - a message that states none of these yields [`RemedyKind::None`], an
//!   explicit "no machine-readable remedy" value. The human message may still
//!   carry a prose step; the value never claims one it cannot ground;
//! - `retry_safe` is `Some(true)` only when the message itself tells the
//!   reader to rerun or retry, `Some(false)` only when it says repeating the
//!   call changes nothing, and `None` otherwise. It reports what the message
//!   states, not an analysis of side effects.
//!
//! Renderers serialize this value; they never re-derive it.

/// Longest command span copied into a remedy. A longer span is not a
/// command a consumer can run from one wire field, so it is not offered.
const MAX_COMMAND_LEN: usize = 300;

/// Alternatives beyond the first stated command are bounded and subordinate.
const MAX_ALTERNATIVES: usize = 3;

/// Bound on any list of flags or placeholders.
const MAX_LISTED: usize = 8;

/// Words directly before a `--flag` that make it an instruction to pass it.
const FLAG_VERBS: &[&str] = &[
    "pass", "add", "use", "repeat", "supply", "give", "set", "with", "specify",
];

/// Words that continue a flag list (`pass --before and --after`).
const FLAG_JOINERS: &[&str] = &["and", "or", "plus", "&", ","];

/// Phrases by which a message says repeating the call will not help.
const NO_RETRY_PHRASES: &[&str] = &[
    "changes nothing",
    "will not help",
    "does not help",
    "do not retry",
    "don't retry",
    "never retry",
    "do not rerun",
    "do not re-run",
];

/// Words by which a message tells the reader to repeat the operation.
const RETRY_WORDS: &[&str] = &["rerun", "re-run", "retry", "try again", "run it again"];

/// What kind of machine-readable remedy the prose grounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemedyKind {
    /// The message names a runnable `ripr ...` command.
    Command,
    /// The message names an MCP tool to call instead of a CLI command.
    McpTool,
    /// The message names only flags to pass.
    Flags,
    /// The message states no machine-readable remedy. Explicit, never a
    /// guess: the prose step, if any, stays in the message.
    None,
}

impl RemedyKind {
    /// Stable wire spelling, governed by `check-output-contracts`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::McpTool => "mcp_tool",
            Self::Flags => "flags",
            Self::None => "none",
        }
    }
}

impl serde::Serialize for RemedyKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The structured remedy of one refusal message. Field set is fixed so a
/// consumer reads the same keys for every kind; absent facts are `null` or
/// empty, never omitted.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) struct RefusalRemedy {
    pub(crate) kind: RemedyKind,
    /// The first runnable `ripr ...` command the message names, verbatim.
    pub(crate) command: Option<String>,
    /// Further distinct commands the message names, in message order.
    pub(crate) alternatives: Vec<String>,
    /// The first `ripr_<tool>` MCP tool name the message names.
    pub(crate) tool: Option<String>,
    /// Flags the message tells the reader to pass, add or repeat.
    pub(crate) flags: Vec<String>,
    /// Placeholders (`<ref>`, `BASE`, `{root}`) in `command` the consumer
    /// must fill before running it.
    pub(crate) fill_in: Vec<String>,
    /// Whether the message states a retry is expected to work (`true`), says
    /// repeating changes nothing (`false`), or says neither (`null`).
    pub(crate) retry_safe: Option<bool>,
}

impl RefusalRemedy {
    /// The explicit "no machine-readable remedy" value.
    #[cfg(test)]
    pub(crate) fn none() -> Self {
        Self {
            kind: RemedyKind::None,
            command: None,
            alternatives: Vec::new(),
            tool: None,
            flags: Vec::new(),
            fill_in: Vec::new(),
            retry_safe: None,
        }
    }

    /// Derive the remedy from refusal prose. Total and side-effect free.
    pub(crate) fn from_prose(prose: &str) -> Self {
        let mut commands = stated_commands(prose);
        let command = if commands.is_empty() {
            None
        } else {
            Some(commands.remove(0))
        };
        commands.truncate(MAX_ALTERNATIVES);
        let tool = stated_tool(prose);
        let flags = stated_flags(prose, command.as_deref());
        let fill_in = command.as_deref().map(placeholders).unwrap_or_default();
        let kind = if command.is_some() {
            RemedyKind::Command
        } else if tool.is_some() {
            RemedyKind::McpTool
        } else if !flags.is_empty() {
            RemedyKind::Flags
        } else {
            RemedyKind::None
        };
        Self {
            kind,
            command,
            alternatives: commands,
            tool,
            flags,
            fill_in,
            retry_safe: retry_safe(prose),
        }
    }
}

/// Backticked spans of `prose`, in order.
fn backtick_spans(prose: &str) -> impl Iterator<Item = &str> {
    prose.split('`').skip(1).step_by(2)
}

/// True when `span` is a runnable ripr command: `ripr` followed by a space
/// and a subcommand-shaped word. `ripr.toml`, `ripr_check` and a bare `ripr`
/// are not commands.
fn is_ripr_command(span: &str) -> bool {
    let Some(rest) = span.strip_prefix("ripr ") else {
        return false;
    };
    let first = rest.trim_start().split_whitespace().next().unwrap_or("");
    let subcommand_shaped = first
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '-')
        && first
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    subcommand_shaped && !span.contains('\n') && span.len() <= MAX_COMMAND_LEN
}

fn stated_commands(prose: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for span in backtick_spans(prose) {
        let span = span.trim();
        if is_ripr_command(span) && !out.iter().any(|seen| seen == span) {
            out.push(span.to_string());
        }
    }
    out
}

fn stated_tool(prose: &str) -> Option<String> {
    let bytes = prose.as_bytes();
    let mut from = 0;
    while let Some(found) = prose[from..].find("ripr_") {
        let at = from + found;
        let identifier_byte = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
        let boundary_before = at == 0 || !identifier_byte(bytes[at - 1]);
        let mut end = at + "ripr_".len();
        while end < bytes.len()
            && (bytes[end].is_ascii_lowercase()
                || bytes[end].is_ascii_digit()
                || bytes[end] == b'_')
        {
            end += 1;
        }
        let name = prose[at..end].trim_end_matches('_');
        let boundary_after = end >= bytes.len() || !identifier_byte(bytes[end]);
        if boundary_before && boundary_after && name.len() > "ripr_".len() {
            return Some(name.to_string());
        }
        from = end.max(at + 1);
    }
    None
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.'
}

/// `--flag` tokens the message tells the reader to pass. A flag counts when
/// the word right before it is an instruction verb (`pass`, `add`, ...), or a
/// joiner continuing a list whose previous item counted. Flags inside the
/// stated command belong to the command and are not repeated here.
fn stated_flags(prose: &str, command: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut last_counted_end: Option<usize> = None;
    let bytes = prose.as_bytes();
    let mut from = 0;
    while let Some(found) = prose[from..].find("--") {
        let at = from + found;
        let mut end = at + 2;
        while end < bytes.len()
            && (bytes[end].is_ascii_lowercase()
                || bytes[end].is_ascii_digit()
                || bytes[end] == b'-')
        {
            end += 1;
        }
        from = end.max(at + 2);
        let flag = prose[at..end].trim_end_matches('-');
        let starts_ok = flag.len() > 2
            && flag
                .as_bytes()
                .get(2)
                .is_some_and(|byte| byte.is_ascii_lowercase());
        // `---` rules and mid-word dashes are not flags.
        let boundary_before = at == 0 || !is_word_byte(bytes[at - 1]) || bytes[at - 1] == b'`';
        if !starts_ok || !boundary_before {
            continue;
        }
        let before = prose[..at].trim_end_matches('`').trim_end();
        let word = before
            .rsplit(|c: char| c.is_whitespace() || c == '(' || c == ';' || c == ':')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let verb_ok = FLAG_VERBS.contains(&word.as_str());
        let chained = FLAG_JOINERS.contains(&word.trim_end_matches(',')) || before.ends_with(',');
        let continues = chained
            && last_counted_end.is_some_and(|prev| {
                prose[prev..at]
                    .chars()
                    .all(|c| c.is_whitespace() || c == '`' || c == ',' || c.is_alphabetic())
                    && prose[prev..at].split_whitespace().count() <= 3
            });
        if !(verb_ok || continues) {
            continue;
        }
        last_counted_end = Some(end);
        let in_command = command.is_some_and(|command| command_has_flag(command, flag));
        if !in_command && !out.iter().any(|seen| seen == flag) && out.len() < MAX_LISTED {
            out.push(flag.to_string());
        }
    }
    out
}

fn command_has_flag(command: &str, flag: &str) -> bool {
    command.split_whitespace().any(|token| token == flag)
}

/// Placeholders in a command the consumer must fill: `<...>`, `{...}` and
/// bare uppercase words such as `BASE`.
fn placeholders(command: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for token in command.split_whitespace() {
        let angled = token.starts_with('<') && token.ends_with('>');
        let braced = token.starts_with('{') && token.ends_with('}');
        let upper = token.len() >= 2
            && token
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
            && token.chars().any(|c| c.is_ascii_uppercase());
        if (angled || braced || upper)
            && !out.iter().any(|seen| seen == token)
            && out.len() < MAX_LISTED
        {
            out.push(token.to_string());
        }
    }
    out
}

/// Whole-word match of `phrase` in lowercase `text`.
fn contains_phrase(text: &str, phrase: &str) -> bool {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(found) = text[from..].find(phrase) {
        let at = from + found;
        let end = at + phrase.len();
        let before_ok = at == 0 || !bytes[at - 1].is_ascii_alphanumeric();
        let after_ok = end >= bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        from = at + 1;
    }
    false
}

fn retry_safe(prose: &str) -> Option<bool> {
    let lower = prose.to_ascii_lowercase();
    if NO_RETRY_PHRASES
        .iter()
        .any(|phrase| contains_phrase(&lower, phrase))
    {
        return Some(false);
    }
    if RETRY_WORDS.iter().any(|word| contains_phrase(&lower, word)) {
        return Some(true);
    }
    None
}

#[cfg(test)]
#[path = "refusal_remedy_corpus_tests.rs"]
mod corpus_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backticked_ripr_command_is_copied_verbatim() {
        let remedy = RefusalRemedy::from_prose(
            "no scope; run `ripr check --base BASE` or `ripr pilot --root .` to continue",
        );
        assert_eq!(remedy.kind, RemedyKind::Command);
        assert_eq!(remedy.command.as_deref(), Some("ripr check --base BASE"));
        assert_eq!(remedy.alternatives, vec!["ripr pilot --root .".to_string()]);
        assert_eq!(remedy.fill_in, vec!["BASE".to_string()]);
    }

    #[test]
    fn placeholder_arguments_are_reported_not_filled() {
        let remedy = RefusalRemedy::from_prose(
            "seam gone; run `ripr agent repair --phase before --seam-id <id>`",
        );
        assert_eq!(remedy.fill_in, vec!["<id>".to_string()]);
    }

    #[test]
    fn quoted_and_unquoted_flags_after_an_instruction_verb_are_flags() {
        let quoted = RefusalRemedy::from_prose("needs a root; pass `--root <dir>`");
        assert_eq!(quoted.kind, RemedyKind::Flags);
        assert_eq!(quoted.flags, vec!["--root".to_string()]);
        let list = RefusalRemedy::from_prose("requires both; pass --before and --after");
        assert_eq!(
            list.flags,
            vec!["--before".to_string(), "--after".to_string()]
        );
    }

    #[test]
    fn flags_inside_the_command_are_not_repeated() {
        let remedy = RefusalRemedy::from_prose(
            "start over with `ripr agent repair --phase before`; pass --phase",
        );
        assert_eq!(remedy.kind, RemedyKind::Command);
        assert!(remedy.flags.is_empty(), "{:?}", remedy.flags);
    }

    #[test]
    fn mcp_tool_name_is_a_remedy_when_no_cli_command_is_stated() {
        let remedy =
            RefusalRemedy::from_prose("list the current ids with ripr_list_gaps, then retry");
        assert_eq!(remedy.kind, RemedyKind::McpTool);
        assert_eq!(remedy.tool.as_deref(), Some("ripr_list_gaps"));
        assert_eq!(remedy.retry_safe, Some(true));
    }

    #[test]
    fn message_without_a_stated_remedy_is_an_explicit_none() {
        for prose in [
            "the base does not resolve",
            "check the path exists and you can read it",
            "see ripr.toml and RIPR_MAX_REPO_INDEX_FILES",
            "mentions `Cargo.toml` and `ripr` alone and `ripr.toml`",
            "a flag --json appears in a sentence that is not an instruction",
            "",
        ] {
            let remedy = RefusalRemedy::from_prose(prose);
            assert_eq!(remedy, RefusalRemedy::none(), "{prose}");
        }
    }

    #[test]
    fn retry_safe_follows_only_what_the_message_states() {
        let again = RefusalRemedy::from_prose("fix the path, then rerun");
        assert_eq!(again.retry_safe, Some(true));
        let pointless = RefusalRemedy::from_prose(
            "calling it again changes nothing; read next.static_limit instead of retrying",
        );
        assert_eq!(pointless.retry_safe, Some(false));
        let silent = RefusalRemedy::from_prose("pass `--json`");
        assert_eq!(silent.retry_safe, None);
        let lookalike = RefusalRemedy::from_prose("the field is retryable");
        assert_eq!(lookalike.retry_safe, None);
    }

    #[test]
    fn overlong_command_spans_are_not_offered() {
        let long = format!("`ripr check {}`", "x".repeat(MAX_COMMAND_LEN));
        assert_eq!(RefusalRemedy::from_prose(&long).kind, RemedyKind::None);
    }
}
