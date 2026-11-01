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
//! - `retry_stated` is `Some(true)` only when the message itself tells the
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
const MAX_LISTED: usize = 16;

/// Words directly before a `--flag` that make it an instruction to pass it.
const FLAG_VERBS: &[&str] = &[
    "pass",
    "add",
    "use",
    "repeat",
    "repeating",
    "repeated",
    "supply",
    "give",
    "set",
    "with",
    "specify",
    "keep",
    "keeping",
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
    pub(crate) retry_stated: Option<bool>,
    /// `NAME=value` environment prerequisites the message states outside the
    /// command span, verbatim.
    pub(crate) env: Vec<String>,
    /// True when the only step the message gives is conditional (`if it
    /// persists, run ...`): the command is a fallback, not a first move.
    pub(crate) conditional: bool,
    /// True when `command`/`tool` is not self-sufficient: the message names
    /// flags or environment prerequisites outside the span. A consumer must
    /// not run the command as-is without reading them.
    pub(crate) prose_has_more: bool,
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
            retry_stated: None,
            env: Vec::new(),
            conditional: false,
            prose_has_more: false,
        }
    }

    /// Derive the remedy from refusal prose. Total and side-effect free.
    ///
    /// The primary step is the instructed one, not the first mention: a
    /// conditional fallback (`if it persists, run ...`) or an explanatory
    /// mention (`X is a `ripr check` finding ID`) never becomes the primary
    /// remedy while an instructed step exists. See [`Role`].
    pub(crate) fn from_prose(prose: &str) -> Self {
        let commands = stated_mentions(prose, is_ripr_command);
        let tools = stated_tool_mentions(prose);
        let pick = |mentions: &[Mention], role: Role| -> Vec<String> {
            let mut out: Vec<String> = Vec::new();
            for mention in mentions.iter().filter(|mention| mention.role == role) {
                if !out.contains(&mention.text) {
                    out.push(mention.text.clone());
                }
            }
            out
        };
        let instructed_commands = pick(&commands, Role::Instructed);
        let conditional_commands = pick(&commands, Role::Conditional);
        let instructed_tools = pick(&tools, Role::Instructed);
        let conditional_tools = pick(&tools, Role::Conditional);

        // Precedence: an instructed step beats any conditional one; within a
        // role a command beats a tool. Explanatory mentions are never used.
        let (kind, mut command_pool, tool, conditional) = if !instructed_commands.is_empty() {
            let mut pool = instructed_commands;
            pool.extend(conditional_commands);
            (
                RemedyKind::Command,
                pool,
                instructed_tools.first().cloned(),
                false,
            )
        } else if let Some(tool) = instructed_tools.first() {
            (
                RemedyKind::McpTool,
                conditional_commands,
                Some(tool.clone()),
                false,
            )
        } else if !conditional_commands.is_empty() {
            (
                RemedyKind::Command,
                conditional_commands,
                conditional_tools.first().cloned(),
                true,
            )
        } else if let Some(tool) = conditional_tools.first() {
            (RemedyKind::McpTool, Vec::new(), Some(tool.clone()), true)
        } else {
            (RemedyKind::None, Vec::new(), None, false)
        };
        let command = if kind == RemedyKind::Command && !command_pool.is_empty() {
            Some(command_pool.remove(0))
        } else {
            None
        };
        command_pool.truncate(MAX_ALTERNATIVES);
        // With a stated step, every flag named outside a command span is
        // reported (recall: a trust-bound restart must not look complete
        // without its flags). With no step, only instructed flags count.
        let ranges = command_span_ranges(prose);
        let has_step = kind != RemedyKind::None;
        let flags = stated_flags(prose, &ranges, !has_step);
        let env = stated_env(prose, &ranges);
        let kind = if kind == RemedyKind::None && !flags.is_empty() {
            RemedyKind::Flags
        } else {
            kind
        };
        let prose_has_more = has_step && (!flags.is_empty() || !env.is_empty());
        let fill_in = command.as_deref().map(placeholders).unwrap_or_default();
        Self {
            kind,
            command,
            alternatives: command_pool,
            tool,
            flags,
            fill_in,
            retry_stated: retry_stated(prose),
            env,
            conditional,
            prose_has_more,
        }
    }
}

/// True when `span` is a runnable ripr command: `ripr` followed by a space
/// and a subcommand-shaped word. `ripr.toml`, `ripr_check` and a bare `ripr`
/// are not commands.
fn is_ripr_command(span: &str) -> bool {
    let Some(rest) = span.strip_prefix("ripr ") else {
        return false;
    };
    let first = rest.split_whitespace().next().unwrap_or("");
    let subcommand_shaped = first
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '-')
        && first
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    subcommand_shaped && !span.contains('\n') && span.len() <= MAX_COMMAND_LEN
}

