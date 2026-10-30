//! Test-local assertion helpers, read as the asserts they run (#9636).
//!
//! A test that calls a same-file `check_x(50, True)` whose body is only
//! `assert x(score) == want` runs exactly the assert the helper holds, with
//! the call's literal arguments in place of the parameters. The test-extraction
//! machinery reads assertions out of a test's own body, so such a test read as
//! "0 related tests" and abstained.
//!
//! Unlike `helper_assertions` (#9419, #9711), this is a same-file,
//! literal-argument, multi-assert source rewrite that runs first, so
//! `helper_assertions` then sees no call to resolve.
//!
//! This pass rewrites the file's source before extraction: each straight-line
//! helper-call statement of a plain test becomes the helper's asserts with the
//! literal arguments substituted, on the same physical line (`;`-joined), so
//! every other line number is unchanged. It is credit-or-unknown only: it
//! produces no new finding kind, and the rewritten asserts go through the
//! ordinary oracle admission, so a weak or unrecognised assert is no stronger
//! than when written inline. Anything it cannot prove stays as written and
//! therefore stays unknown:
//!
//! - the helper is a module-level, undecorated, synchronous `def` with plain
//!   positional parameters (no defaults, `*args`, keywords) whose body is only
//!   `assert` statements (a leading docstring is allowed), and every mention
//!   of its name in the file is its own definition or a direct call;
//! - the call is a top-level statement of a module-level `test*` function
//!   (never under `if`/loops/`with`/`try`), with exactly as many literal
//!   positional arguments as the helper has parameters;
//! - the test takes no parameters and holds only expression and `assert`
//!   statements, so no local name can shadow what the helper's asserts read;
//! - a parameter appears in the helper's asserts only as a plain value (never
//!   as an attribute, keyword, or inside a string) and the asserts hold no
//!   `lambda`, comprehension, walrus, `await` or `yield`;
//! - the file does not rebind names dynamically (`globals`, `setattr`,
//!   `exec`, `eval`, star import).
//!
//! A helper defined in another file, passed as a value, or reached on a
//! conditional path is never inlined (#9419 owns the cross-file case).

use super::source_utils::text_for_range;
use rustpython_parser::ast::{self, Expr, Ranged, Stmt};
use std::collections::BTreeMap;

/// One inlineable helper: its parameters and each assert's source text.
struct Helper {
    parameters: Vec<String>,
    asserts: Vec<String>,
}

/// The source with each provable helper-call statement of a plain test
/// replaced by the helper's substituted asserts, or `None` when nothing
/// qualifies.
pub(super) fn inline_test_helpers(source: &str, body: &[Stmt]) -> Option<String> {
    if file_rebinds_dynamically(source) {
        return None;
    }
    let helpers = collect_helpers(source, body);
    if helpers.is_empty() {
        return None;
    }
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for stmt in body {
        let Stmt::FunctionDef(test) = stmt else {
            continue;
        };
        if !test.name.as_str().starts_with("test")
            || !plain_test(test)
            || text_for_range(source, test.range).contains(":=")
        {
            continue;
        }
        for statement in &test.body {
            let Some((name, arguments)) = helper_call(source, statement) else {
                continue;
            };
            let Some(helper) = helpers.get(name) else {
                continue;
            };
            if helper.parameters.len() != arguments.len() {
                continue;
            }
            let Some(replacement) = substitute_all(helper, &arguments) else {
                continue;
            };
            let range = statement.range();
            let start = usize::from(range.start());
            let end = usize::from(range.end());
            // The statement must be the whole of its physical line(s): a
            // `;`-joined sibling or a trailing comment keeps its own text.
            if !statement_owns_its_lines(source, start, end) {
                continue;
            }
            edits.push((start, end, replacement));
        }
    }
    if edits.is_empty() {
        return None;
    }
    let mut rewritten = source.to_string();
    edits.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    for (start, end, replacement) in edits {
        rewritten.replace_range(start..end, &replacement);
    }
    Some(finish(source, rewritten))
}

/// Test-only seam: a source carrying the marker gets a rewrite that cannot
/// parse, to exercise the caller's fall-back to the original source.
#[cfg(test)]
fn finish(source: &str, rewritten: String) -> String {
    if source.contains("ripr-test-force-parse-failure") {
        format!("{rewritten}\n)(")
    } else {
        rewritten
    }
}

#[cfg(not(test))]
fn finish(_source: &str, rewritten: String) -> String {
    rewritten
}

