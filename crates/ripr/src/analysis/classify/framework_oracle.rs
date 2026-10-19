//! RIPR-SPEC-0240: a test-framework oracle ripr does not evaluate is not a gap.
//!
//! Four test idioms check a changed value through machinery ripr reads only
//! at the surface. Each leaves a related test whose oracle is real and whose
//! reading here is `weakly_exposed`, a certain "assertion too weak" claim:
//!
//! - **environment**: a branch chosen by `std::env::var` is reached only after
//!   the test calls `set_var` or `remove_var`, then asserts an exact value;
//! - **mock closure**: a mockall `.returning(|to, _| ..)` closure reads the
//!   call's arguments, and the test asserts an exact value that flows from it;
//! - **expect-test**: `expect![[".."]].assert_debug_eq(&value)` compares the
//!   whole debug text of the owner's result;
//! - **insta**: `assert_json_snapshot!(value)` compares a snapshot kept in a
//!   committed file ripr does not read, and no redaction names the changed
//!   field.
//!
//! ripr cannot tell whether those oracles notice the change, so it withholds
//! the gap under `rust_framework_oracle_unresolved` and credits nothing. Like
//! every RIPR-SPEC-0240 rule it only withholds, so the actionability flip
//! stays fail-closed.

use super::gap_admission::{is_ignored, withhold_gap};
use crate::analysis::facts::{FunctionFact, TestFact};
use crate::analysis::rust_index::RustIndex;
use crate::domain::*;

const LIMIT: StaticLimitKind = StaticLimitKind::RustFrameworkOracleUnresolved;
const FIRST_UNRESOLVED_EDGE: &str =
    "whether the framework oracle the related test checks notices the changed code";
const INSTA_MACROS: [&str; 6] = [
    "assert_snapshot!",
    "assert_debug_snapshot!",
    "assert_json_snapshot!",
    "assert_yaml_snapshot!",
    "assert_ron_snapshot!",
    "assert_compact_json_snapshot!",
];

/// The framework idiom a related test checks the owner with.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Idiom {
    Environment,
    MockClosure,
    ExpectDebug,
    InstaSnapshot,
}

impl Idiom {
    fn advice(self) -> &'static str {
        match self {
            Idiom::Environment => {
                "sets or removes an environment variable this code reads and asserts an exact value, and ripr does not follow that activation to the changed branch"
            }
            Idiom::MockClosure => {
                "sets a mock expectation whose closure reads the call's arguments and asserts an exact value, and ripr does not evaluate that closure"
            }
            Idiom::ExpectDebug => {
                "compares the owner's result with an expect-test debug literal, and ripr does not compare that literal with the changed value"
            }
            Idiom::InstaSnapshot => {
                "compares the owner's result with an insta snapshot kept outside the source, and no redaction names the changed field"
            }
        }
    }
}