/// How the prose uses one mentioned command or tool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Role {
    /// The reader is told to do it (the default for a mention).
    Instructed,
    /// A fallback inside an `if`/`unless`/`when` clause.
    Conditional,
    /// Described, not requested: `X is a `ripr check` finding ID`.
    Explanatory,
}

struct Mention {
    text: String,
    role: Role,
}

/// Words that make the text after a copula an instruction again
/// (`... is stale, so run `ripr x``).
const IMPERATIVE_WORDS: &[&str] = &[
    "run", "call", "use", "try", "retry", "rerun", "re-run", "pass", "add", "execute", "see",
    "with", "via", "start", "restart", "then",
];

const COPULAS: &[&str] = &[
    " is a ",
    " is an ",
    " is the ",
    " are a ",
    " are the ",
    " are ",
];

/// Text of the clause that precedes byte offset `at`. Clauses end at `;` and
/// at `. ` outside backticks, so a `.` inside a command never splits one.
fn clause_prefix(prose: &str, at: usize) -> &str {
    let mut start = 0;
    let mut in_tick = false;
    let mut chars = prose.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if index >= at {
            break;
        }
        match ch {
            '`' => in_tick = !in_tick,
            ';' if !in_tick => start = index + 1,
            '.' if !in_tick && chars.peek().is_none_or(|(_, next)| next.is_whitespace()) => {
                start = index + 1;
            }
            _ => {}
        }
    }
    &prose[start.min(at)..at]
}

fn role_of(prefix: &str) -> Role {
    let lower = prefix.to_ascii_lowercase();
    if let Some(cond) = ["if", "unless", "when", "otherwise"]
        .iter()
        .filter_map(|word| phrase_position(&lower, word))
        .max()
    {
        // After the condition's comma the mention is the conditional step;
        // before it the mention is part of the premise (`if the id came from
        // ripr_x, ...`), which describes and does not instruct.
        return if lower[cond..].contains([',', ':']) {
            Role::Conditional
        } else {
            Role::Explanatory
        };
    }
    let after_copula = COPULAS
        .iter()
        .filter_map(|copula| lower.rfind(copula).map(|at| at + copula.len()))
        .max();
    match after_copula {
        Some(after)
            if !IMPERATIVE_WORDS
                .iter()
                .any(|word| contains_phrase(&lower[after..], word)) =>
        {
            Role::Explanatory
        }
        _ => Role::Instructed,
    }
}

/// Backticked spans accepted by `accept`, with the role the prose gives them.
fn stated_mentions(prose: &str, accept: fn(&str) -> bool) -> Vec<Mention> {
    let mut out = Vec::new();
    let mut offset = 0;
    for (index, part) in prose.split('`').enumerate() {
        let at = offset;
        offset += part.len() + 1;
        if index % 2 == 0 {
            continue;
        }
        let span = part.trim();
        if accept(span) {
            // `at` is the first byte inside the span; the tick sits before it.
            let prefix = clause_prefix(prose, at.saturating_sub(1));
            out.push(Mention {
                text: span.to_string(),
                role: role_of(prefix),
            });
        }
    }
    out
}

fn stated_tool_mentions(prose: &str) -> Vec<Mention> {
    let bytes = prose.as_bytes();
    let identifier_byte = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = prose[from..].find("ripr_") {
        let at = from + found;
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
            out.push(Mention {
                text: name.to_string(),
                role: role_of(clause_prefix(prose, at)),
            });
        }
        from = end.max(at + 1);
    }
    out
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.'
}

/// Byte ranges (including the ticks) of backticked ripr command spans.
fn command_span_ranges(prose: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut offset = 0;
    for (index, part) in prose.split('`').enumerate() {
        let start = offset;
        offset += part.len() + 1;
        if index % 2 == 1 && is_ripr_command(part.trim()) {
            out.push((start.saturating_sub(1), offset));
        }
    }
    out
}