fn file_rebinds_dynamically(source: &str) -> bool {
    [
        "globals(",
        "setattr(",
        "exec(",
        "eval(",
        "import *",
        "__getattr__",
    ]
    .iter()
    .any(|needle| source.contains(needle))
}

/// A test with no parameters whose top-level statements are only expressions
/// and asserts: nothing in it can rebind a name the helper's asserts read.
fn plain_test(test: &ast::StmtFunctionDef) -> bool {
    test.decorator_list.is_empty()
        && test.args.posonlyargs.is_empty()
        && test.args.args.is_empty()
        && test.args.vararg.is_none()
        && test.args.kwonlyargs.is_empty()
        && test.args.kwarg.is_none()
        && test
            .body
            .iter()
            .all(|stmt| matches!(stmt, Stmt::Expr(_) | Stmt::Assert(_)))
}

fn collect_helpers(source: &str, body: &[Stmt]) -> BTreeMap<String, Helper> {
    let mut helpers = BTreeMap::new();
    for stmt in body {
        let Stmt::FunctionDef(function) = stmt else {
            continue;
        };
        let name = function.name.as_str();
        if name.starts_with("test") || !function.decorator_list.is_empty() {
            continue;
        }
        let args = &function.args;
        if !args.posonlyargs.is_empty()
            || args.vararg.is_some()
            || !args.kwonlyargs.is_empty()
            || args.kwarg.is_some()
            || args.args.iter().any(|arg| arg.default.is_some())
        {
            continue;
        }
        let parameters: Vec<String> = args
            .args
            .iter()
            .map(|arg| arg.def.arg.as_str().to_string())
            .collect();
        let mut asserts = Vec::new();
        let mut plain = true;
        for (index, statement) in function.body.iter().enumerate() {
            match statement {
                Stmt::Assert(assertion) => {
                    asserts.push(text_for_range(source, assertion.range));
                }
                Stmt::Expr(expr)
                    if index == 0
                        && matches!(
                            &*expr.value,
                            Expr::Constant(constant)
                                if matches!(constant.value, ast::Constant::Str(_))
                        ) => {}
                _ => plain = false,
            }
        }
        if !plain || asserts.is_empty() || asserts.iter().any(|text| text.contains('\n')) {
            continue;
        }
        if !every_mention_is_definition_or_direct_call(source, name) {
            continue;
        }
        // A name defined twice is not a unique helper.
        let definitions = body
            .iter()
            .filter(|other| matches!(other, Stmt::FunctionDef(f) if f.name.as_str() == name))
            .count();
        // Any other `def NAME` or `class NAME` (conditional, nested, in a
        // class) also rebinds the name.
        let spelled = source
            .match_indices(name)
            .filter(|(at, _)| {
                let head = source[..*at].trim_end();
                let keyword = if head.ends_with("def") {
                    3
                } else if head.ends_with("class") {
                    5
                } else {
                    return false;
                };
                !head[..head.len() - keyword].ends_with(word_char)
                    && !source[*at + name.len()..].starts_with(word_char)
            })
            .count();
        if definitions != 1 || spelled != 1 {
            continue;
        }
        helpers.insert(
            name.to_string(),
            Helper {
                parameters,
                asserts,
            },
        );
    }
    helpers
}

/// Whole-word mentions of `name`: the `def name` token and `name(` calls only.
fn every_mention_is_definition_or_direct_call(source: &str, name: &str) -> bool {
    let word = |ch: Option<char>| ch.is_some_and(|ch| ch == '_' || ch.is_alphanumeric());
    source.match_indices(name).all(|(at, _)| {
        let before = source[..at].chars().next_back();
        let after = source[at + name.len()..].chars().next();
        if word(before) || word(after) {
            return true;
        }
        let head = source[..at].trim_end();
        let declares = head.ends_with("def") && !head[..head.len() - 3].ends_with(word_char);
        let called = source[at + name.len()..].trim_start().starts_with('(');
        !head.ends_with(['.', '@']) && (declares || called)
    })
}

fn word_char(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

/// `name(arg, ...)` as an expression statement: the helper name and the
/// source text of each literal positional argument. `None` for any other
/// call shape, a keyword or starred argument, or a non-literal argument.
fn helper_call<'a>(source: &str, statement: &'a Stmt) -> Option<(&'a str, Vec<String>)> {
    let Stmt::Expr(expr) = statement else {
        return None;
    };
    let Expr::Call(call) = &*expr.value else {
        return None;
    };
    let Expr::Name(name) = &*call.func else {
        return None;
    };
    if !call.keywords.is_empty() {
        return None;
    }
    let mut arguments = Vec::new();
    for argument in &call.args {
        if !is_literal(argument) {
            return None;
        }
        let text = text_for_range(source, argument.range());
        arguments.push(if matches!(argument, Expr::UnaryOp(_)) {
            format!("({text})")
        } else {
            text
        });
    }
    Some((name.id.as_str(), arguments))
}

