//! RIPR-SPEC-0240: unknown, not a gap.
//!
//! A `reachable_unrevealed` finding says the tests that reach a change do
//! not check it. ripr may make that claim only when the failing step was
//! established from evidence it could read. When every related `assert_eq!`
//! was refused because of a limit in ripr's own reading (an unparsed or
//! unplaced file, an unidentified test, a feature `cfg`, a binding that only
//! may rebind the macro), the finding becomes `static_unknown` with a named
//! `static_limit_kind`, so no renderer, gate, or repair route treats an
//! analyzer blind spot as a missing test.
//!
//! A refusal for a shape that can keep the assertion from running (a branch,
//! a closure, an opaque macro, a gated module, a real rebinding) keeps the
//! gap: runtime controls in `tests/owner_pin_execution.rs` prove those gaps.
//!
//! This is the single owner of that decision for Rust findings. It runs after
//! every other limit post-pass and never upgrades a class: it only withholds a
//! gap, which keeps the actionability flip fail-closed.

use super::decision::confidence_score;
use super::reveal::ASSERTION_CONTEXT_UNESTABLISHED;
use crate::analysis::facts::{FunctionContainer, TestFact};
use crate::analysis::rust_index::RustIndex;
use crate::domain::*;

/// Classifier-to-post-pass marker: every refused related `assert_eq!` was
/// refused for an analyzer limit. The post-pass removes it from the finding.
pub(in crate::analysis) const REFUSALS_ARE_ANALYZER_LIMITS: &str =
    "gap_admission: every refused related assertion rests on an analyzer limit";

const LIMIT: StaticLimitKind = StaticLimitKind::RustAssertionContextUnresolved;
const JUDGED_TEST_LIMIT: usize = 3;

/// Withhold a Rust gap whose evidence rests on an analyzer limit. Returns
/// `true` when the finding was rewritten to `static_unknown`.
pub(in crate::analysis) fn withhold_unsupported_gap(finding: &mut Finding) -> bool {
    let before = finding.evidence.len();
    finding
        .evidence
        .retain(|line| line != REFUSALS_ARE_ANALYZER_LIMITS);
    let marked = finding.evidence.len() != before;
    if !(marked && gap_rests_on_refusal(finding)) {
        return false;
    }
    let judged = judged_tests(finding);
    let macro_name = if finding.probe.family == ProbeFamily::MatchArm {
        "assert"
    } else {
        "assert_eq"
    };
    let first_unresolved_edge =
        format!("whether the refused related assertions run as the standard `{macro_name}!`");
    withhold_gap(
        finding,
        LIMIT,
        &first_unresolved_edge,
        next_step(&judged, macro_name),
        &judged,
    );
    true
}

/// Rewrite a gap finding to `static_unknown` under the named `limit`:
/// rescore, record the stop reason and the limitation edges, and drop the
/// absence claims that would feed repair routes.
pub(super) fn withhold_gap(
    finding: &mut Finding,
    limit: StaticLimitKind,
    first_unresolved_edge: &str,
    next_step: String,
    judged: &[String],
) {
    let withheld_class = finding.class.as_str();
    finding.class = ExposureClass::StaticUnknown;
    // The classifier scored this finding as a gap, and a gap class earns a
    // bonus for a confident negative claim. Rescore for the class it now
    // has; `min` keeps any lower score an earlier limit pass assigned.
    let ripr = &finding.ripr;
    finding.confidence = finding.confidence.min(confidence_score(
        &ripr.reach,
        &ripr.infect,
        &ripr.propagate,
        &ripr.reveal.observe,
        &ripr.reveal.discriminate,
        &finding.class,
    ));
    finding.static_limit_kind = Some(limit);
    if !finding
        .stop_reasons
        .contains(&StopReason::GapEvidenceUnresolved)
    {
        finding.stop_reasons.push(StopReason::GapEvidenceUnresolved);
    }
    finding.evidence.push(format!(
        "gap_withheld: {withheld_class} is not claimed; {first_unresolved_edge} ({})",
        limit.as_str()
    ));
    finding.evidence.extend([
        format!(
            "{LIMITATION_LAST_ESTABLISHED_EDGE_PREFIX}{}",
            last_established_edge(finding, judged)
        ),
        format!("{LIMITATION_FIRST_UNRESOLVED_EDGE_PREFIX}{first_unresolved_edge}"),
        format!("{LIMITATION_ANALYZER_ROUTE_PREFIX}analysis/classify/gap-admission"),
        format!(
            "{LIMITATION_NON_CLAIM_PREFIX}no missing test, no weak oracle, and no test adequacy is established"
        ),
    ]);
    // The absence lines ("no assertion observes ...") and the missing
    // discriminators are the unestablished claim itself. Keeping them would
    // feed repair placement and agent packets a gap this policy withheld.
    finding.missing = vec![
        limit
            .describe_for_family(&finding.probe.family)
            .into_owned(),
    ];
    finding.activation.missing_discriminators.clear();
    finding.recommended_next_step = Some(next_step);
}

const LOG_LIMIT: StaticLimitKind = StaticLimitKind::RustLogCaptureUnresolved;
const LOG_FIRST_UNRESOLVED_EDGE: &str =
    "whether the captured log record carries the changed value the test asserts";
const LOG_MACROS: [&str; 5] = ["trace", "debug", "info", "warn", "error"];

/// #7133: withhold the gap on a changed `log::` record when a
/// related test installs a capturing logger before it calls the owner and
/// asserts exactly on a value read from that logger's store. ripr reads
/// neither the logger nor the format template, so the comparison may well
/// discriminate; it is a named limit, never credit. Only a free-function
/// owner (`owner_name`) counts: a bare `owner(` call cannot name a method.
/// Returns `true` when the finding was rewritten to `static_unknown`.
pub(in crate::analysis) fn withhold_log_capture_gap(
    finding: &mut Finding,
    index: &RustIndex,
    owner_name: Option<&str>,
) -> bool {
    let Some(owner) = owner_name else {
        return false;
    };
    if finding.static_limit_kind.is_some()
        || finding.class != ExposureClass::WeaklyExposed
        || !matches!(
            finding.probe.family,
            ProbeFamily::SideEffect | ProbeFamily::CallDeletion
        )
        || !is_qualified_log_macro(&finding.probe.expression)
    {
        return false;
    }
    // One string literal of the asserted operands must carry the static
    // fragments of the changed record's format template, in order, so an
    // assertion on another record of the same owner keeps this record's
    // gap. A template with no alphanumeric static text cannot be bound to
    // an assertion and keeps its gap.
    let Some(template) = log_template(&finding.probe.expression) else {
        return false;
    };
    let capturing: Vec<String> = finding
        .related_tests
        .iter()
        .filter(|related| {
            // A name-only or file-only relation does not show that the
            // test's bare `owner(` call reaches the changed owner (an
            // imported same-name function would also match).
            related.relation_reason == Some(RelationReason::DirectOwnerCall)
                && index.tests().iter().any(|test| {
                    test.name == related.name
                        && test.file == related.file
                        && test_asserts_captured_log(test, owner, index, &template)
                })
        })
        .map(|related| {
            format!(
                "`{}` ({}:{})",
                related.name,
                related.file.display().to_string().replace('\\', "/"),
                related.line
            )
        })
        .collect();
    if capturing.is_empty() {
        return false;
    }
    let next_step = format!(
        "{} installs a capturing logger and asserts on the captured records, which ripr does not resolve, so it does not report a gap. Check that the assertion compares the changed log record; add a test only if none does.",
        capturing.join(", ")
    );
    withhold_gap(
        finding,
        LOG_LIMIT,
        LOG_FIRST_UNRESOLVED_EDGE,
        next_step,
        &capturing,
    );
    true
}

/// `log::info!(..)` and the other `log` level macros, by their qualified
/// path only: a bare `info!` may be any macro. `tracing::` events reach a
/// `log` logger only through tracing's `log` feature, which ripr does not
/// read, so they keep their gap.
fn is_qualified_log_macro(expression: &str) -> bool {
    let trimmed = expression.trim();
    trimmed.strip_prefix("log::").is_some_and(|rest| {
        LOG_MACROS.iter().any(|level| {
            rest.strip_prefix(level)
                .is_some_and(|after| after.trim_start().starts_with('!'))
        })
    })
}

/// Whether `test` installs a capturing logger, then calls `owner`, then
/// asserts with an exact oracle on a captured value. The install is
/// `log::set_logger(&STORE)` / `log::set_boxed_logger(STORE)` with a plain
/// binding as argument, or `let store = helper();` where `helper` is in the
/// same file, installs a logger and returns a type with a same-file
/// `impl log::Log`. The store also extends one
/// hop to a binding read from it after the owner call
/// (`let lines = logger.0.lock()..;`). Every owner call is tried, so a
/// re-call after a late capture still counts. The assertion must carry a
/// string literal and no count or status projection: `lines.len()` or
/// `installed.is_ok()` does not compare the record's text.
fn test_asserts_captured_log(
    test: &TestFact,
    owner: &str,
    index: &RustIndex,
    template: &LogTemplate,
) -> bool {
    let masked = crate::analysis::extract::mask_comments_and_strings(&test.body);
    let same_file = || {
        index
            .functions()
            .iter()
            .filter(|function| function.file == test.file)
    };
    let logger_types: Vec<&str> = same_file()
        .filter_map(|function| match &function.item.container {
            FunctionContainer::TraitImpl {
                trait_path,
                self_ty,
            } if matches!(trait_path.as_str(), "log::Log" | "Log") => Some(self_ty.as_str()),
            _ => None,
        })
        .collect();
    let helpers: Vec<&str> = same_file()
        .filter(|function| returns_installed_logger(function.body.as_str(), &logger_types))
        .map(|function| function.name.as_str())
        .collect();
    free_call_offsets(&masked, owner)
        .into_iter()
        .any(|call_at| captured_and_asserted_around(test, &masked, call_at, &helpers, template))
}

