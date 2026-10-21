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
use crate::analysis::test_grip_evidence::{BindingReach, local_is_owner_result_before, text_calls};
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
            } else if direct && expect_debug_literal(test, &owner.name) {
                Idiom::ExpectDebug
            } else if direct && insta_leaves_field_unredacted(test, changed_text, &owner.name) {
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

/// The owner reads an environment variable by a literal name, the test sets
/// or removes a variable of that same name, and the owner is called after
/// that write: a write that follows the call activates nothing.
fn env_activated(test: &TestFact, owner: &FunctionFact) -> bool {
    // Offsets of `code` are offsets of the original: a write or call that is
    // only commented out or quoted is not there.
    let code = masked(&test.body);
    if !(code.contains("set_var") || code.contains("remove_var")) {
        return false;
    }
    let read = literal_call_names(&owner.body, &["env::var", "env::var_os"]);
    literal_call_sites(&test.body, &["set_var", "remove_var"])
        .iter()
        .filter(|(name, _)| read.contains(name))
        .map(|(_, at)| *at)
        .min()
        .is_some_and(|first_write| calls(&code[first_write..], &owner.name))
}

/// Whether `text` calls `name` (`name(` as a whole identifier).
fn calls(text: &str, name: &str) -> bool {
    text_calls(text, name)
}

/// The string literals passed first to each `callee("..` call in `text`,
/// where `callee` is not the tail of a longer identifier (`reset_var`).
fn literal_call_names(text: &str, callees: &[&str]) -> Vec<String> {
    literal_call_sites(text, callees)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// Like [`literal_call_names`], with the byte offset of each call. Only a
/// call in code counts (a commented or quoted one is masked away); the name
/// is read from the original text at the same offsets, so a real literal name
/// is kept.
fn literal_call_sites(text: &str, callees: &[&str]) -> Vec<(String, usize)> {
    let code = masked(text);
    let mut names = Vec::new();
    for callee in callees {
        for (at, _) in code.match_indices(callee) {
            if code[..at]
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
                names.push((literal[..end].to_string(), at));
            }
        }
    }
    names
}

/// The asserted operand is the owner's result: a call to the owner, or a
/// local whose live binding where the assertion begins (`at`, an offset of the
/// masked `code`) was bound by `let [mut] name = ..owner(..);`. A binding that
/// was replaced, reassigned, closed with its block or made only after `at`
/// does not reach the assertion.
fn derives_from_owner(code: &str, at: usize, operand: &str, owner: &str) -> bool {
    let operand = operand.trim().trim_start_matches('&').trim();
    if calls(operand, owner) {
        return true;
    }
    is_ident(operand)
        && local_is_owner_result_before(
            &code[..at],
            operand,
            owner,
            BindingReach::OwnerCallInInitializer,
        )
}

/// The byte range of the arguments of the macro or call whose `(` follows
/// `from` (after whitespace) in `code`, which has no strings or comments.
fn call_arguments(code: &str, from: usize) -> Option<(usize, usize)> {
    let tail = code.get(from..)?;
    let open = from + (tail.len() - tail.trim_start().len());
    if code.as_bytes().get(open) != Some(&b'(') {
        return None;
    }
    let start = open + 1;
    Some((start, start + macro_arguments(&code[start..])?.len()))
}

/// The byte ranges of the top-level, comma-separated arguments of `code`,
/// which has no strings or comments.
fn split_arguments(code: &str) -> Vec<(usize, usize)> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (offset, ch) in code.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push((start, offset));
                start = offset + 1;
            }
            _ => {}
        }
    }
    parts.push((start, code.len()));
    parts
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
fn expect_debug_literal(test: &TestFact, owner: &str) -> bool {
    let raw: &str = &test.body;
    let code = masked(raw);
    let Some(call) = code.find(".assert_debug_eq(") else {
        return false;
    };
    let Some(open) = code[..call].rfind("expect![") else {
        return false;
    };
    // The asserted value must be the owner's result as bound where the
    // assertion runs.
    let Some((from, to)) = call_arguments(&code, call + ".assert_debug_eq".len()) else {
        return false;
    };
    if !derives_from_owner(&code, call, &code[from..to], owner) {
        return false;
    }
    // The literal is read from the original text at the same offsets.
    raw[open + "expect![".len()..call]
        .trim_start_matches(|ch: char| matches!(ch, '[' | 'r' | '#' | '"') || ch.is_whitespace())
        .starts_with(|ch: char| ch.is_alphanumeric() || matches!(ch, '[' | '(' | '{' | '-'))
}