fn is_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Constant(constant) => matches!(
            constant.value,
            ast::Constant::None
                | ast::Constant::Bool(_)
                | ast::Constant::Str(_)
                | ast::Constant::Int(_)
                | ast::Constant::Float(_)
        ),
        Expr::UnaryOp(unary) => {
            matches!(unary.op, ast::UnaryOp::USub | ast::UnaryOp::UAdd)
                && matches!(
                    &*unary.operand,
                    Expr::Constant(constant)
                        if matches!(
                            constant.value,
                            ast::Constant::Int(_) | ast::Constant::Float(_)
                        )
                )
        }
        _ => false,
    }
}

fn statement_owns_its_lines(source: &str, start: usize, end: usize) -> bool {
    let line_start = source[..start].rfind('\n').map_or(0, |at| at + 1);
    let line_end = source[end..].find('\n').map_or(source.len(), |at| end + at);
    source[line_start..start].trim().is_empty()
        && source[end..line_end].trim().is_empty()
        && !source[start..end].contains('\n')
}

/// The helper's asserts, `;`-joined, with each parameter replaced by its
/// call-site literal. `None` when any parameter use is not a plain value.
fn substitute_all(helper: &Helper, arguments: &[String]) -> Option<String> {
    let mut parts = Vec::new();
    for text in &helper.asserts {
        parts.push(substitute(text, &helper.parameters, arguments)?);
    }
    Some(parts.join("; "))
}

fn substitute(text: &str, parameters: &[String], arguments: &[String]) -> Option<String> {
    if text.contains(['\\', '`'])
        || ["lambda", " for ", ":=", "await", "yield", "[", "{"]
            .iter()
            .any(|needle| text.contains(needle))
    {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    let mut quote: Option<char> = None;
    while index < text.len() {
        let ch = text[index..].chars().next()?;
        if let Some(open) = quote {
            if ch == open {
                quote = None;
            }
            out.push(ch);
            index += ch.len_utf8();
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
        } else if word_char(ch) {
            let end = text[index..]
                .find(|next: char| !word_char(next))
                .map_or(text.len(), |at| index + at);
            let word = &text[index..end];
            if let Some(position) = parameters.iter().position(|parameter| parameter == word) {
                let before = text[..index].trim_end();
                let after = text[end..].trim_start();
                // An attribute, a keyword argument, or an assignment target
                // is not a plain value read.
                let keyword = after.starts_with('=') && !after.starts_with("==");
                let argument = arguments.get(position)?;
                // A numeric literal cannot be a member object (`5.bit_length()`
                // is a syntax error that would drop the whole file's tests).
                let numeric = argument.starts_with(|c: char| c.is_ascii_digit() || c == '(');
                // A string-prefix letter (`b'abc'`) would fuse with the
                // argument's own quote.
                let prefixed = text[end..].starts_with(['\'', '"']);
                if before.ends_with('.')
                    || keyword
                    || prefixed
                    || (numeric && after.starts_with(['.', '(']))
                {
                    return None;
                }
                out.push_str(argument);
            } else {
                out.push_str(word);
            }
            index = end;
            continue;
        }
        out.push(ch);
        index += ch.len_utf8();
    }
    // A parameter name left inside a string literal would have been rewritten
    // by a naive pass; refuse rather than guess which reading is intended.
    if parameters
        .iter()
        .any(|parameter| word_inside_quotes(text, parameter))
    {
        return None;
    }
    Some(out)
}

fn word_inside_quotes(text: &str, word: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut literal = String::new();
    for ch in text.chars() {
        match quote {
            Some(open) if ch == open => {
                if contains_word(&literal, word) {
                    return true;
                }
                literal.clear();
                quote = None;
            }
            Some(_) => literal.push(ch),
            None if ch == '\'' || ch == '"' => quote = Some(ch),
            None => {}
        }
    }
    false
}

fn contains_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        !text[..at].chars().next_back().is_some_and(word_char)
            && !text[at + word.len()..]
                .chars()
                .next()
                .is_some_and(word_char)
    })
}