fn captured_and_asserted_around(
    test: &TestFact,
    masked: &str,
    call_at: usize,
    helpers: &[&str],
    template: &LogTemplate,
) -> bool {
    let call_line = test.start_line + masked[..call_at].matches('\n').count();
    let before = &masked[..call_at];
    let mut stores: Vec<String> = install_arguments(before);
    for helper in helpers {
        for at in free_call_offsets(before, helper) {
            stores.extend(let_binding_at(masked, at));
        }
    }
    if stores.is_empty() {
        return false;
    }
    // One hop: a binding read from the store after the owner call. A read
    // through a count or status projection is not the captured text, and a
    // `let` that rebinds a store's name without reading a store drops it.
    let mut read_from_store = Vec::new();
    for statement in masked[call_at..].split(';') {
        let Some(binding) = let_binding_at(statement, statement.len()) else {
            continue;
        };
        let initializer = statement.split_once('=').map_or("", |(_, rest)| rest);
        let reads_store = stores.iter().any(|store| names(initializer, store));
        stores.retain(|store| *store != binding);
        read_from_store.retain(|read| *read != binding);
        if reads_store && !has_count_or_status_projection(initializer) {
            read_from_store.push(binding);
        }
    }
    stores.extend(read_from_store);
    test.assertions.iter().any(|assertion| {
        // Only the compared operands count: a quoted diagnostic message
        // (`assert_eq!(lines, Vec::new(), "empty")`) is not a record.
        let Some(operands) = crate::analysis::extract::assertion_oracle_text(&assertion.text)
        else {
            return false;
        };
        let (masked_assertion, string_quotes) =
            crate::analysis::extract::mask_with_string_delimiter_quotes(&operands);
        assertion.line > call_line
            && string_literal_contents(&operands)
                .iter()
                .any(|literal| template.matches(literal))
            && matches!(
                assertion.kind,
                OracleKind::ExactValue | OracleKind::WholeObjectEquality
            )
            && string_quotes >= 2
            && !has_count_or_status_projection(&masked_assertion)
            && stores.iter().any(|store| names(&masked_assertion, store))
    })
}

/// The values of the string literals in `code`, read by the shared lexical
/// scan so a literal inside a comment (nested block comments included) or a
/// quote in a character literal is never live. A raw literal's value is its
/// contents; a cooked literal is unescaped, and one with an escape
/// [`unescape`] does not decode is left out, and so is a `format!` /
/// `format_args!` template, whose text is not the value it produces.
fn string_literal_contents(code: &str) -> Vec<String> {
    let masked = crate::analysis::extract::mask_comments_and_strings(code);
    crate::analysis::extract::string_literal_spans(code)
        .into_iter()
        .filter(|(open, _, _)| !opens_format_template(masked.get(..*open).unwrap_or_default()))
        .filter_map(|(open, close, raw)| {
            let contents = &code[open + 1..close];
            if raw {
                Some(contents.to_string())
            } else {
                unescape(contents)
            }
        })
        .collect()
}

/// Whether the masked code `before` a string literal's quote ends by
/// opening a `format!` / `format_args!` call, so the literal is its
/// template (`format!(r#"..`, `format! [`, `std::format!{` included).
fn opens_format_template(before: &str) -> bool {
    let before = before.trim_end_matches('#');
    let before = before.strip_suffix('r').unwrap_or(before).trim_end();
    let Some(before) = before.strip_suffix(['(', '[', '{']) else {
        return false;
    };
    let Some(name) = before.trim_end().strip_suffix('!') else {
        return false;
    };
    let name = name.trim_end();
    name.ends_with("format") || name.ends_with("format_args")
}

/// The value of cooked string contents with the simple escapes (`\n`,
/// `\t`, `\r`, `\0`, `\\`, `\"`, `\'`) decoded. `None` for any other
/// escape (`\u{..}`, `\x..`, a line continuation), so text that is not
/// decoded is never compared.
fn unescape(contents: &str) -> Option<String> {
    let mut value = String::with_capacity(contents.len());
    let mut chars = contents.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            value.push(ch);
            continue;
        }
        value.push(match chars.next()? {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '0' => '\0',
            '\\' => '\\',
            '"' => '"',
            '\'' => '\'',
            _ => return None,
        });
    }
    Some(value)
}

/// Whether every fragment occurs in `literal`, each after the previous one.
fn contains_in_order(literal: &str, fragments: &[String]) -> bool {
    let mut rest = literal;
    fragments.iter().all(|fragment| {
        rest.find(fragment.as_str()).is_some_and(|at| {
            rest = &rest[at + fragment.len()..];
            true
        })
    })
}

/// Projections that read a count, a status or a partial match, not the
/// captured text.
const COUNT_OR_STATUS_PROJECTIONS: [&str; 11] = [
    ".len()",
    ".is_empty()",
    ".count()",
    ".capacity()",
    ".is_ok()",
    ".is_err()",
    ".is_some()",
    ".is_none()",
    ".starts_with(",
    ".ends_with(",
    ".contains(",
];

fn has_count_or_status_projection(masked: &str) -> bool {
    COUNT_OR_STATUS_PROJECTIONS
        .iter()
        .any(|projection| masked.contains(projection))
}

/// The contents of the format-template argument of a `log::` macro call:
/// the first argument after an optional `target: <expr>,` and an optional
/// `key = value, ..;` list. Separators are found on the shared masked view,
/// so a `,` or `;` inside a comment, string or character literal never
/// counts. `None` when that argument is not a plain string literal, so the
/// record cannot be bound to a test.
fn log_template_literal(expression: &str) -> Option<&str> {
    // The arguments open right after `log::<level>!`; searching for the
    // first `!(` could land in a nested `format!(..)` when the record uses
    // `log::info! (` or brace delimiters, so anything but `(` is unbound.
    let rest = expression.trim_start().strip_prefix("log::")?;
    let after_level = LOG_MACROS
        .iter()
        .find_map(|level| rest.strip_prefix(level))?;
    let arguments = after_level
        .trim_start()
        .strip_prefix('!')?
        .trim_start()
        .strip_prefix('(')?;
    let masked = crate::analysis::extract::mask_comments_and_strings(arguments);
    let (commas, semicolons) = top_level_separators(&masked);
    let mut start = 0;
    if masked
        .trim_start()
        .strip_prefix("target")
        .is_some_and(|rest| rest.trim_start().starts_with(':'))
    {
        start = commas.first()? + 1;
    }
    if let Some(semicolon) = semicolons.iter().find(|at| **at >= start) {
        start = semicolon + 1;
    }
    // The template is the next token: only masked comments and whitespace
    // may sit between the separator and its opening quote, which also
    // rules out a raw (`r"`) or byte (`b"`) prefix.
    crate::analysis::extract::string_literal_spans(arguments)
        .into_iter()
        .find(|(quote, _, _)| *quote >= start)
        .filter(|(quote, _, _)| {
            masked
                .get(start..*quote)
                .is_some_and(|gap| gap.trim().is_empty())
        })
        .map(|(quote, close, _)| &arguments[quote + 1..close])
}

/// Byte offsets of the depth-zero `,` and `;` in `masked` macro arguments
/// (comments, strings and character literals already blanked); scanning
/// stops at the closing `)`.
fn top_level_separators(masked: &str) -> (Vec<usize>, Vec<usize>) {
    let (mut commas, mut semicolons) = (Vec::new(), Vec::new());
    let mut depth = 0usize;
    for (at, ch) in masked.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => break,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => commas.push(at),
            ';' if depth == 0 => semicolons.push(at),
            _ => {}
        }
    }
    (commas, semicolons)
}

/// The static text of a `log::` record's format template: its fragments
/// between placeholders and escapes (`"audited {}"` gives `["audited "]`),
/// and whether the template starts or ends with static text, which then
/// anchors the asserted literal (`"audited {}"` does not match
/// `"unaudited 42"`).
#[derive(Debug, PartialEq, Eq)]
struct LogTemplate {
    fragments: Vec<String>,
    anchored_start: bool,
    anchored_end: bool,
}

impl LogTemplate {
    /// Whether `literal` could be this record's text: anchored edges match
    /// exactly and the remaining fragments occur in order between them.
    fn matches(&self, literal: &str) -> bool {
        let mut body = literal;
        let mut fragments = self.fragments.as_slice();
        if self.anchored_start {
            let Some((first, rest)) = fragments.split_first() else {
                return false;
            };
            let Some(stripped) = body.strip_prefix(first.as_str()) else {
                return false;
            };
            body = stripped;
            fragments = rest;
        }
        if self.anchored_end {
            match fragments.split_last() {
                Some((last, rest)) => {
                    let Some(stripped) = body.strip_suffix(last.as_str()) else {
                        return false;
                    };
                    body = stripped;
                    fragments = rest;
                }
                // A template of static text only: the literal is that text.
                None => return body.is_empty(),
            }
        }
        contains_in_order(body, fragments)
    }
}

