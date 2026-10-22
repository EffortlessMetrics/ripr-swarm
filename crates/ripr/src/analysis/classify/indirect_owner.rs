//! RIPR-SPEC-0240: an owner a test reaches without a call ripr can place is
//! not a gap.
//!
//! Three source forms leave a test whose oracle is real and whose reading is
//! `weakly_exposed`, a certain "assertion too weak" claim, because ripr sees no
//! `owner(..)` call it can pair with the assertion:
//!
//! - **function value**: the test names the owner without calling it
//!   (`.map(owner)`, `let f: fn(u32) -> u32 = owner; f(2)`), so the call
//!   happens inside machinery ripr does not follow;
//! - **generated owner**: the changed line is an item-position macro
//!   invocation (`def_add!(add_five, 2 + 3);`) that defines the owner, and the
//!   expansion is not source ripr reads;
//! - **executor-wrapped call**: the owner call is the only argument of
//!   `block_on(..)`, a local helper polling a `Future` or a known executor,
//!   and the assertion reads the helper's result.
//!
//! None of these is credited; the gap is withheld under
//! `rust_indirect_owner_call_unresolved`. Like every RIPR-SPEC-0240 rule this
//! only withholds, so the actionability flip stays fail-closed.

use super::gap_admission::{is_ignored, withhold_gap};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::{FunctionFact, TestFact};
use crate::analysis::rust_index::{RustIndex, is_test_file};
use crate::domain::*;

const LIMIT: StaticLimitKind = StaticLimitKind::RustIndirectOwnerCallUnresolved;
const FIRST_UNRESOLVED_EDGE: &str =
    "whether the assertion observes the owner reached without a call ripr can place";

/// Callee names read as running a future to completion even when their
/// definition is not in the workspace (`futures::executor::block_on`,
/// `pollster::block_on`, a runtime's `rt.block_on(..)`).
const KNOWN_EXECUTORS: [&str; 1] = ["block_on"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    FunctionValue,
    Generated,
    ExecutorWrapped,
}

impl Form {
    fn advice(self) -> &'static str {
        match self {
            Form::FunctionValue => {
                "passes the owner as a function value, and ripr does not follow the indirect call to the assertion"
            }
            Form::Generated => {
                "reaches an owner a macro generates, and ripr does not expand the macro"
            }
            Form::ExecutorWrapped => {
                "calls the owner inside an executor helper such as `block_on`, and ripr does not pair the assertion on the helper's result with the owner's call"
            }
        }
    }
}

/// Withhold a gap whose reaching tests use one of the forms above. Returns
/// `true` when the finding was rewritten to `static_unknown`.
pub(in crate::analysis) fn withhold_indirect_owner_gap(
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
    let changed_line = finding.probe.location.line;
    let owner = finding.probe.owner.as_ref().and_then(|owner_id| {
        index.functions().iter().find(|function| {
            function.id == *owner_id
                && function.start_line <= changed_line
                && changed_line <= function.end_line
        })
    });
    let judged: Vec<(String, Form)> = match owner {
        Some(owner) => judged_for_owner(finding, owner, index),
        None => generated_owner(finding, index).into_iter().collect(),
    };
    if judged.is_empty() {
        return false;
    }
    let labels: Vec<String> = judged.iter().map(|(label, _)| label.clone()).collect();
    let mut forms = Vec::<Form>::new();
    for (_, form) in &judged {
        if !forms.contains(form) {
            forms.push(*form);
        }
    }
    let next_step = format!(
        "{} {}, so ripr does not report a gap. Check that the assertion's expected value differs between the old and the new behavior; add a test only if it does not.",
        labels.join(", "),
        forms
            .iter()
            .map(|form| form.advice())
            .collect::<Vec<_>>()
            .join("; and ")
    );
    withhold_gap(finding, LIMIT, FIRST_UNRESOLVED_EDGE, next_step, &labels);
    true
}

fn label(test: &TestFact) -> String {
    format!(
        "`{}` ({}:{})",
        test.name,
        test.file.display().to_string().replace('\\', "/"),
        test.start_line
    )
}