/// An insta snapshot macro with no inline `@` literal, on a changed struct
/// field initializer that no redaction names. Any other changed line keeps
/// the gap.
fn insta_leaves_field_unredacted(test: &TestFact, changed_text: &str, owner: &str) -> bool {
    let raw: &str = &test.body;
    let code = masked(raw);
    // Redactions and filters set outside the macro are not read.
    if ["add_redaction", "with_settings", "Settings"]
        .iter()
        .any(|name| code.contains(name))
    {
        return false;
    }
    let mut found = false;
    for macro_name in INSTA_MACROS {
        for (at, _) in code.match_indices(macro_name) {
            if !code[..at].chars().next_back().is_none_or(boundary) {
                continue;
            }
            let Some((from, to)) = call_arguments(&code, at + macro_name.len()) else {
                continue;
            };
            let arguments = &code[from..to];
            if arguments.contains('@') {
                continue;
            }
            // The snapshotted value follows an optional literal name; the
            // masked text hides a literal, so the original shows it.
            let parts = split_arguments(arguments);
            let literal_first = |(start, _): (usize, usize)| {
                let first = raw[from + start..to].trim_start();
                first.starts_with('"')
                    || first
                        .strip_prefix('r')
                        .is_some_and(|rest| rest.trim_start_matches('#').starts_with('"'))
            };
            let (start, end) = match parts.as_slice() {
                [first, second, ..] if literal_first(*first) => *second,
                [first, ..] => *first,
                [] => continue,
            };
            if !derives_from_owner(&code, at, &arguments[start..end], owner) {
                continue;
            }
            // Only a struct field the snapshot serializes: a changed
            // predicate or arm shapes the whole rendering, which the
            // snapshot weakly checks as before.
            let Some(field) = field_name(changed_text) else {
                continue;
            };
            if redaction_selectors(&raw[from..to])
                .iter()
                .any(|selector| selector.split(boundary).any(|s| s == field))
            {
                continue;
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

    /// `derives_from_owner` at the end of `body`, as masked code.
    fn derives(body: &str, operand: &str) -> bool {
        let code = masked(body);
        derives_from_owner(&code, code.len(), operand, "row")
    }

    #[test]
    fn an_operand_derives_from_the_owner_by_call_or_binding() {
        let body = "let out = row(1); let other = 5;";
        assert!(derives(body, "row(2)"));
        assert!(derives(body, "&out"));
        assert!(derives("let mut out = row(1);", "out"));
        assert!(!derives(body, "other"));
        assert!(!derives(body, "unbound"));
        assert!(!derives(body, "prefix_row(2)"));
    }

    #[test]
    fn a_binding_must_reach_the_assertion_to_derive_from_the_owner() {
        for replaced in [
            "let out = row(1); let out = vec![1];",
            "let out = row(1);\nlet out = vec![1];",
            "let mut out = row(1); out = vec![1];",
            "let mut out = row(1); out += 1;",
            "let out = row(1); if let Some(out) = pick() {",
            "let out = vec![1]; { let out = row(1); }",
            "let out = row(1); { let out = vec![1];",
            "let out = row(1); { let out = vec![1]; } let out = 5;",
            "let out = vec![1];",
            "let out = row(1);\napply(|| {\n    let out = vec![1];",
            "let out = row(1);\nlet f = || {\n    if c {\n        let out = vec![1];",
            "let out = row(1);\n#[inline]\nlet out = vec![1];",
            "let mut out = row(1); std::mem::replace(&mut out, vec![1]);",
            "let mut out = row(1);\nrun(|| { out = vec![1]; });",
            "let mut out = row(1);\nlet taken = std::mem::take(&mut out);",
            "let mut out = row(1);\nlet mut y = 0;\n(out, y) = (vec![1], 1);",
            "let out = row(1);\nrun(|out| {",
            "let out = row(1);\nrun(a, |out| {\n    foo();",
            "let out = row(1);\nlet g = |out| {",
            "let out = row(1);\nitems.iter().for_each(|(i, out)| {",
            "let out = row(1); run(|out| {",
            "let out = row(1);\nfoo(if let Some(out) = p {",
            "let out = row(1);\nrun(|| 1, |out| {",
            "let out = row(1);\nrun(a || b, |out| {",
            "let out = row(1);\nrun(move || (), |out| {",
            "let out = row(1);\nrun(|| {\n  a || b;\n}, |out| {",
            "let out = row(1);\nlet r = foo(a || b, |out| {",
            "let out = row(1);\nrun(a, b || c,\n    |out|\n    {",
            "let out = row(1);\nrun(|out|",
            "let out = row(1);\nrun(a | b, |out| {",
            "let out = row(1);\nrun(FLAG | M, move |out| {",
            "let out = row(1);\nlet r = foo(a | b, |out| -> u8 {",
            "let out = row(1);\nrun(|a| a | 1, |out| {",
            "let out = row(1);\nrun(a ^ b | c, |out| {",
            "let out = row(1);\nrun(f(a | b), |out| {",
            "let out = row(1);\nrun([a | b], |out| {",
            "let out = row(1);\nrun(a || b | c, |out| {",
            "let out = row(1);\nfoo(a, b | c,\n |out|\n {",
            "let out = row(1);\nif let Foo { out, .. } = foo() {",
            "let out = row(1);\nif let Some(Foo { out }) = foo() {",
            "let out = row(1);\nmatch p {\n    Foo { out } =>",
            "let out = row(1);\nmatch p {\n    Some(Foo { out }) => {",
            "let out = row(1);\nfor Foo { out } in v {",
            "let out = row(1);\nrun(|Foo { out }| {",
            "let out = row(1);\nlet g = |Foo { out }| 1;",
            "let out = row(1);\nlet out = |out: usize| {",
            "let out = row(1);\nlet out = if let Some(out) = Some(2usize) {",
            "let out = row(1);\nlet r = if let Some(out) = p {",
            "let out = row(1);\nlet out = |out|",
            "let mut out = row(1);\nlet out = (std::mem::replace(&mut out, vec![1]),",
            "let out = row(1);\nrun(|Foo::<u8> { out }| {",
            "let out = row(1);\nrun(|module::Foo::<fn() -> u8> { out }| {",
            "let out = row(1);\nmatch p {\n    (out, y) => {",
            "let out = row(1);\nmatch p {\n    A => 1,\n    (out, _) if c => {",
            "let out = row(1);\nmatch p {\n    Foo {\n        a,\n        out,\n    } => {",
            "let out = row(1);\nmatch p {\n    Foo { out } if matches!(out, 1) => {",
            "let out = row(1);\nif c && let Some(out) = foo() {",
            "let out = row(1);\nfn inner(out: u8) {",
            "let out = row(1);\nimpl Foo {\n    pub fn m(out: u8) {",
            "let mut out = row(1);\nlet c = || out = 5;",
            "let mut out = row(1);\nlet mut inc = || out += 1;",
            "let mut out = row(1);\nlet y = foo(out = 5,",
            "let mut out = row(1);\nf(&mut r#out);",
            "let mut out = row(1);\naddr_of_mut!(out);",
            "let out = row(1);\nmy_macro! { let out = 5; }",
            "let mut out = row(1);\nlet\nc = || out = 5;",
            "let out = row(1);\nlet/**/out = 7;",
            "let mut out = row(1);\nlet mut/**/out = 7;",
            "let mut out = row(1);\ny = out = 5;",
            "let mut out = row(1);\nfoo(y = 2, out = 5);",
            "let mut out = row(1);\nlet z = [y = 2, out = 5];",
            "let mut out = row(1);\naddr_of_mut![out];",
            "let mut out = row(1);\nlet c = &mut/**/out;",
            "let out = row(1);\nfor\tout in v {",
            "let out = row(1);\nfor/**/(a, out) in v {",
            "let out = row(1);\nfn/**/f(out: u8) {",
            "let out = row(1);\nfor(a, out) in v {",
            "let out = row(1);\nfor(out)in v {",
            "let out = row(1);\nfor&out in v {",
            "let out = row(1);\nmacro_rules! m { ($n:ident) => { let $n = 5; }; }\nm!(out);",
            "let out = row(1);\nwhile c && let Some(out) = it.next() {",
            "let out = row(1);\nFoo { out, y } = foo();",
            "let out = row(1);\nSome(out) = foo();",
            "let mut out = row(1);\nout[0] = 5;",
            "let mut out = row(1);\n(*out).y = 5;",
            "let mut out = row(1);\nf(&raw mut out);",
            "let mut out = row(1);\nf(&mut (out));",
            "let mut out = row(1);\nswap(&mut *out, &mut y);",
        ] {
            assert!(!derives(replaced, "out"), "{replaced}");
        }
        // Controls: a closed inner replacement leaves the outer binding.
        for kept in [
            "let out = row(1); { let out = vec![1]; }",
            "let out = row(1); { let other = vec![1]; }",
            "let out = vec![1]; let out = row(1);",
            "let out = vec![1];\nlet out = row(1);",
            "let out = Wrap { items: vec![row(1)] };",
            "let out = row(1);\napply(|| {\n    let out = vec![1];\n});",
            "let out = row(1);\nlet f = || {\n    let other = vec![1];\n};",
            "let out = row(1);\nrun(|other| {",
            "let out = row(1);\nlet c = || other = 5;",
            "let out = row(1);\nmy_macro! { let other = 5; }",
            "let out = row(1);\nlet out =",
            "let out = row(1);\n#[inline]\nlet out =",
            "let out = row(1);\nlet out = |Foo::<u8> { y }| {",
            "let out = row(1);\nmatch p {\n    (y, z) => {",
            "let out = row(1);\nif c && let Some(y) = foo() {",
            "let out = row(1);\nFoo { y, z } = foo();",
            "let out = row(1);\nif let Foo { other } = foo() {",
            "let out = row(1);\nlet out =",
            "let out = row(1);\nlet out = foo(",
            "let out = row(1);\nmatch p {\n    Foo { other } => {",
            "let out = row(1);\nrun(a | b, |other| {",
            "let out = row(1);\nfoo(if let Some(other) = p {",
            "let out = row(1);\nrun(|| 1, |other| {",
            "let out = row(1);\nrun(|| {\n    foo(out);\n});",
        ] {
            assert!(derives(kept, "out"), "{kept}");
        }
        // The assertion runs before the owner binding is made.
        let body = "let out = vec![1]; check(&out); let out = row(1);";
        let code = masked(body);
        let at = code.find("check").unwrap_or(0);
        assert!(at > 0);
        assert!(!derives_from_owner(&code, at, "out", "row"));
        assert!(derives_from_owner(&code, code.len(), "out", "row"));
    }

    #[test]
    fn a_commented_or_quoted_binding_is_not_a_binding() {
        for hidden in [
            "// let out = row(1);\nlet out = vec![1];",
            "/* a /* b */ let out = row(1); */ let out = vec![1];",
            "let note = r#\"let out = row(1);\"#; let out = vec![1];",
        ] {
            assert!(!derives(hidden, "out"), "{hidden}");
        }
        // The commented line is the only difference from a live binding.
        assert!(derives("let out = row(1); // note\n", "out"));
        // A commented replacement does not replace.
        assert!(derives("let out = row(1);\n// let out = vec![1];", "out"));
    }

    #[test]
    fn commented_or_quoted_writes_are_not_writes() {
        assert!(
            literal_call_sites("// std::env::remove_var(\"APP\");", &["remove_var"]).is_empty()
        );
        assert!(literal_call_sites("/* a /* b */ set_var(\"APP\", 1) */", &["set_var"]).is_empty());
        assert!(literal_call_sites("let n = r#\"set_var(\"APP\")\"#;", &["set_var"]).is_empty());
        assert_eq!(
            literal_call_sites("x(); remove_var(\"APP\");", &["remove_var"]),
            vec![("APP".to_string(), 5)]
        );
    }

    #[test]
    fn arguments_split_at_top_level_commas_only() {
        let text = "a, f(x, y), {k => v}";
        let parts: Vec<&str> = split_arguments(text)
            .into_iter()
            .map(|(from, to)| &text[from..to])
            .collect();
        assert_eq!(parts, vec!["a", " f(x, y)", " {k => v}"]);
    }

    #[test]
    fn names_match_whole_identifiers() {
        assert!(contains_name("to != x", "to"));
        assert!(!contains_name("total != x", "to"));
    }
}