/// The [`LogTemplate`] of a `log::` record. `None` when the template cannot
/// be read or has no alphanumeric static text.
fn log_template(expression: &str) -> Option<LogTemplate> {
    // Compared as decoded text, so an escape is ordinary text rather than
    // a gap that could absorb another record's words.
    let template = unescape(log_template_literal(expression)?)?;
    let mut fragments = Vec::new();
    let mut current = String::new();
    let mut chars = template.chars().peekable();
    let mut arguments: Vec<String> = Vec::new();
    // `{}` takes the next positional argument, so it is named by its index.
    let mut next_positional = 0usize;
    let (mut positional, mut named) = (false, false);
    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                current.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                current.push('}');
            }
            '{' => {
                fragments.push(std::mem::take(&mut current));
                let mut argument = String::new();
                for inner in chars.by_ref() {
                    if inner == '}' {
                        break;
                    }
                    argument.push(inner);
                }
                // A named or positional argument used twice (`"{0} by {0}"`)
                // ties two placeholders together, which independent gaps
                // cannot express; such a template is not bound.
                let (name, spec) = argument.split_once(':').unwrap_or((&argument, ""));
                // A width or precision taken from an argument (`{:1$}`,
                // `{:w$}`, `{:.*}`) ties placeholders the same way.
                if spec.contains(['$', '*']) {
                    return None;
                }
                let mut name = name.trim().to_string();
                if name.is_empty() {
                    name = next_positional.to_string();
                    next_positional += 1;
                }
                // A named argument also has a position (`"{} id {x}", x = v`),
                // so mixing named and positional placeholders may tie them.
                if name.parse::<usize>().is_ok() {
                    positional = true;
                } else {
                    named = true;
                }
                if (positional && named) || arguments.contains(&name) {
                    return None;
                }
                arguments.push(name);
            }
            _ => current.push(ch),
        }
    }
    fragments.push(current);
    // Any static text at an edge, whitespace included, anchors that edge.
    let anchored_start = fragments
        .first()
        .is_some_and(|fragment| !fragment.is_empty());
    let anchored_end = fragments
        .last()
        .is_some_and(|fragment| !fragment.is_empty());
    let fragments: Vec<String> = fragments
        .into_iter()
        .filter(|fragment| !fragment.is_empty())
        .collect();
    // Static text of only spaces and punctuation (`"{}: {}"`) shows up in
    // unrelated records, so it cannot bind the assertion to this record.
    fragments
        .iter()
        .any(|fragment| fragment.chars().any(char::is_alphanumeric))
        .then_some(LogTemplate {
            fragments,
            anchored_start,
            anchored_end,
        })
}

/// Plain-binding arguments of `log::set_logger(..)` /
/// `log::set_boxed_logger(..)` calls in masked `code` (`&LOGGER` names
/// `LOGGER`). The call's own result is never a store.
fn install_arguments(code: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    for install in ["log::set_logger(", "log::set_boxed_logger("] {
        for (at, call) in code.match_indices(install) {
            let argument = code[at + call.len()..]
                .split([')', ','])
                .next()
                .unwrap_or_default()
                .trim()
                .trim_start_matches('&')
                .trim();
            if is_ident(argument) {
                arguments.push(argument.to_string());
            }
        }
    }
    arguments
}

/// Whether a helper's body installs a logger through `log::`, returns that
/// binding, and has a return type naming one of `logger_types` (a same-file
/// type with an `impl log::Log`), so a `let` of its result is the capture
/// store. A helper that installs one logger but returns another value, even
/// of the same type, is not a store.
fn returns_installed_logger(body: &str, logger_types: &[&str]) -> bool {
    let masked = crate::analysis::extract::mask_comments_and_strings(body);
    let installed = install_arguments(&masked);
    // The installed binding must be a block's tail expression (the body's,
    // or a `get_or_init(|| { .. })` closure's), so the helper hands back the
    // logger it installed rather than some other value of the same type.
    let returns_installed = masked.match_indices('}').any(|(close, _)| {
        let inner = &masked[..close];
        let tail_start = inner.rfind([';', '{', '}']).map_or(0, |index| index + 1);
        let tail = inner[tail_start..].trim().trim_start_matches('&').trim();
        installed.iter().any(|argument| argument == tail)
    });
    let signature = masked.split('{').next().unwrap_or_default();
    let Some((_, return_type)) = signature.split_once("->") else {
        return false;
    };
    returns_installed
        && logger_types.iter().any(|logger| {
            let name = logger.rsplit("::").next().unwrap_or(logger);
            names(return_type, name)
        })
}

/// Offsets of `name(` calls in masked `code` that are not method calls,
/// calls through another module's path (`other::name(`; `super::`,
/// `crate::` and `self::` still count) or part of a longer identifier.
fn free_call_offsets(code: &str, name: &str) -> Vec<usize> {
    let call = format!("{name}(");
    code.match_indices(&call)
        .filter(|(at, _)| {
            let before = &code[..*at];
            let path_prefix = before.ends_with("::")
                && !["super::", "crate::", "self::"]
                    .iter()
                    .any(|local| before.ends_with(local));
            !path_prefix
                && !code[..*at].chars().next_back().is_some_and(|before| {
                    before.is_alphanumeric() || before == '_' || before == '.'
                })
        })
        .map(|(at, _)| at)
        .collect()
}

/// The binding of the `let [mut] binding = ..` statement that contains
/// offset `at` of masked `code`.
fn let_binding_at(code: &str, at: usize) -> Option<String> {
    let before = code.get(..at)?;
    let start = before.rfind([';', '{', '}']).map_or(0, |index| index + 1);
    let statement = before[start..].trim_start();
    let rest = statement.strip_prefix("let ")?.trim_start();
    let rest = rest.strip_prefix("mut ").map_or(rest, str::trim_start);
    let end = rest
        .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .unwrap_or(rest.len());
    let binding = &rest[..end];
    (is_ident(binding) && rest[end..].trim_start().starts_with(['=', ':']))
        .then(|| binding.to_string())
}

fn is_ident(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_alphanumeric() || ch == '_')
        && text != "_"
}

fn names(text: &str, ident: &str) -> bool {
    text.match_indices(ident).any(|(at, name)| {
        let boundary = |ch: char| !(ch.is_alphanumeric() || ch == '_');
        text[..at].chars().next_back().is_none_or(boundary)
            && text[at + name.len()..].chars().next().is_none_or(boundary)
    })
}

/// Classifier-to-post-pass marker (#8600): some matched related test has a
/// line at the call-fact budget, seen before the render cap trims the rows.
/// The post-pass consumes it.
pub(in crate::analysis) const CALL_BUDGET_SATURATED: &str =
    "gap_admission: a related test line is at the call-fact budget";

/// Absence lines `missing_evidence` derives from a test's calls and inputs.
const BUDGET_ABSENCE_LINES: [&str; 4] = [
    "No relevant oracle was detected",
    "No strong discriminator was detected",
    "No exact error variant discriminator was detected",
    "No detected boundary input for the changed predicate",
];

const BUDGET_LIMIT: StaticLimitKind = StaticLimitKind::RustCallFactBudget;
const BUDGET_FIRST_UNRESOLVED_EDGE: &str =
    "whether a related test line holds more calls than the per-line call-fact budget kept";