fn judged_for_owner(
    finding: &Finding,
    owner: &FunctionFact,
    index: &RustIndex,
) -> Vec<(String, Form)> {
    let mut judged = Vec::new();
    for test in index.tests() {
        if is_ignored(test) || test.assertions.is_empty() {
            continue;
        }
        let code = mask_comments_and_strings(&test.body);
        // A test that names the owner as a value is a candidate only where it
        // can see the owner: its own file, or an integration test.
        let sees_owner = test.file == owner.file || is_test_file(&test.file);
        if sees_owner && names_owner_as_value(&code, &owner.name) {
            judged.push((label(test), Form::FunctionValue));
            continue;
        }
        let related_direct = finding.related_tests.iter().any(|related| {
            related.name == test.name
                && related.file == test.file
                && related.line == test.start_line
                && related.relation_reason == Some(RelationReason::DirectOwnerCall)
        });
        if related_direct && executor_wraps_owner_call(&code, &owner.name, test, index) {
            judged.push((label(test), Form::ExecutorWrapped));
        }
    }
    judged
}

/// The changed line is an item-position macro invocation: no function in its
/// file holds the line, and the changed expression is `name!(..)`. The tests
/// named are the related ones that call a name the invocation spells.
fn generated_owner(finding: &Finding, index: &RustIndex) -> Option<(String, Form)> {
    let line = finding.probe.location.line;
    let file = &finding.probe.location.file;
    let inside_function = index.functions().iter().any(|function| {
        function.file == *file && function.start_line <= line && line <= function.end_line
    });
    if inside_function {
        return None;
    }
    let expression = mask_comments_and_strings(finding.probe.expression.trim());
    let expression = expression.trim().trim_end_matches(';').trim_end();
    let name_end = expression.find('!')?;
    let name = &expression[..name_end];
    if !is_path(name) {
        return None;
    }
    let rest = expression[name_end + 1..].trim_start();
    if !rest.starts_with(['(', '[', '{']) {
        return None;
    }
    let related = finding
        .related_tests
        .iter()
        .find(|related| related.relation_reason == Some(RelationReason::DirectOwnerCall))
        .or_else(|| finding.related_tests.first())?;
    let test = index.tests().iter().find(|test| {
        test.name == related.name && test.file == related.file && test.start_line == related.line
    })?;
    Some((label(test), Form::Generated))
}

fn is_ident(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(|ch: char| ch.is_ascii_digit())
        && text.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
}

fn is_path(text: &str) -> bool {
    !text.is_empty() && text.split("::").all(is_ident)
}

fn is_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Offsets at which `name` appears as a whole identifier in `code`.
fn identifier_offsets<'a>(code: &'a str, name: &'a str) -> impl Iterator<Item = usize> + 'a {
    code.match_indices(name)
        .map(|(at, _)| at)
        .filter(move |at| {
            code[..*at]
                .chars()
                .next_back()
                .is_none_or(|ch| !is_ident_char(ch))
                && code[at + name.len()..]
                    .chars()
                    .next()
                    .is_none_or(|ch| !is_ident_char(ch))
        })
}

/// The test's code names `name` as a value: not called, not a macro, not a
/// method, field label or definition, and the test binds no local of that
/// name.
fn names_owner_as_value(code: &str, name: &str) -> bool {
    if is_bound_locally(code, name) {
        return false;
    }
    identifier_offsets(code, name).any(|at| {
        let before = code[..at].trim_end();
        let after = code[at + name.len()..].trim_start();
        let defined = before.ends_with("fn") && ends_at_boundary(&before[..before.len() - 2]);
        let method = before.ends_with('.');
        let imported = line_starts_with_use(code, at);
        let called = after.starts_with(['(', '!']) || after.starts_with("::<");
        let field_label = after.starts_with(':') && !after.starts_with("::");
        !(defined || method || imported || called || field_label)
    })
}

fn ends_at_boundary(text: &str) -> bool {
    text.chars().next_back().is_none_or(|ch| !is_ident_char(ch))
}

fn line_starts_with_use(code: &str, at: usize) -> bool {
    let start = code[..at].rfind('\n').map_or(0, |newline| newline + 1);
    let line = code[start..at].trim_start();
    line.starts_with("use ") || line.starts_with("pub use ")
}