/// `NAME=value` environment prerequisites named outside command spans.
fn stated_env(prose: &str, ranges: &[(usize, usize)]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut offset = 0;
    for token in prose.split_inclusive(char::is_whitespace) {
        let at = offset;
        offset += token.len();
        if ranges.iter().any(|(start, end)| at >= *start && at < *end) {
            continue;
        }
        let trimmed = token.trim_matches(|c: char| c.is_whitespace() || "`,;:.()".contains(c));
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        let name_ok = name.len() >= 3
            && name.starts_with(|c: char| c.is_ascii_uppercase())
            && name
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
        if name_ok
            && !value.is_empty()
            && !out.iter().any(|seen| seen == trimmed)
            && out.len() < MAX_LISTED
        {
            out.push(trimmed.to_string());
        }
    }
    out
}

/// `--flag` immediately after "the original" counts only in an instruction
/// (`repeating` / `keep` / `keeping the original`), not in a description
/// ("differs from the original `--base`").
fn original_flag_instruction(before: &str) -> bool {
    let lower = before.to_ascii_lowercase();
    lower.ends_with("repeat the original")
        || lower.ends_with("repeated the original")
        || lower.ends_with("repeating the original")
        || lower.ends_with("keep the original")
        || lower.ends_with("keeping the original")
}

/// `--flag` tokens outside command spans. When `gated`, a flag counts only if
/// the word right before it is an instruction verb (`pass`, `add`, `repeating
/// the original`, ...) or a joiner continuing a list whose previous item
/// counted; otherwise (a step is stated) every such flag counts.
fn stated_flags(prose: &str, ranges: &[(usize, usize)], gated: bool) -> Vec<String> {
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
        let boundary_before = at == 0 || !is_word_byte(bytes[at - 1]);
        if !starts_ok || !boundary_before {
            continue;
        }
        if ranges
            .iter()
            .any(|(start, stop)| at >= *start && at < *stop)
        {
            continue;
        }
        let before = prose[..at].trim_end_matches('`').trim_end();
        let word = before
            .rsplit(|c: char| c.is_whitespace() || c == '(' || c == ';' || c == ':')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let verb_ok = FLAG_VERBS.contains(&word.as_str())
            || (word == "original" && original_flag_instruction(before));
        let chained = FLAG_JOINERS.contains(&word.trim_end_matches(',')) || before.ends_with(',');
        let continues = chained
            && last_counted_end.is_some_and(|prev| {
                prose[prev..at]
                    .chars()
                    .all(|c| c.is_whitespace() || c == '`' || c == ',' || c.is_alphabetic())
                    && prose[prev..at].split_whitespace().count() <= 3
            });
        if gated && !(verb_ok || continues) {
            continue;
        }
        last_counted_end = Some(end);
        if !out.iter().any(|seen| seen == flag) && out.len() < MAX_LISTED {
            out.push(flag.to_string());
        }
    }
    out
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

/// Byte position of the last whole-word match of `phrase` in lowercase `text`.
fn phrase_position(text: &str, phrase: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let mut from = 0;
    let mut last = None;
    while let Some(found) = text[from..].find(phrase) {
        let at = from + found;
        let end = at + phrase.len();
        let before_ok = at == 0 || !word(bytes[at - 1]);
        let after_ok = end >= bytes.len() || !word(bytes[end]);
        if before_ok && after_ok {
            last = Some(at);
        }
        from = at + 1;
    }
    last
}

/// Whole-word match of `phrase` in lowercase `text`; `_` is a word character.
fn contains_phrase(text: &str, phrase: &str) -> bool {
    phrase_position(text, phrase).is_some()
}