/// Withhold a gap whose related tests check the owner through a framework
/// idiom above. Returns `true` when the finding was rewritten to
/// `static_unknown`.
pub(in crate::analysis) fn withhold_framework_oracle_gap(
    finding: &mut Finding,
    index: &RustIndex,
) -> bool {
    if finding.static_limit_kind.is_some()
        || !matches!(
            finding.class,
            ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
        )
    {
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
    let family = finding.probe.family.clone();
    let changed_text = owner
        .body
        .lines()
        .nth(changed_line - owner.start_line)
        .unwrap_or("");
    let judged: Vec<(String, Idiom)> = finding
        .related_tests
        .iter()
        .filter_map(|related| {
            let test = index.tests().iter().find(|test| {
                test.name == related.name
                    && test.file == related.file
                    && test.start_line == related.line
            })?;
            if is_ignored(test) {
                return None;
            }
            let strong = test
                .assertions
                .iter()
                .any(|assertion| assertion.strength == OracleStrength::Strong);
            let direct = related.relation_reason == Some(RelationReason::DirectOwnerCall);
            let idiom = if strong
                && direct
                && matches!(
                    family,
                    ProbeFamily::MatchArm | ProbeFamily::Predicate | ProbeFamily::ErrorPath
                )
                && env_activated(test, owner)
            {
                Idiom::Environment
            } else if strong && mock_closure_reads_arguments(test, changed_text) {
                Idiom::MockClosure
            } else if direct && expect_debug_literal(test) {
                Idiom::ExpectDebug
            } else if direct && insta_leaves_field_unredacted(test, changed_text) {
                Idiom::InstaSnapshot
            } else {
                return None;
            };
            let label = format!(
                "`{}` ({}:{})",
                related.name,
                related.file.display().to_string().replace('\\', "/"),
                related.line
            );
            Some((label, idiom))
        })
        .collect();
    if judged.is_empty() {
        return false;
    }
    let labels: Vec<String> = judged.iter().map(|(label, _)| label.clone()).collect();
    let advice = judged
        .iter()
        .fold(Vec::<Idiom>::new(), |mut seen, (_, idiom)| {
            if !seen.contains(idiom) {
                seen.push(*idiom);
            }
            seen
        });
    let next_step = format!(
        "{} {}, so ripr does not report a gap. Check that the test's setup reaches the changed code and that its expectation differs between the old and the new behavior; add a test only if either fails.",
        labels.join(", "),
        advice
            .iter()
            .map(|idiom| idiom.advice())
            .collect::<Vec<_>>()
            .join("; and ")
    );
    withhold_gap(finding, LIMIT, FIRST_UNRESOLVED_EDGE, next_step, &labels);
    true
}

fn masked(text: &str) -> String {
    crate::analysis::extract::mask_comments_and_strings(text)
}

fn boundary(ch: char) -> bool {
    !(ch.is_alphanumeric() || ch == '_')
}

fn is_ident(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(|ch: char| ch.is_ascii_digit())
        && text.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
}

/// Whether `needle` appears in `text` as a whole path segment or name.
fn contains_name(text: &str, needle: &str) -> bool {
    text.match_indices(needle).any(|(at, _)| {
        text[..at].chars().next_back().is_none_or(boundary)
            && text[at + needle.len()..]
                .chars()
                .next()
                .is_none_or(boundary)
    })
}

/// The owner reads an environment variable by a literal name and the test
/// sets or removes a variable of that same name.
fn env_activated(test: &TestFact, owner: &FunctionFact) -> bool {
    let body: &str = &test.body;
    if !(body.contains("set_var(") || body.contains("remove_var(")) {
        return false;
    }
    let read = literal_call_names(&owner.body, &["env::var", "env::var_os"]);
    let written = literal_call_names(body, &["set_var", "remove_var"]);
    read.iter().any(|name| written.contains(name))
}

/// The string literals passed first to each `callee("..` call in `text`,
/// where `callee` is not the tail of a longer identifier (`reset_var`).
fn literal_call_names(text: &str, callees: &[&str]) -> Vec<String> {
    let mut names = Vec::new();
    for callee in callees {
        for (at, _) in text.match_indices(callee) {
            if text[..at]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
            {
                continue;
            }
            let Some(after) = text[at + callee.len()..].trim_start().strip_prefix('(') else {
                continue;
            };
            let Some(literal) = after.trim_start().strip_prefix('"') else {
                continue;
            };
            if let Some(end) = literal.find('"') {
                names.push(literal[..end].to_string());
            }
        }
    }
    names
}

/// A mockall expectation whose `returning` closure uses one of its named
/// parameters, on a mocked method the changed line calls: the call's
/// arguments then decide the value the test asserts.
fn mock_closure_reads_arguments(test: &TestFact, changed_text: &str) -> bool {
    let raw: &str = &test.body;
    if !raw.contains(".returning(|") {
        return false;
    }
    let body = masked(raw);
    body.match_indices(".returning(|").any(|(at, marker)| {
        // The mocked method is the `expect_<name>` call that opens the chain.
        let Some(name) = body[..at]
            .rmatch_indices("expect_")
            .find(|(start, _)| body[..*start].chars().next_back().is_none_or(boundary))
            .map(|(start, _)| {
                body[start + "expect_".len()..]
                    .chars()
                    .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                    .collect::<String>()
            })
        else {
            return false;
        };
        if name.is_empty() || !changed_text.contains(&format!(".{name}(")) {
            return false;
        }
        let params_start = at + marker.len();
        let Some(params_end) = body[params_start..].find('|') else {
            return false;
        };
        let rest = &body[params_start + params_end + 1..];
        // The closure ends where `returning(` closes.
        let mut depth = 1usize;
        let closure_end = rest
            .char_indices()
            .find(|(_, ch)| {
                match ch {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
                depth == 0
            })
            .map_or(rest.len(), |(end, _)| end);
        let closure = &rest[..closure_end];
        body[params_start..params_start + params_end]
            .split(',')
            .filter_map(|param| {
                let name = param
                    .trim()
                    .trim_start_matches('&')
                    .trim_start_matches("mut ")
                    .split(':')
                    .next()
                    .unwrap_or("")
                    .trim();
                (is_ident(name) && !name.starts_with('_')).then_some(name)
            })
            .any(|name| contains_name(closure, name))
    })
}

/// `expect![[".."]]....assert_debug_eq(..)` with a non-empty literal. The
/// literal is the expectation, so an empty one is another rule's case.
fn expect_debug_literal(test: &TestFact) -> bool {
    let body: &str = &test.body;
    let Some(call) = body.find(".assert_debug_eq(") else {
        return false;
    };
    let Some(open) = body[..call].rfind("expect![") else {
        return false;
    };
    body[open + "expect![".len()..call]
        .trim_start_matches(|ch: char| matches!(ch, '[' | 'r' | '#' | '"') || ch.is_whitespace())
        .starts_with(|ch: char| ch.is_alphanumeric() || matches!(ch, '[' | '(' | '{' | '-'))
}

/// An insta snapshot macro with no inline `@` literal, on a changed struct
/// field initializer that no redaction names. Any other changed line keeps
/// the gap.
fn insta_leaves_field_unredacted(test: &TestFact, changed_text: &str) -> bool {
    let body: &str = &test.body;
    // Redactions and filters set outside the macro are not read.
    if ["add_redaction", "with_settings", "Settings"]
        .iter()
        .any(|name| body.contains(name))
    {
        return false;
    }
    let mut found = false;
    for macro_name in INSTA_MACROS {
        for (at, _) in body.match_indices(macro_name) {
            if !body[..at].chars().next_back().is_none_or(boundary) {
                continue;
            }
            let after = &body[at + macro_name.len()..];
            let Some(arguments) = after
                .trim_start()
                .strip_prefix('(')
                .and_then(|rest| macro_arguments(rest))
            else {
                return false;
            };
            if arguments.contains('@') {
                return false;
            }
            // Only a struct field the snapshot serializes: a changed
            // predicate or arm shapes the whole rendering, which the
            // snapshot weakly checks as before.
            let Some(field) = field_name(changed_text) else {
                return false;
            };
            if redaction_selectors(arguments)
                .iter()
                .any(|selector| selector.split(boundary).any(|s| s == field))
            {
                return false;
            }
            found = true;
        }
    }
    found
}

/// The text up to the parenthesis that closes a macro call whose `(` was
/// just consumed; `None` when it does not close.
fn macro_arguments(rest: &str) -> Option<&str> {
    let mut depth = 1usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, ch) in rest.char_indices() {
        if in_string {
            match (escaped, ch) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[..offset]);
                }
            }
            _ => {}
        }
    }
    None
}