/// #8600: `extract_call_facts` keeps at most `MAX_CALL_FACTS_PER_LINE`
/// distinct callees per source line and drops the rest without a marker. A
/// finding's negative claim (no assertion, no helper, one owner call) reads
/// the related test's calls, so a related test with a saturated line cannot
/// establish it: the dropped call may be the oracle. This is the one shared
/// owner of that decision, derived from the shared saturation predicate, so
/// no consumer re-derives it and no facts or cache schema carries a flag.
///
/// A gap class (`reachable_unrevealed`, `weakly_exposed`) becomes
/// `static_unknown`. An already-unknown class keeps its class and gains the
/// named reason. A positive (`exposed`) or `no_static_path` finding is left
/// alone: a dropped call can only remove reach, never invent a gap. Never
/// upgrades a class. Returns `true` when the finding was changed.
pub(in crate::analysis) fn withhold_call_budget_gap(
    finding: &mut Finding,
    index: &RustIndex,
) -> bool {
    // The classifier saw every matched test before the render cap trimmed the
    // rows; the marker carries that, and is always consumed.
    let before = finding.evidence.len();
    finding
        .evidence
        .retain(|line| line != CALL_BUDGET_SATURATED);
    let marked = finding.evidence.len() != before;
    if finding.static_limit_kind.is_some() {
        return false;
    }
    let is_gap = matches!(
        finding.class,
        ExposureClass::ReachableUnrevealed | ExposureClass::WeaklyExposed
    );
    let is_unknown = matches!(
        finding.class,
        ExposureClass::InfectionUnknown
            | ExposureClass::PropagationUnknown
            | ExposureClass::StaticUnknown
    );
    if !(is_gap || is_unknown) {
        return false;
    }
    let saturated: Vec<String> = finding
        .related_tests
        .iter()
        .filter(|related| {
            index.tests().iter().any(|test| {
                test.name == related.name
                    && test.file == related.file
                    && test.start_line == related.line
                    && crate::analysis::extract::has_saturated_line(test.calls.iter())
            })
        })
        .map(|related| {
            format!(
                "`{}` ({}:{})",
                related.name,
                related.file.display().to_string().replace('\\', "/"),
                related.line
            )
        })
        .collect();
    if saturated.is_empty() && !marked {
        return false;
    }
    // A saturated test's absence rows (no call path, no assertion, no input
    // reaching the value, no observing assertion) are claims over its calls
    // or assertion helpers. `weak_assertion` and `missing_exact_assertion`
    // describe an assertion ripr did read, so they stay. Absence rows are
    // claims over its calls; a dropped call may be the owner call or the
    // assertion helper, so they read as unconfirmed, not as established.
    for related in &mut finding.related_tests {
        let is_saturated = index.tests().iter().any(|test| {
            test.name == related.name
                && test.file == related.file
                && test.start_line == related.line
                && crate::analysis::extract::has_saturated_line(test.calls.iter())
        });
        if is_saturated
            && matches!(
                related.miss,
                Some(
                    RelatedTestMiss::NoCallPath
                        | RelatedTestMiss::NoAssertion
                        | RelatedTestMiss::MissingInput
                        | RelatedTestMiss::AssertionNotObserving
                )
            )
        {
            related.miss = Some(RelatedTestMiss::ObservationUnconfirmed);
        }
    }
    let subject = if saturated.is_empty() {
        "A related test".to_string()
    } else {
        saturated.join(", ")
    };
    if is_gap {
        let next_step = format!(
            "{subject} has a line with {} distinct calls, the per-line call budget, so ripr may not see every call there and does not report a gap. Split that line or check the test's assertions directly; add a test only if none checks the change.",
            crate::analysis::extract::MAX_CALL_FACTS_PER_LINE
        );
        withhold_gap(
            finding,
            BUDGET_LIMIT,
            BUDGET_FIRST_UNRESOLVED_EDGE,
            next_step,
            &saturated,
        );
        return true;
    }
    finding.static_limit_kind = Some(BUDGET_LIMIT);
    // Drop the absence claims over the test's calls, as `withhold_gap` does
    // for a gap: no oracle, no discriminator, no exact variant, no boundary
    // input (a dropped call such as `row(99)` could be the input), and the
    // missing-discriminator values and prescription built from them. The
    // finding's other missing evidence stays; the limit takes their place.
    let before_missing = finding.missing.len();
    finding.missing.retain(|line| {
        !BUDGET_ABSENCE_LINES.contains(&line.as_str())
            && !line.starts_with(MISSING_DISCRIMINATOR_VALUE_PREFIX)
    });
    let had_discriminators = !finding.activation.missing_discriminators.is_empty();
    finding.activation.missing_discriminators.clear();
    if finding.missing.len() != before_missing || had_discriminators {
        finding.missing.push(
            BUDGET_LIMIT
                .describe_for_family(&finding.probe.family)
                .into_owned(),
        );
        finding.recommended_next_step = Some(format!(
            "{subject} has a line with {} distinct calls, the per-line call budget, so ripr may not see every call or input there. Split that line or check the test directly; add a test only if none checks the change.",
            crate::analysis::extract::MAX_CALL_FACTS_PER_LINE
        ));
    }
    finding.evidence.extend([
        format!(
            "{LIMITATION_LAST_ESTABLISHED_EDGE_PREFIX}{}",
            last_established_edge(finding, &saturated)
        ),
        format!("{LIMITATION_FIRST_UNRESOLVED_EDGE_PREFIX}{BUDGET_FIRST_UNRESOLVED_EDGE}"),
        format!("{LIMITATION_ANALYZER_ROUTE_PREFIX}analysis/classify/gap-admission"),
        format!(
            "{LIMITATION_NON_CLAIM_PREFIX}no missing test, no weak oracle, and no test adequacy is established"
        ),
    ]);
    true
}

const PANIC_LIMIT: StaticLimitKind = StaticLimitKind::RustPanicOutcomeUnresolved;
const PANIC_FIRST_UNRESOLVED_EDGE: &str =
    "whether the changed code decides the panic the `#[should_panic]` test expects";
const PANIC_MACROS: [&str; 10] = [
    "panic",
    "unreachable",
    "todo",
    "unimplemented",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
];

/// RIPR-SPEC-0240: a `#[should_panic]` test with no assertion still has an
/// oracle, the panic outcome. When such a test calls the owner directly and
/// the owner can panic at or after the changed line, a changed guard may
/// decide that panic, so "no assertion" is not established and the
/// `reachable_unrevealed` gap is withheld under its own limit. A test with
/// `expected = ".."` counts only when that text is in a string literal of
/// the owner, since a different message means the panic comes from
/// elsewhere. ripr does not credit the panic either: a bare
/// `#[should_panic]` also passes on a panic from another callee. Returns
/// `true` when the finding was rewritten to `static_unknown`.
pub(in crate::analysis) fn withhold_should_panic_gap(
    finding: &mut Finding,
    index: &RustIndex,
) -> bool {
    if finding.static_limit_kind.is_some() || finding.class != ExposureClass::ReachableUnrevealed {
        return false;
    }
    let Some(owner_id) = finding.probe.owner.as_ref() else {
        return false;
    };
    let changed_line = finding.probe.location.line;
    let Some(owner) = index.functions().iter().find(|function| {
        function.id == *owner_id
            && function.start_line <= changed_line
            && changed_line <= function.end_line
    }) else {
        return false;
    };
    let body = owner.body.as_str();
    let masked = crate::analysis::extract::mask_comments_and_strings(body);
    let tail_start: usize = body
        .split_inclusive('\n')
        .take(changed_line - owner.start_line)
        .map(str::len)
        .sum();
    let invocations = panic_macro_arguments(&masked, tail_start);
    if invocations.is_empty() {
        return false;
    }
    // An `expected` message counts only when it is a literal argument of a
    // panicking macro invoked on or after the changed line: a message raised
    // before the change, or a literal the owner never panics with, does not
    // depend on it.
    let owner_literals: Vec<String> = invocations
        .into_iter()
        .filter_map(|arguments| body.get(arguments))
        .flat_map(string_literal_contents)
        .collect();
    let panicking: Vec<String> = finding
        .related_tests
        .iter()
        .filter(|related| {
            related.relation_reason == Some(RelationReason::DirectOwnerCall)
                && index.tests().iter().any(|test| {
                    test.name == related.name
                        && test.file == related.file
                        && test.start_line == related.line
                        && test.assertions.is_empty()
                        && !is_ignored(test)
                        && expects_owner_panic(test, &owner_literals)
                })
        })
        .map(|related| {
            format!(
                "`{}` ({}:{})",
                related.name,
                related.file.display().to_string().replace('\\', "/"),
                related.line
            )
        })
        .collect();
    if panicking.is_empty() {
        return false;
    }
    let next_step = format!(
        "{} calls this code and passes only if something panics, and ripr does not resolve whether the panic comes from the changed code or from an earlier call, so it does not report a gap. Check that the test's input reaches the changed check and that no earlier call can panic first; add a test only if either fails.",
        panicking.join(", ")
    );
    withhold_gap(
        finding,
        PANIC_LIMIT,
        PANIC_FIRST_UNRESOLVED_EDGE,
        next_step,
        &panicking,
    );
    true
}