fn retry_stated(prose: &str) -> Option<bool> {
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
    fn flags_inside_the_command_span_belong_to_the_command() {
        let remedy =
            RefusalRemedy::from_prose("start over with `ripr agent repair --phase before`");
        assert_eq!(remedy.kind, RemedyKind::Command);
        assert!(remedy.flags.is_empty(), "{:?}", remedy.flags);
        assert!(!remedy.prose_has_more);
    }

    /// The real trust-bound message (python_repair_binding.rs): the restart
    /// command is not self-sufficient without the repeated flags.
    #[test]
    fn trust_bound_restart_reports_the_flags_it_requires() {
        let remedy = RefusalRemedy::from_prose(
            "the pinned state is stale; start a new attempt with `ripr agent repair --seam-id <id> --phase before`, repeating the original `--root`, `--python-repair-trust-manifest`, `--python-repair-trust-attempt`, `--edit-authorized` and `--edit-authority <identity>` flags",
        );
        assert_eq!(remedy.kind, RemedyKind::Command);
        assert_eq!(
            remedy.command.as_deref(),
            Some("ripr agent repair --seam-id <id> --phase before")
        );
        for flag in [
            "--root",
            "--python-repair-trust-manifest",
            "--python-repair-trust-attempt",
            "--edit-authorized",
            "--edit-authority",
        ] {
            assert!(remedy.flags.iter().any(|f| f == flag), "{flag}: {remedy:?}");
        }
        assert!(remedy.prose_has_more, "command must not look complete");
    }

    #[test]
    fn env_prerequisite_outside_the_span_is_captured_and_flagged() {
        let remedy = RefusalRemedy::from_prose(
            "the repo-exposure artifact is over budget; rerun `ripr pilot` with RIPR_PILOT_REPO_EXPOSURE_BYTES=0",
        );
        assert_eq!(remedy.command.as_deref(), Some("ripr pilot"));
        assert_eq!(
            remedy.env,
            vec!["RIPR_PILOT_REPO_EXPOSURE_BYTES=0".to_string()]
        );
        assert!(remedy.prose_has_more);
        // The retry is stated by the prose, but an unchanged retry is not
        // claimed to succeed: `retry_stated` is only what the message says.
        assert_eq!(remedy.retry_stated, Some(true));
    }

    #[test]
    fn mcp_tool_name_is_a_remedy_when_no_cli_command_is_stated() {
        let remedy =
            RefusalRemedy::from_prose("list the current ids with ripr_list_gaps, then retry");
        assert_eq!(remedy.kind, RemedyKind::McpTool);
        assert_eq!(remedy.tool.as_deref(), Some("ripr_list_gaps"));
        assert_eq!(remedy.retry_stated, Some(true));
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
    fn retry_stated_follows_only_what_the_message_states() {
        let again = RefusalRemedy::from_prose("fix the path, then rerun");
        assert_eq!(again.retry_stated, Some(true));
        let pointless = RefusalRemedy::from_prose(
            "calling it again changes nothing; read next.static_limit instead of retrying",
        );
        assert_eq!(pointless.retry_stated, Some(false));
        let silent = RefusalRemedy::from_prose("pass `--json`");
        assert_eq!(silent.retry_stated, None);
        let lookalike = RefusalRemedy::from_prose("the field is retryable");
        assert_eq!(lookalike.retry_stated, None);
    }

    #[test]
    fn conditional_fallback_never_outranks_the_instructed_tool() {
        // mcp/workspace.rs check_retry_recovery
        let remedy = RefusalRemedy::from_prose(
            "retry ripr_refresh with {\"base\":\"main\"}; if the failure persists, run `ripr check --root .` outside MCP for the full diagnostic",
        );
        assert_eq!(remedy.kind, RemedyKind::McpTool, "{remedy:?}");
        assert_eq!(remedy.tool.as_deref(), Some("ripr_refresh"));
        assert_eq!(remedy.command, None);
        assert_eq!(remedy.alternatives, vec!["ripr check --root .".to_string()]);
    }

    #[test]
    fn explanatory_mention_is_not_the_primary_command() {
        // cli agent.rs unknown_seam_id_hint for a gap-shaped id
        let remedy = RefusalRemedy::from_prose(
            "`gap:...` is a `ripr check` canonical gap ID, not a seam ID; `ripr repair <gap-id> --root .` resolves it to its repair seam, or run `ripr pilot --root .` to list current seam IDs",
        );
        assert_eq!(remedy.kind, RemedyKind::Command);
        assert_eq!(
            remedy.command.as_deref(),
            Some("ripr repair <gap-id> --root .")
        );
        assert_eq!(remedy.alternatives, vec!["ripr pilot --root .".to_string()]);
        assert_eq!(remedy.fill_in, vec!["<gap-id>".to_string()]);
        // Only an explanatory mention: explicit none, never a guess.
        let only = RefusalRemedy::from_prose("`probe:x` is a `ripr check` finding ID");
        assert_eq!(only, RefusalRemedy::none());
    }

    #[test]
    fn conditional_tool_clause_does_not_lead_the_steps() {
        // mcp/repair.rs unknown_attempt
        let remedy = RefusalRemedy::from_prose(
            "if the id came from ripr_prepare_repair, call ripr_refresh and ripr_prepare_repair again to create a current transaction; durable attempts are listed by ripr_list_repair_attempts",
        );
        assert_eq!(remedy.tool.as_deref(), Some("ripr_refresh"), "{remedy:?}");
    }

    #[test]
    fn a_lone_conditional_step_is_reported_as_conditional() {
        let remedy = RefusalRemedy::from_prose("if it persists, run `ripr doctor`");
        assert_eq!(remedy.command.as_deref(), Some("ripr doctor"));
        assert!(remedy.conditional, "{remedy:?}");
        // An unconditional step is not marked conditional.
        let plain = RefusalRemedy::from_prose("run `ripr doctor`");
        assert!(!plain.conditional);
        // A conditional fallback behind an instructed step is not the primary.
        let behind =
            RefusalRemedy::from_prose("run `ripr pilot`; if it persists, run `ripr doctor`");
        assert_eq!(behind.command.as_deref(), Some("ripr pilot"));
        assert!(!behind.conditional);
    }

    #[test]
    fn underscore_is_a_word_character_for_retry_words() {
        assert_eq!(
            RefusalRemedy::from_prose("see the ripr_retry_budget field").retry_stated,
            None
        );
    }

    #[test]
    fn overlong_command_spans_are_not_offered() {
        let long = format!("`ripr check {}`", "x".repeat(MAX_COMMAND_LEN));
        assert_eq!(RefusalRemedy::from_prose(&long).kind, RemedyKind::None);
    }

    #[test]
    fn original_before_a_flag_is_not_a_verb_in_a_description() {
        let remedy = RefusalRemedy::from_prose("the base differs from the original `--base`");
        assert_eq!(remedy.kind, RemedyKind::None);
        assert!(remedy.flags.is_empty());
    }

    #[test]
    fn keep_the_original_before_a_flag_is_still_an_instruction() {
        let remedy =
            RefusalRemedy::from_prose("keep the original `--base` and rerun with the same scope");
        assert_eq!(remedy.kind, RemedyKind::Flags);
        assert_eq!(remedy.flags, vec!["--base".to_string()]);
        assert_eq!(remedy.retry_stated, Some(true));
    }

    #[test]
    fn repeat_the_original_before_a_flag_is_still_an_instruction() {
        let remedy = RefusalRemedy::from_prose("repeat the original `--base` and retry");
        assert_eq!(remedy.kind, RemedyKind::Flags);
        assert_eq!(remedy.flags, vec!["--base".to_string()]);
        assert_eq!(remedy.retry_stated, Some(true));
    }

    #[test]
    fn flag_list_is_capped_at_max_listed_preserving_order() {
        let flags: Vec<String> = (1..=17).map(|n| format!("--cap-{n:02}")).collect();
        let prose = format!(
            "run `ripr doctor`; {}",
            flags
                .iter()
                .map(|f| f.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        );
        let remedy = RefusalRemedy::from_prose(&prose);
        assert_eq!(remedy.kind, RemedyKind::Command);
        assert_eq!(remedy.flags.len(), MAX_LISTED);
        assert_eq!(
            remedy.flags,
            flags[..MAX_LISTED].to_vec(),
            "order must be preserved"
        );
        assert!(remedy.prose_has_more, "stated step with extra prose flags");
    }

    #[test]
    fn gated_flag_only_prose_respects_the_list_cap() {
        let flags: Vec<String> = (1..=17).map(|n| format!("--only-{n:02}")).collect();
        let prose = format!("pass {}", flags.join(" and "));
        let remedy = RefusalRemedy::from_prose(&prose);
        assert_eq!(remedy.kind, RemedyKind::Flags);
        assert_eq!(remedy.flags.len(), MAX_LISTED);
        assert_eq!(remedy.flags, flags[..MAX_LISTED].to_vec());
    }

    #[test]
    fn exactly_max_listed_flags_are_all_reported() {
        let flags: Vec<String> = (1..=MAX_LISTED).map(|n| format!("--full-{n:02}")).collect();
        let prose = format!("run `ripr doctor`; {}", flags.join(" "));
        let remedy = RefusalRemedy::from_prose(&prose);
        assert_eq!(remedy.flags.len(), MAX_LISTED);
        assert_eq!(remedy.flags, flags);
        assert!(remedy.prose_has_more);
    }

    #[test]
    fn duplicate_flags_do_not_consume_the_list_cap() {
        let unique: Vec<String> = (1..=MAX_LISTED).map(|n| format!("--dup-{n:02}")).collect();
        let mut prose = String::from("run `ripr doctor`; ");
        for flag in &unique {
            prose.push_str(flag);
            prose.push(' ');
            prose.push_str(flag);
            prose.push(' ');
        }
        prose.push_str("--extra-slot");
        let remedy = RefusalRemedy::from_prose(&prose);
        assert_eq!(remedy.flags.len(), MAX_LISTED);
        assert_eq!(remedy.flags, unique);
        assert!(
            !remedy.flags.iter().any(|f| f == "--extra-slot"),
            "17th distinct flag must not appear when cap is full"
        );
    }
}