/// `let name`, `let mut name`, or a closure/fn parameter list holding `name`
/// make a later mention the local, not the owner.
fn is_bound_locally(code: &str, name: &str) -> bool {
    identifier_offsets(code, name).any(|at| {
        let before = code[..at].trim_end();
        let before = before.strip_suffix("mut").map_or(before, str::trim_end);
        let after = code[at + name.len()..].trim_start();
        let declared = before.ends_with("let") && ends_at_boundary(&before[..before.len() - 3]);
        let parameter = after.starts_with(':')
            && !after.starts_with("::")
            && code[..at].trim_end().ends_with(['(', ',', '|']);
        declared || parameter
    })
}

/// The owner is called as the only argument of an executor: a local helper
/// whose signature mentions `Future`, or a callee named like a known
/// executor, written bare, as a path or as a method.
fn executor_wraps_owner_call(code: &str, owner: &str, test: &TestFact, index: &RustIndex) -> bool {
    identifier_offsets(code, owner).any(|at| {
        let after = &code[at + owner.len()..];
        if !after.trim_start().starts_with('(') {
            return false;
        }
        let blanks = after.len() - after.trim_start().len();
        let Some(call_end) = call_end(code, at + owner.len() + blanks) else {
            return false;
        };
        // The owner call is the whole argument list of the wrapper.
        if !code[call_end..].trim_start().starts_with(')') {
            return false;
        }
        let before = code[..at].trim_end();
        let Some(open) = before.strip_suffix('(') else {
            return false;
        };
        let open = open.trim_end();
        let callee_start = open
            .rfind(|ch: char| !(is_ident_char(ch) || ch == ':'))
            .map_or(0, |at| at + 1);
        let callee = &open[callee_start..];
        let leaf = callee.rsplit("::").next().unwrap_or(callee);
        if !is_ident(leaf) || leaf == owner {
            return false;
        }
        if KNOWN_EXECUTORS.contains(&leaf) {
            return true;
        }
        // A local helper: defined in the test's file, taking a future.
        callee == leaf
            && !open[..callee_start].trim_end().ends_with('.')
            && index.functions().iter().any(|function| {
                function.name == leaf
                    && function.file == test.file
                    && signature(function).contains("Future")
            })
    })
}

/// The text of a function up to its body's opening brace.
fn signature(function: &FunctionFact) -> String {
    let body: &str = &function.body;
    let masked = mask_comments_and_strings(body);
    masked
        .find('{')
        .map_or(masked.as_str(), |open| &masked[..open])
        .to_string()
}

/// Offset just past the `)` that closes the call whose `(` starts at `open`.
fn call_end(code: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in code[open..].char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_value_is_named_and_a_call_is_not() {
        let code = "assert_eq!(v.into_iter().map(inc).collect::<Vec<_>>(), w);";
        assert!(names_owner_as_value(code, "inc"));
        assert!(!names_owner_as_value("assert_eq!(inc(1), 2);", "inc"));
        assert!(!names_owner_as_value("assert_eq!(inc (1), 2);", "inc"));
        assert!(!names_owner_as_value("inc!(1);", "inc"));
        assert!(!names_owner_as_value("x.inc;", "inc"));
        assert!(!names_owner_as_value("let inc = 3; use_it(inc);", "inc"));
        assert!(!names_owner_as_value("use super::inc;\n", "inc"));
        assert!(!names_owner_as_value("S { inc: 1 };", "inc"));
        assert!(!names_owner_as_value("let increment = 3;", "inc"));
    }

    #[test]
    fn a_function_pointer_binding_is_a_value() {
        let code = "let f: fn(u32) -> u32 = call_through_pointer; assert_eq!(f(2), 6);";
        assert!(names_owner_as_value(code, "call_through_pointer"));
    }

    #[test]
    fn a_local_with_the_owner_name_is_not_the_owner() {
        assert!(!names_owner_as_value(
            "let mut inc = 1; inc += 1; keep(inc);",
            "inc"
        ));
        assert!(!names_owner_as_value(
            "let go = |inc: u32| inc + 1; go(1);",
            "inc"
        ));
    }

    #[test]
    fn call_end_matches_nested_delimiters() {
        let code = "add(f(1, 2), [3]) + 1";
        assert_eq!(call_end(code, 3), Some(17));
        assert_eq!(call_end("add(1", 3), None);
    }

    #[test]
    fn item_macros_need_a_path_and_a_delimiter() {
        assert!(is_path("def_add"));
        assert!(is_path("crate::def_add"));
        assert!(!is_path("2 + 3"));
        assert!(!is_path(""));
    }
}