/// String literals in the arguments that key a redaction (`".id" => ..`).
fn redaction_selectors(arguments: &str) -> Vec<&str> {
    arguments
        .split('"')
        .skip(1)
        .step_by(2)
        .zip(arguments.split('"').skip(2).step_by(2))
        .filter(|(_, after)| after.trim_start().starts_with("=>"))
        .map(|(literal, _)| literal)
        .collect()
}

/// `name` of a struct-literal field initializer line (`name: expr,`).
fn field_name(line: &str) -> Option<&str> {
    let (name, rest) = line.trim().split_once(':')?;
    (!rest.starts_with(':') && is_ident(name.trim())).then_some(name.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_initializer_line_names_its_field_only() {
        assert_eq!(field_name("    items: items.len(),"), Some("items"));
        assert_eq!(field_name("let total = a::b();"), None);
        assert_eq!(field_name("Err(_) => 3,"), None);
        assert_eq!(field_name("std::env::var(\"X\")"), None);
    }

    #[test]
    fn redaction_selectors_are_the_path_literals_of_the_argument_list() {
        assert_eq!(
            redaction_selectors(
                "record, { \".id\" => \"[id]\", \".meta.at\" => \"[t]\", \"[].n\" => 1 }"
            ),
            vec![".id", ".meta.at", "[].n"]
        );
        assert!(redaction_selectors("record").is_empty());
    }

    #[test]
    fn macro_arguments_stop_at_the_matching_parenthesis_outside_strings() {
        assert_eq!(
            macro_arguments("a(\")\"), { \".x\" => \"y\" }); tail"),
            Some("a(\")\"), { \".x\" => \"y\" }")
        );
        assert_eq!(macro_arguments("unclosed("), None);
    }

    fn test_named(source: &str, name: &str) -> Result<TestFact, String> {
        let facts = crate::analysis::rust_index::summarize_file(
            std::path::PathBuf::from("src/lib.rs"),
            source.to_string(),
        );
        facts
            .tests
            .iter()
            .find(|test| test.name == name)
            .cloned()
            .ok_or_else(|| format!("premise: test {name}"))
    }

    #[test]
    fn a_mock_closure_counts_only_when_it_reads_a_named_argument() -> Result<(), String> {
        let source = |closure: &str| {
            format!(
                "#[cfg(test)]
mod tests {{
    #[test]
    fn counts() {{
        let mut mailer = MockMailer::new();
        mailer.expect_send().returning({closure});
        assert_eq!(remind_all(&mailer), 1);
    }}
}}
"
            )
        };
        for (closure, reads) in [
            ("|to, _| to != \"cy\"", true),
            ("|_, body| body.len() > 3", true),
            ("|to: &str, _| to.len() == 2", true),
            ("|_, _| true", false),
            ("|to, _| true", false),
            // The name only appears inside a longer identifier.
            ("|to, _| total > 0", false),
        ] {
            assert_eq!(
                mock_closure_reads_arguments(
                    &test_named(&source(closure), "counts")?,
                    "mailer.send(user)"
                ),
                reads,
                "{closure}"
            );
        }
        // No `expect_` call: not a mock expectation.
        let plain = "#[cfg(test)]
mod tests {
    #[test]
    fn counts() {
        let f = items.returning(|to, _| to != 1);
        assert_eq!(f, 1);
    }
}
";
        assert!(!mock_closure_reads_arguments(
            &test_named(plain, "counts")?,
            "mailer.send(user)"
        ));
        // The changed line calls a different mocked method.
        assert!(!mock_closure_reads_arguments(
            &test_named(&source("|to, _| to != \"cy\""), "counts")?,
            "mailer.deliver(user)"
        ));
        Ok(())
    }

    #[test]
    fn literal_call_names_read_the_first_literal_of_whole_callees() {
        assert_eq!(
            literal_call_names(
                "std::env::var(\"A\"); env::var_os( \"B\" )",
                &["env::var", "env::var_os"]
            ),
            vec!["A".to_string(), "B".to_string()]
        );
        assert!(literal_call_names("reset_var(\"A\"); set_var(NAME, 1)", &["set_var"]).is_empty());
    }

    #[test]
    fn names_match_whole_identifiers() {
        assert!(contains_name("to != x", "to"));
        assert!(!contains_name("total != x", "to"));
    }
}