/// The byte ranges of the bracketed arguments of every panicking macro
/// invoked at or after byte `from` of the masked body: `name!(..)`,
/// `name![..]` or `name!{..}` (`std::panic!` and `core::panic!` included;
/// `debug_assert!` is not `assert!`, and `todo != 0` is a comparison). An
/// invocation whose brackets do not close in the body is left out.
fn panic_macro_arguments(masked: &str, from: usize) -> Vec<std::ops::Range<usize>> {
    let boundary = |ch: char| !(ch.is_alphanumeric() || ch == '_');
    let mut arguments = Vec::new();
    for name in PANIC_MACROS {
        for (at, _) in masked.match_indices(name).filter(|(at, _)| *at >= from) {
            if !masked[..at].chars().next_back().is_none_or(boundary) {
                continue;
            }
            let after_name = &masked[at + name.len()..];
            let Some(after_bang) = after_name.trim_start().strip_prefix('!') else {
                continue;
            };
            let open = masked.len() - after_bang.trim_start().len();
            if !masked[open..].starts_with(['(', '[', '{']) {
                continue;
            }
            let mut depth = 0usize;
            for (offset, ch) in masked[open..].char_indices() {
                match ch {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => {
                        depth -= 1;
                        if depth == 0 {
                            arguments.push(open..open + offset + 1);
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    arguments
}

/// Whether `test` carries `#[ignore]`, so a default run never executes it.
pub(super) fn is_ignored(test: &TestFact) -> bool {
    test.attrs.iter().any(|attr| {
        let compact: String = attr.chars().filter(|ch| !ch.is_whitespace()).collect();
        compact == "#[ignore]" || compact.starts_with("#[ignore=")
    })
}

/// Whether `test` is a `#[should_panic]` test whose expected panic the
/// owner can raise: a bare attribute, `expected = ""` (which matches any
/// message), or `expected = ".."` whose text is in one of the owner's panic
/// message literals. An attribute ripr cannot read keeps the gap.
fn expects_owner_panic(test: &TestFact, owner_literals: &[String]) -> bool {
    test.attrs.iter().any(|attr| {
        let compact: String = attr.chars().filter(|ch| !ch.is_whitespace()).collect();
        if compact == "#[should_panic]" {
            return true;
        }
        let Some(arguments) = compact
            .strip_prefix("#[should_panic(expected=")
            .and_then(|rest| rest.strip_suffix(")]"))
        else {
            return false;
        };
        // Read the literal from the original text so inner spaces survive.
        if !(arguments.starts_with('"') && arguments.ends_with('"')) {
            return false;
        }
        match string_literal_contents(attr).as_slice() {
            [expected] => {
                owner_literals
                    .iter()
                    .any(|literal| literal.contains(expected.as_str()))
                    || expected.is_empty()
            }
            _ => false,
        }
    })
}

fn gap_rests_on_refusal(finding: &Finding) -> bool {
    // A producer that already named a limit owns the finding's explanation.
    finding.static_limit_kind.is_none()
        && finding.class == ExposureClass::ReachableUnrevealed
        && finding.ripr.reveal.observe.state == StageState::No
        && finding.ripr.reveal.observe.summary == ASSERTION_CONTEXT_UNESTABLISHED
}

/// Refusal scope is computed per finding, so every related test is a
/// candidate source of the refused assertions. The rows are already ranked
/// strongest relation first.
fn judged_tests(finding: &Finding) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for test in &finding.related_tests {
        let label = format!(
            "`{}` ({}:{})",
            test.name,
            test.file.display().to_string().replace('\\', "/"),
            test.line
        );
        if !names.contains(&label) {
            names.push(label);
        }
        if names.len() == JUDGED_TEST_LIMIT {
            break;
        }
    }
    names
}

fn last_established_edge(finding: &Finding, judged: &[String]) -> String {
    if judged.is_empty() {
        format!(
            "reach {}: {}",
            finding.ripr.reach.state.as_str(),
            finding.ripr.reach.summary
        )
    } else {
        format!("related tests {}", judged.join(", "))
    }
}

fn next_step(judged: &[String], macro_name: &str) -> String {
    let tests = if judged.is_empty() {
        "the tests that call this code".to_string()
    } else {
        judged.join(", ")
    };
    format!(
        "ripr could not read whether the assertions in {tests} run as the standard `{macro_name}!`, so it does not report a gap. Check that one of them compares the changed value; add a test only if none does."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn stage(state: StageState, summary: &str) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, summary)
    }

    fn related(name: &str) -> RelatedTest {
        RelatedTest {
            name: name.to_string(),
            file: PathBuf::from("src/lib.rs"),
            line: 40,
            oracle: None,
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::None,
            relation_reason: Some(RelationReason::DirectOwnerCall),
            relation_confidence: Some(RelationConfidence::High),
            miss: None,
        }
    }

    /// The serde `format_u8` shape: a feature-gated test whose loop
    /// `assert_eq!` was refused, marked by the classifier.
    fn marked() -> Finding {
        let yes = || stage(StageState::Yes, "yes");
        let mut activation = ActivationEvidence::default();
        activation
            .missing_discriminators
            .push(MissingDiscriminatorFact {
                value: "n == 100".to_string(),
                reason: "predicate boundary equality is not asserted".to_string(),
                flow_sink: None,
            });
        Finding {
            id: "probe:gap-admission".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:gap-admission".to_string()),
                location: SourceLocation::new("src/lib.rs", 10, 1),
                owner: None,
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: None,
                expression: "n > 99".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::ReachableUnrevealed,
            ripr: RiprEvidence {
                reach: stage(StageState::Yes, "Related tests appear to reach owner"),
                infect: yes(),
                propagate: yes(),
                reveal: RevealEvidence {
                    observe: stage(StageState::No, ASSERTION_CONTEXT_UNESTABLISHED),
                    discriminate: stage(StageState::No, "unestablished"),
                },
            },
            confidence: 0.79,
            evidence: vec![
                "assertion not credited: `assert_eq!` in test_format_u8 at src/lib.rs:41: the test carries `#[cfg(feature = \"std\")]`".to_string(),
                REFUSALS_ARE_ANALYZER_LIMITS.to_string(),
            ],
            missing: vec!["No relevant oracle was detected".to_string()],
            flow_sinks: Vec::new(),
            activation,
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests: vec![related("test_format_u8")],
            recommended_next_step: None,
            language: Some(LanguageId::Rust),
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: SourceCurrentness::CandidateCurrent,
        }
    }

    #[test]
    fn analyzer_limit_refusals_withhold_the_gap_and_name_the_test() {
        let mut finding = marked();
        assert!(withhold_unsupported_gap(&mut finding));
        assert_eq!(finding.class, ExposureClass::StaticUnknown);
        assert_eq!(finding.static_limit_kind, Some(LIMIT));
        assert_eq!(
            finding.stop_reasons,
            vec![StopReason::GapEvidenceUnresolved]
        );
        assert_eq!(finding.missing, vec![LIMIT.describe().to_string()]);
        assert!(finding.activation.missing_discriminators.is_empty());
        assert!(
            !finding
                .evidence
                .iter()
                .any(|line| line == REFUSALS_ARE_ANALYZER_LIMITS),
            "the marker is consumed"
        );
        assert!(
            finding
                .evidence
                .iter()
                .any(|line| line.starts_with("gap_withheld: reachable_unrevealed")),
            "{:?}",
            finding.evidence
        );
        assert_eq!(
            finding.recommended_next_step.as_deref(),
            Some(
                "ripr could not read whether the assertions in `test_format_u8` (src/lib.rs:40) run as the standard `assert_eq!`, so it does not report a gap. Check that one of them compares the changed value; add a test only if none does."
            )
        );
    }

    #[test]
    fn withholding_drops_the_gap_confidence_bonus_and_never_raises_a_score() {
        // reach/infect/propagate yes (0.2 each), observe/discriminate no
        // (0.02 each) score 0.64; reachable_unrevealed adds 0.15.
        let mut scored_as_gap = marked();
        assert!(withhold_unsupported_gap(&mut scored_as_gap));
        assert!((scored_as_gap.confidence - 0.64).abs() < 1e-6);

        let mut already_lower = marked();
        already_lower.confidence = 0.5;
        assert!(withhold_unsupported_gap(&mut already_lower));
        assert!((already_lower.confidence - 0.5).abs() < 1e-6);
    }

    #[test]
    fn an_unmarked_refusal_keeps_the_gap() {
        // A local-shape refusal (`if` branch, uncalled closure) leaves no
        // marker: the runtime controls prove those gaps.
        let mut finding = marked();
        finding
            .evidence
            .retain(|line| line != REFUSALS_ARE_ANALYZER_LIMITS);
        assert!(!withhold_unsupported_gap(&mut finding));
        assert_eq!(finding.class, ExposureClass::ReachableUnrevealed);
        assert_eq!(finding.static_limit_kind, None);
    }

    #[test]
    fn a_marker_without_the_refused_observe_keeps_the_gap_and_is_consumed() {
        let mut finding = marked();
        finding.ripr.reveal.observe = stage(
            StageState::No,
            "Related tests were found, but no assertion appears to observe the changed value, error, field, or effect",
        );
        assert!(!withhold_unsupported_gap(&mut finding));
        assert_eq!(finding.class, ExposureClass::ReachableUnrevealed);
        assert!(
            !finding
                .evidence
                .iter()
                .any(|line| line == REFUSALS_ARE_ANALYZER_LIMITS)
        );
    }

    #[test]
    fn an_existing_named_limit_and_non_gap_classes_are_untouched() {
        let mut named = marked();
        named.static_limit_kind = Some(StaticLimitKind::RustMacroWrappedAssertionUnresolved);
        assert!(!withhold_unsupported_gap(&mut named));
        assert_eq!(named.class, ExposureClass::ReachableUnrevealed);

        for class in [
            ExposureClass::Exposed,
            ExposureClass::WeaklyExposed,
            ExposureClass::NoStaticPath,
            ExposureClass::InfectionUnknown,
            ExposureClass::PropagationUnknown,
            ExposureClass::StaticUnknown,
        ] {
            let mut finding = marked();
            finding.class = class.clone();
            assert!(!withhold_unsupported_gap(&mut finding));
            assert_eq!(finding.class, class);
            assert_eq!(finding.static_limit_kind, None);
        }
    }

    #[test]
    fn withholding_twice_adds_one_stop_reason() {
        let mut finding = marked();
        assert!(withhold_unsupported_gap(&mut finding));
        assert!(!withhold_unsupported_gap(&mut finding));
        assert_eq!(
            finding.stop_reasons,
            vec![StopReason::GapEvidenceUnresolved]
        );
    }

    const LOG_SOURCE: &str = r##"
pub fn audit(amount: u32) {
    log::info!("audited {}", amount * 7);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Capture(std::sync::Mutex<Vec<String>>);

    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            self.0.lock().unwrap().push(record.args().to_string());
        }
        fn flush(&self) {}
    }

    fn capture() -> &'static Capture {
        let logger: &'static Capture = Box::leak(Box::new(Capture(Default::default())));
        log::set_logger(logger).unwrap();
        logger
    }

    #[test]
    fn audit_logs_the_scaled_amount() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, vec!["audited 42".to_string()]);
    }

    #[test]
    fn audit_runs() {
        audit(6);
    }

    #[test]
    fn audit_then_capture() {
        audit(6);
        let logger = capture();
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, vec!["audited 42".to_string()]);
    }

    #[test]
    fn audit_asserts_elsewhere() {
        let _logger = capture();
        audit(6);
        let total = 6 * 7;
        assert_eq!(total, 42);
    }

    static LOGGER: Capture = Capture(std::sync::Mutex::new(Vec::new()));

    #[test]
    fn audit_direct_install() {
        log::set_logger(&LOGGER).unwrap();
        audit(6);
        assert_eq!(*LOGGER.0.lock().unwrap(), vec!["audited 42".to_string()]);
    }

    fn capture_once() -> &'static Capture {
        static ONCE: std::sync::OnceLock<&'static Capture> = std::sync::OnceLock::new();
        ONCE.get_or_init(|| {
            let logger: &'static Capture = Box::leak(Box::new(Capture(Default::default())));
            let _ = log::set_logger(logger);
            logger
        })
    }

    #[test]
    fn audit_through_once_lock() {
        let logger = capture_once();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert!(lines.iter().any(|l| l == "audited 42"), "{lines:?}");
    }

    #[test]
    fn audit_again_after_late_capture() {
        audit(1);
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, vec!["audited 42".to_string()]);
    }

    #[test]
    fn audit_counts_lines() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines.len(), "audited 42".len());
    }

    #[test]
    fn audit_checks_line_width() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines.iter().next().map(String::len), Some(10));
    }

    #[test]
    fn audit_checks_install_result() {
        let installed = log::set_logger(&LOGGER);
        audit(6);
        assert_eq!(installed.is_ok(), true);
    }

    fn silence() -> &'static str {
        log::set_logger(&LOGGER).ok();
        "six"
    }

    struct Plain(std::sync::Mutex<Vec<String>>);

    static RAW: Plain = Plain(std::sync::Mutex::new(Vec::new()));

    fn install_plain() -> &'static Plain {
        log::set_logger(&RAW).ok();
        &RAW
    }

    #[test]
    fn audit_with_plain_store() {
        let store = install_plain();
        audit(6);
        assert_eq!(*store.0.lock().unwrap(), vec!["audited 42".to_string()]);
    }

    #[test]
    fn audit_with_helper_value() {
        let word = silence();
        audit(6);
        assert_eq!(word, "six");
    }

    mod other {
        pub fn audit(_amount: u32) {}
    }

    #[test]
    fn audit_path_call() {
        let logger = capture();
        other::audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, vec!["audited 42".to_string()]);
    }

    #[test]
    fn audit_asserts_before_call() {
        let logger = capture();
        assert_eq!(*logger.0.lock().unwrap(), vec!["audited 42".to_string()]);
        audit(6);
    }

    #[test]
    fn audit_counts_through_a_binding() {
        let logger = capture();
        audit(6);
        let n = logger.0.lock().unwrap().len();
        assert_eq!(n.to_string(), "audited 1");
    }

    #[test]
    fn audit_checks_prefix() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines[0].starts_with("audited "), true);
    }

    #[test]
    fn audit_shadows_the_store() {
        let logger = capture();
        audit(6);
        let logger = 5;
        assert_eq!(logger.to_string(), "audited 5");
    }

    #[test]
    fn audit_quote_in_comment() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, expected_lines() /* "audited " */);
    }

    #[test]
    fn audit_through_super() {
        let logger = capture();
        super::audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, vec!["audited 42".to_string()]);
    }

    #[test]
    fn audit_not_equal() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_ne!(lines, vec!["audited 0".to_string()]);
    }

    #[test]
    fn audit_raw_quoted() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, vec![r#"before "audited 42" after"#.to_string()]);
    }

    #[test]
    fn audit_cooked_quoted() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, vec!["before \"audited 42\" after".to_string()]);
    }

    #[test]
    fn audit_split_literals() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, vec!["audited 42".to_string(), ": ok".to_string()]);
    }

    #[test]
    fn audit_nested_comment() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, vec!["other" /* a /* b */ "audited 42" */]);
    }

    static UNRELATED: Capture = Capture(std::sync::Mutex::new(Vec::new()));

    fn capture_other() -> &'static Capture {
        log::set_logger(&LOGGER).ok();
        &UNRELATED
    }

    #[test]
    fn audit_with_other_logger() {
        let logger = capture_other();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, vec!["audited 42".to_string()]);
    }

    #[test]
    fn audit_diagnostic_only() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert_eq!(lines, Vec::<String>::new(), "audited ");
    }

    #[test]
    fn audit_relational() {
        let logger = capture();
        audit(6);
        let lines = logger.0.lock().unwrap().clone();
        assert!(lines.contains(&"audited 42".to_string()));
    }
}
"##;

    fn log_index() -> RustIndex {
        let facts = crate::analysis::rust_index::summarize_file(
            PathBuf::from("src/lib.rs"),
            LOG_SOURCE.to_string(),
        );
        let mut index = RustIndex::default();
        index.extend_functions(facts.functions.iter().cloned());
        index.extend_tests(facts.tests.iter().cloned());
        index.insert_file_only(PathBuf::from("src/lib.rs"), facts);
        index
    }

    /// The cell-0096 shape: a weakly exposed `log::info!` record with one
    /// related test of the given name.
    fn log_finding(test: &str, expression: &str) -> Finding {
        let mut finding = marked();
        finding.class = ExposureClass::WeaklyExposed;
        finding.probe.family = ProbeFamily::SideEffect;
        finding.probe.expression = expression.to_string();
        finding.evidence.clear();
        finding.related_tests = vec![related(test)];
        finding
    }

    const AUDIT_LOG: &str = "log::info!(\"audited {}\", amount * 7);";

    #[test]
    fn a_captured_log_assertion_withholds_the_gap_under_its_own_limit() {
        let index = log_index();
        for test in [
            "audit_logs_the_scaled_amount",
            "audit_direct_install",
            "audit_again_after_late_capture",
            "audit_through_once_lock",
            "audit_through_super",
        ] {
            let mut finding = log_finding(test, AUDIT_LOG);
            assert!(
                withhold_log_capture_gap(&mut finding, &index, Some("audit")),
                "{test}"
            );
            assert_eq!(finding.class, ExposureClass::StaticUnknown, "{test}");
            assert_eq!(
                finding.static_limit_kind,
                Some(StaticLimitKind::RustLogCaptureUnresolved),
                "{test}"
            );
            assert_eq!(
                finding.stop_reasons,
                vec![StopReason::GapEvidenceUnresolved],
                "{test}"
            );
            assert!(finding.activation.missing_discriminators.is_empty());
            assert!(
                finding
                    .recommended_next_step
                    .as_deref()
                    .is_some_and(|step| step.contains(&format!("`{test}`"))),
                "{test}: {:?}",
                finding.recommended_next_step
            );
            assert!(
                finding
                    .evidence
                    .iter()
                    .any(|line| line.contains(LOG_FIRST_UNRESOLVED_EDGE)),
                "{test}"
            );
        }
    }

    #[test]
    fn template_fragments_need_static_text_in_order() {
        assert_eq!(log_template("log::info!(\"{}: {}\", a, b);"), None);
        let template = log_template("log::info!(\"audited {}: ok\", a);");
        assert_eq!(
            template,
            Some(LogTemplate {
                fragments: vec!["audited ".to_string(), ": ok".to_string()],
                anchored_start: true,
                anchored_end: true,
            })
        );
        let matches = |expression: &str, literal: &str| {
            log_template(expression).is_some_and(|template| template.matches(literal))
        };
        let both_ends = "log::info!(\"audited {}: ok\", a);";
        assert!(matches(both_ends, "audited 42: ok"));
        assert!(!matches(both_ends, "unaudited 42: ok"));
        assert!(!matches(both_ends, "audited 42: okay"));
        // A nested macro's template is never read as the record's own.
        let spaced = "log::info! (\"finished {}\", format!(\"audited {}\", n));";
        assert!(matches(spaced, "finished 7"));
        assert!(!matches(spaced, "audited 7"));
        assert_eq!(
            log_template("log::info! { \"finished {}\", format!(\"audited {}\", n) }"),
            None
        );
        // Escapes compare as decoded text, so an edge escape still anchors
        // that edge and a mid-template escape is not a wildcard.
        let quoted_edges = "log::info!(\"\\\"audited {}\\\"\", n);";
        assert!(matches(quoted_edges, "\"audited 42\""));
        assert!(!matches(quoted_edges, "unaudited 42"));
        let newline_end = "log::info!(\"audited {}\\n\", n);";
        assert!(matches(newline_end, "audited 42\n"));
        assert!(!matches(newline_end, "audited 42"));
        assert!(!matches("log::info!(\"audited\\n \");", "audited by bob"));
        assert!(!matches(
            "log::info!(\" \\taudited {}\", n);",
            "unaudited 42"
        ));
        assert!(!matches(
            "log::info!(\"audited\\n{}\", n);",
            "audited by bob"
        ));
        assert!(!matches(
            "log::info!(\"audited\\tuser {}\", n);",
            "audited by user 1"
        ));
        // Whitespace at an edge anchors that edge too.
        assert!(matches(
            "log::info!(\" {} audited {}\", a, b);",
            "  audited 42"
        ));
        assert!(!matches(
            "log::info!(\" {} audited {}\", a, b);",
            "x  audited 42"
        ));
        assert!(matches(
            "log::info!(\"{} audited {} \", a, b);",
            "x audited 42 "
        ));
        assert!(!matches(
            "log::info!(\"{} audited {} \", a, b);",
            "x audited 42 y"
        ));
        // A named or positional argument used twice is not bound; used once,
        // it is an ordinary placeholder.
        assert_eq!(log_template("log::info!(\"{0} audited {0}\", u);"), None);
        assert_eq!(log_template("log::info!(\"{who} audited {who:?}\");"), None);
        assert!(matches(
            "log::info!(\"{who} audited {what}\");",
            "alice audited 4"
        ));
        // Named and positional placeholders together, or a width or precision
        // taken from an argument, may tie two placeholders as well.
        assert_eq!(log_template("log::info!(\"{who} audited {0}\", n);"), None);
        assert_eq!(log_template("log::info!(\"{} audited {x}\", x = v);"), None);
        for template in [
            "{:.*} rows {1}",
            "{:1$} id {}",
            "{:w$} by {w}",
            "{0:>width$} {width}",
        ] {
            let expression = format!("log::info!(\"audit {template}\", a, b);");
            assert_eq!(log_template(&expression), None, "{template}");
        }
        assert!(matches(
            "log::info!(\"{:>8} audited {:.2}\", a, b);",
            "alice audited 4.00"
        ));
        assert_eq!(log_template("log::info!(\"{} audited {0}\", u);"), None);
        assert_eq!(log_template("log::info!(\"{0} audited {:?}\", u);"), None);
        assert!(matches(
            "log::info!(\"{} audited {1}\", a, b);",
            "alice audited bob"
        ));
        // A `format!` template in the assertion is not asserted text.
        assert!(
            string_literal_contents("vec![format!(\"map {}\", m), \"map {}\".into()]")
                .iter()
                .eq(["map {}"])
        );
        assert!(string_literal_contents("format_args!(\"a {}\", b)").is_empty());
        for code in [
            "format!(r\"a {}\", b)",
            "format!(r#\"a {}\"#, b)",
            "std::format! (\"a {}\", b)",
            "format![\"a {}\", b]",
            "format!{\"a {}\", b}",
            "format !(\"a {}\", b)",
            "format!(r##\"a {}\"##, b)",
        ] {
            assert!(string_literal_contents(code).is_empty(), "{code}");
        }
        assert!(
            string_literal_contents("informat(\"a {}\")")
                .iter()
                .eq(["a {}"])
        );
        // An escape that is not decoded leaves the template unbound.
        assert_eq!(log_template("log::info!(\"\\u{61}udited {}\", n);"), None);
        // A raw template is not read; the gap stays.
        assert_eq!(log_template("log::info!(r\"audited {}\", n);"), None);
        assert!(matches(
            "log::info!(\"{} audited {}\", a, b);",
            "x audited 42"
        ));
        assert!(matches("log::info!(\"audited\");", "audited"));
        assert!(!matches("log::info!(\"audited\");", "audited 42"));
        // A cooked template binds the same text in a raw or cooked literal.
        let quoted = "log::info!(\"before \\\"audited {}\\\" after\", amount * 7);";
        assert!(matches(quoted, "before \"audited 42\" after"));
        assert_eq!(
            string_literal_contents(
                "x(r#\"before \"audited 42\" after\"#, \"before \\\"audited 42\\\" after\")"
            ),
            vec!["before \"audited 42\" after".to_string(); 2]
        );
        let ordered = ["audited".to_string(), "42".to_string()];
        assert!(contains_in_order("audited 42", &ordered));
        let reversed = ["42".to_string(), "audited".to_string()];
        assert!(!contains_in_order("audited 42", &reversed));
        let repeated = ["audited".to_string(), "audited".to_string()];
        assert!(!contains_in_order("audited 42", &repeated));
    }

    #[test]
    fn a_targeted_or_keyed_record_binds_its_own_template() {
        let index = log_index();
        for expression in [
            "log::info!(target: \"audit\", \"audited {}\", amount * 7);",
            "log::info!(user = \"bob\"; \"audited {}\", amount * 7);",
            "log::info!(target: \"audit\", \"audited {}\", /* ; */ amount * 7);",
            "log::info!(ch = ';'; \"audited {}\", amount * 7);",
            "log::info!(ch = ','; \"audited {}\", amount * 7);",
            "log::info!(/* lead, ; */ \"audited {}\", amount * 7);",
            "log::info!(target: \"a;b\", user = \"bob\"; \"audited {}\", amount * 7);",
        ] {
            let mut finding = log_finding("audit_logs_the_scaled_amount", expression);
            assert!(
                withhold_log_capture_gap(&mut finding, &index, Some("audit")),
                "{expression}"
            );
        }
        // A raw and a cooked literal of the same text bind alike.
        for test in ["audit_raw_quoted", "audit_cooked_quoted"] {
            let mut quoted = log_finding(
                test,
                "log::info!(\"before \\\"audited {}\\\" after\", amount * 7);",
            );
            assert!(
                withhold_log_capture_gap(&mut quoted, &index, Some("audit")),
                "{test}"
            );
        }
        // The fragments must sit in one literal: a record split across
        // two expected lines is not this record's text.
        let mut split = log_finding(
            "audit_split_literals",
            "log::info!(\"audited {}: ok\", amount * 7);",
        );
        assert!(!withhold_log_capture_gap(&mut split, &index, Some("audit")));
    }

    #[test]
    fn an_uncaptured_or_misordered_log_test_keeps_the_gap() {
        let index = log_index();
        for test in [
            "audit_runs",
            "audit_then_capture",
            "audit_asserts_elsewhere",
            "audit_counts_lines",
            "audit_checks_install_result",
            "audit_with_helper_value",
            "audit_path_call",
            "audit_asserts_before_call",
            "audit_relational",
            "audit_checks_line_width",
            "audit_counts_through_a_binding",
            "audit_checks_prefix",
            "audit_shadows_the_store",
            "audit_quote_in_comment",
            "audit_not_equal",
            "audit_nested_comment",
            "audit_with_other_logger",
            "audit_with_plain_store",
            "audit_diagnostic_only",
        ] {
            let mut finding = log_finding(test, AUDIT_LOG);
            assert!(
                !withhold_log_capture_gap(&mut finding, &index, Some("audit")),
                "{test}"
            );
            assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{test}");
            assert_eq!(finding.static_limit_kind, None, "{test}");
        }
    }

    #[test]
    fn only_a_qualified_log_record_on_a_free_owner_is_withheld() {
        let index = log_index();
        let test = "audit_logs_the_scaled_amount";
        let mut bare = log_finding(test, "info!(\"audited {}\", amount * 7);");
        assert!(!withhold_log_capture_gap(&mut bare, &index, Some("audit")));
        let mut println = log_finding(test, "log::println!(\"{}\", amount * 7);");
        assert!(!withhold_log_capture_gap(
            &mut println,
            &index,
            Some("audit")
        ));
        let mut method = log_finding(test, AUDIT_LOG);
        assert!(!withhold_log_capture_gap(&mut method, &index, None));
        let mut other_owner = log_finding(test, AUDIT_LOG);
        assert!(!withhold_log_capture_gap(
            &mut other_owner,
            &index,
            Some("report")
        ));
        let mut predicate = log_finding(test, AUDIT_LOG);
        predicate.probe.family = ProbeFamily::Predicate;
        assert!(!withhold_log_capture_gap(
            &mut predicate,
            &index,
            Some("audit")
        ));
        let mut gap = log_finding(test, AUDIT_LOG);
        gap.class = ExposureClass::ReachableUnrevealed;
        assert!(!withhold_log_capture_gap(&mut gap, &index, Some("audit")));
        let mut limited = log_finding(test, AUDIT_LOG);
        limited.static_limit_kind = Some(StaticLimitKind::Metaprogramming);
        assert!(!withhold_log_capture_gap(
            &mut limited,
            &index,
            Some("audit")
        ));
        let mut other_record = log_finding(test, "log::info!(\"finished {}\", amount);");
        assert!(!withhold_log_capture_gap(
            &mut other_record,
            &index,
            Some("audit")
        ));
        let mut target_prefix = log_finding(
            test,
            "log::info!(target: \"audited\", \"finished {}\", amount);",
        );
        assert!(!withhold_log_capture_gap(
            &mut target_prefix,
            &index,
            Some("audit")
        ));
        let mut identifier_fragment = log_finding(test, "log::warn!(\"lines\");");
        assert!(!withhold_log_capture_gap(
            &mut identifier_fragment,
            &index,
            Some("audit")
        ));
        let mut out_of_order = log_finding(test, "log::info!(\"42{}audited\", amount);");
        assert!(!withhold_log_capture_gap(
            &mut out_of_order,
            &index,
            Some("audit")
        ));
        let mut inner_text = log_finding(test, "log::info!(\"dited {}\", amount);");
        assert!(!withhold_log_capture_gap(
            &mut inner_text,
            &index,
            Some("audit")
        ));
        let mut punctuation_only = log_finding(test, "log::info!(\"{}: {}\", a, b);");
        assert!(!withhold_log_capture_gap(
            &mut punctuation_only,
            &index,
            Some("audit")
        ));
        let mut no_static_text = log_finding(test, "log::info!(\"{}\", amount * 7);");
        assert!(!withhold_log_capture_gap(
            &mut no_static_text,
            &index,
            Some("audit")
        ));
        let mut name_only = log_finding(test, AUDIT_LOG);
        name_only.related_tests[0].relation_reason = Some(RelationReason::SameTestFile);
        assert!(!withhold_log_capture_gap(
            &mut name_only,
            &index,
            Some("audit")
        ));
        let mut tracing = log_finding(test, "tracing::warn!(\"audited {}\", amount * 7);");
        assert!(!withhold_log_capture_gap(
            &mut tracing,
            &index,
            Some("audit")
        ));
    }

    #[test]
    fn a_panic_macro_is_read_by_its_own_name() {
        let spans = |code: &str| {
            panic_macro_arguments(code, 0)
                .into_iter()
                .map(|range| code[range].to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(spans("panic!(f(1));"), ["(f(1))"]);
        assert_eq!(spans("std::panic! [x];"), ["[x]"]);
        assert_eq!(spans("assert!(cols > 0);"), ["(cols > 0)"]);
        assert_eq!(spans("debug_assert!(cols > 0);"), ["(cols > 0)"]);
        for code in [
            "let panic = 1;",
            "my_panic!(x);",
            "if todo != 0 {",
            "let ok = !panic;",
            "panic!(unclosed",
        ] {
            assert!(spans(code).is_empty(), "{code}");
        }
        // Only invocations at or after the start offset count.
        assert!(panic_macro_arguments("panic!(a); x", 3).is_empty());
    }

    const PANIC_SOURCE: &str = r#"pub fn row(index: usize) -> usize {
    if index >= 100 {
        panic!("index past the end");
    }
    index / 4
}

#[test]
#[should_panic]
fn bare() { row(100); }

#[test]
#[should_panic(expected = "past the")]
fn owned_message() { row(100); }

#[test]
#[should_panic(expected = "grid needs columns")]
fn foreign_message() { row(100); }

#[test]
#[should_panic(expected = "")]
fn empty_message() { row(100); }

#[test]
fn plain() { row(100); }
"#;

    #[test]
    fn a_should_panic_test_expects_the_owner_panic_only_when_the_message_matches() {
        let facts = crate::analysis::rust_index::summarize_file(
            PathBuf::from("src/lib.rs"),
            PANIC_SOURCE.to_string(),
        );
        let owner_literals = vec!["index past the end".to_string()];
        let expects = |name: &str| {
            facts
                .tests
                .iter()
                .find(|test| test.name == name)
                .map(|test| expects_owner_panic(test, &owner_literals))
        };
        assert_eq!(expects("bare"), Some(true));
        assert_eq!(expects("owned_message"), Some(true));
        assert_eq!(expects("foreign_message"), Some(false));
        assert_eq!(expects("empty_message"), Some(true));
        assert_eq!(expects("plain"), Some(false));
    }

    /// A test whose one line holds `calls` distinct callees.
    fn budget_index(calls: usize) -> (RustIndex, RelatedTest) {
        let line: String = (0..calls).map(|i| format!("h{i}();")).collect();
        let facts = crate::analysis::rust_index::summarize_file(
            PathBuf::from("src/lib.rs"),
            format!("#[test]\nfn wide() {{\n    {line}\n}}\n"),
        );
        let mut related = related("wide");
        related.line = facts
            .tests
            .iter()
            .find(|test| test.name == "wide")
            .map_or(0, |test| test.start_line);
        let mut index = RustIndex::default();
        index.extend_functions(facts.functions.iter().cloned());
        index.extend_tests(facts.tests.iter().cloned());
        index.insert_file_only(PathBuf::from("src/lib.rs"), facts);
        (index, related)
    }

    #[test]
    fn a_related_test_line_at_the_call_budget_withholds_a_gap_and_names_an_unknown() {
        let budget = crate::analysis::extract::MAX_CALL_FACTS_PER_LINE;
        let finding_for = |class: ExposureClass, related: &RelatedTest| {
            let mut finding = marked();
            finding.class = class;
            finding.evidence.clear();
            finding.related_tests = vec![related.clone()];
            finding
        };
        // One under the budget: nothing may have been dropped.
        let (under, related_under) = budget_index(budget - 1);
        let mut finding = finding_for(ExposureClass::ReachableUnrevealed, &related_under);
        assert!(!withhold_call_budget_gap(&mut finding, &under));
        assert_eq!(finding.class, ExposureClass::ReachableUnrevealed);
        assert_eq!(finding.static_limit_kind, None);

        // Wide fixtures hold exactly the budget, then past it.
        for calls in [budget, budget + 40] {
            let (index, related) = budget_index(calls);
            // Gap classes become `static_unknown` and drop the absence claim.
            for class in [
                ExposureClass::ReachableUnrevealed,
                ExposureClass::WeaklyExposed,
            ] {
                let mut finding = finding_for(class, &related);
                assert!(withhold_call_budget_gap(&mut finding, &index), "{calls}");
                assert_eq!(finding.class, ExposureClass::StaticUnknown);
                assert_eq!(
                    finding.static_limit_kind,
                    Some(StaticLimitKind::RustCallFactBudget)
                );
                assert!(finding.activation.missing_discriminators.is_empty());
                assert!(
                    finding
                        .stop_reasons
                        .contains(&StopReason::GapEvidenceUnresolved)
                );
            }
            // Unknown classes keep their class and gain the reason.
            for class in [
                ExposureClass::InfectionUnknown,
                ExposureClass::PropagationUnknown,
                ExposureClass::StaticUnknown,
            ] {
                let mut finding = finding_for(class.clone(), &related);
                finding.missing = vec![
                    "No relevant oracle was detected".to_string(),
                    "No strong discriminator was detected".to_string(),
                    "No exact error variant discriminator was detected".to_string(),
                    "No detected boundary input for the changed predicate".to_string(),
                    format!("{MISSING_DISCRIMINATOR_VALUE_PREFIX}n == 100"),
                    "keep this unrelated line".to_string(),
                ];
                finding.recommended_next_step = Some("Add a test with input n == 100".to_string());
                assert!(!finding.activation.missing_discriminators.is_empty());
                finding.related_tests[0].miss = Some(RelatedTestMiss::NoAssertion);
                assert!(withhold_call_budget_gap(&mut finding, &index), "{calls}");
                assert_eq!(finding.class, class);
                assert_eq!(
                    finding.static_limit_kind,
                    Some(StaticLimitKind::RustCallFactBudget)
                );
                // Call-derived absence claims are dropped or qualified; the
                // rest of the missing evidence stays.
                assert!(
                    finding
                        .missing
                        .contains(&"keep this unrelated line".to_string())
                );
                assert!(finding.missing.iter().all(|line| !line.starts_with("No ")));
                assert!(finding.missing.iter().any(|line| line.contains("per line")));
                assert!(finding.activation.missing_discriminators.is_empty());
                assert!(
                    finding
                        .recommended_next_step
                        .as_deref()
                        .is_some_and(|step| step.contains("per-line call budget")
                            && !step.contains("n == 100"))
                );
                for call_derived in [
                    RelatedTestMiss::NoCallPath,
                    RelatedTestMiss::NoAssertion,
                    RelatedTestMiss::MissingInput,
                    RelatedTestMiss::AssertionNotObserving,
                ] {
                    let mut again = finding_for(class.clone(), &related);
                    again.related_tests[0].miss = Some(call_derived);
                    assert!(withhold_call_budget_gap(&mut again, &index));
                    assert_eq!(
                        again.related_tests[0].miss,
                        Some(RelatedTestMiss::ObservationUnconfirmed),
                        "{call_derived:?}"
                    );
                }
                // An assertion ripr read keeps its strength miss.
                let mut weak = finding_for(class.clone(), &related);
                weak.related_tests[0].miss = Some(RelatedTestMiss::WeakAssertion);
                assert!(withhold_call_budget_gap(&mut weak, &index));
                assert_eq!(
                    weak.related_tests[0].miss,
                    Some(RelatedTestMiss::WeakAssertion)
                );
            }
            // A positive, an absent path and a producer-named limit are left
            // alone: a dropped call never turns them into a gap.
            for class in [ExposureClass::Exposed, ExposureClass::NoStaticPath] {
                let mut finding = finding_for(class.clone(), &related);
                assert!(!withhold_call_budget_gap(&mut finding, &index));
                assert_eq!(finding.class, class);
                assert_eq!(finding.static_limit_kind, None);
            }
            let mut named = finding_for(ExposureClass::ReachableUnrevealed, &related);
            named.static_limit_kind = Some(StaticLimitKind::RustMacroReachUnresolved);
            assert!(!withhold_call_budget_gap(&mut named, &index));
            assert_eq!(named.class, ExposureClass::ReachableUnrevealed);

            // A finding whose related test is a different test is untouched.
            let mut elsewhere = related.clone();
            elsewhere.name = "other".to_string();
            let mut finding = finding_for(ExposureClass::ReachableUnrevealed, &elsewhere);
            assert!(!withhold_call_budget_gap(&mut finding, &index));
            assert_eq!(finding.class, ExposureClass::ReachableUnrevealed);
        }

        // The classifier's marker covers a saturated test the render cap
        // trimmed from the rows; it is consumed either way.
        let mut capped = marked();
        capped.evidence = vec![CALL_BUDGET_SATURATED.to_string()];
        capped.related_tests = Vec::new();
        assert!(withhold_call_budget_gap(&mut capped, &under));
        assert_eq!(capped.class, ExposureClass::StaticUnknown);
        assert!(!capped.evidence.iter().any(|l| l == CALL_BUDGET_SATURATED));
        let mut exposed = marked();
        exposed.class = ExposureClass::Exposed;
        exposed.evidence = vec![CALL_BUDGET_SATURATED.to_string()];
        assert!(!withhold_call_budget_gap(&mut exposed, &under));
        assert!(!exposed.evidence.iter().any(|l| l == CALL_BUDGET_SATURATED));
    }
}
