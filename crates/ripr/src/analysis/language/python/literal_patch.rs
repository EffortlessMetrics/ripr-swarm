//! A `patch("a.b.fn", return_value=<literal>)` that cannot hide the edit.
//!
//! `mocked_module` withholds credit because a substituted callable may stand
//! in for the very behavior the test pins. One substitution is safe to
//! look through: a zero-argument call replaced by a literal, when the changed
//! line only does arithmetic or formatting around it. The mock then fixes one
//! input of an expression whose other parts the exact assertion still reads
//! (`f"{prefix}-{int(time.time())}"` under a fixed clock). Everything else
//! keeps the limit: any other mock form, an alias that is not `unittest.mock`,
//! a patched name that the owner calls with arguments or uses as a value, a
//! changed line with control flow, and an owner that is itself patched.

use super::approx_band::parse_expression;
use super::related_tests::is_python_identifier_char;
use super::{PythonImport, PythonOwner, PythonTest, expr_full_name};
use rustpython_parser::ast::{self, Expr};

/// The substituted call spellings (`time.time()`, `time()`) on the changed
/// line when every substitution in `test` is a literal zero-argument stand-in
/// that the edit cannot hide behind, else None. Fails closed: any shape it
/// does not recognize returns None.
pub(super) fn literal_patch_calls(
    test: &PythonTest,
    owner: &PythonOwner,
    owner_source: &str,
    old_line: Option<&str>,
    new_line: &str,
) -> Option<Vec<String>> {
    let old_line = old_line?;
    if owner_source.is_empty()
        || test
            .decorators
            .iter()
            .any(|decorator| mentions_substitution(decorator))
    {
        return None;
    }
    let targets = literal_patch_targets(test)?;
    if targets.is_empty() {
        return None;
    }
    let owner_lines = owner_line_texts(owner, owner_source);
    let owner_segments: Vec<&str> = owner.qualified_name.split('.').collect();
    let mut spellings = Vec::new();
    for target in &targets {
        let segments: Vec<&str> = target.split('.').collect();
        let name = *segments.last()?;
        // The owner (or its class) is itself substituted: the test no longer
        // runs the changed code.
        if segments.len() < 2
            || segments.iter().any(|segment| !is_identifier(segment))
            || segments
                .iter()
                .any(|segment| owner_segments.contains(segment))
        {
            return None;
        }
        let qualified = format!("{}.{name}", segments[segments.len() - 2]);
        // Both sides of the edit are one `return` of the same restricted
        // shape, with the substituted call and its `int(...)` wrapping the
        // same number of times, so the edit cannot sit behind the call.
        let old_shape = restricted_return(old_line, &qualified, name)?;
        let new_shape = restricted_return(new_line, &qualified, name)?;
        // Nothing else in the owner mentions the name: no other line can
        // branch on, or consume, the substituted value.
        let only_on_changed_line =
            owner_lines
                .iter()
                .filter(|line| line.contains(name))
                .all(|line| {
                    let text = line.trim();
                    text == new_line.trim() || text == old_line.trim()
                });
        if old_shape != new_shape || new_shape.0 != 1 || !only_on_changed_line {
            return None;
        }
        spellings.push(format!("{qualified}()"));
        spellings.push(format!("{name}()"));
    }
    Some(spellings)
}

/// `(substituted calls, int-wrapped substituted calls)` of a line that is one
/// `return` of an f-string or `+`/`-` expression over names, int and string
/// literals, the substituted call and `int(<that call>)`. Addition and
/// subtraction keep the edit visible whatever literal the patch returns;
/// products, divisions, `max`/`min`, indexing, comparisons, branches and any
/// other call can absorb it (`x * 0`, `x // 3600`, `max(x, 100)`).
fn restricted_return(line: &str, qualified: &str, name: &str) -> Option<(usize, usize, i32)> {
    let expression = line.trim().strip_prefix("return ")?;
    let expr = parse_expression(expression)?;
    let mut counts = (0, 0, 0);
    restricted_counts(&expr, qualified, name, 1, &mut counts).then_some(counts)
}

fn restricted_counts(
    expr: &Expr,
    qualified: &str,
    name: &str,
    sign: i32,
    counts: &mut (usize, usize, i32),
) -> bool {
    match expr {
        Expr::JoinedStr(joined) => joined.values.iter().all(|part| match part {
            Expr::Constant(_) => true,
            Expr::FormattedValue(formatted) => {
                formatted.format_spec.is_none()
                    && formatted.conversion == ast::ConversionFlag::None
                    && restricted_counts(&formatted.value, qualified, name, sign, counts)
            }
            _ => false,
        }),
        Expr::BinOp(binop) => {
            let right_sign = if matches!(binop.op, ast::Operator::Sub) {
                -sign
            } else {
                sign
            };
            matches!(binop.op, ast::Operator::Add | ast::Operator::Sub)
                && restricted_counts(&binop.left, qualified, name, sign, counts)
                && restricted_counts(&binop.right, qualified, name, right_sign, counts)
        }
        Expr::Name(_) => true,
        Expr::Constant(constant) => matches!(
            constant.value,
            ast::Constant::Int(_) | ast::Constant::Str(_)
        ),
        Expr::Call(call) => {
            if is_substituted_call(call, qualified, name) {
                counts.0 += 1;
                counts.2 += sign;
                return true;
            }
            let wraps_substituted = matches!(call.func.as_ref(), Expr::Name(f) if f.id.as_str() == "int")
                && call.keywords.is_empty()
                && matches!(call.args.as_slice(), [Expr::Call(inner)] if is_substituted_call(inner, qualified, name));
            if wraps_substituted {
                counts.0 += 1;
                counts.1 += 1;
                counts.2 += sign;
            }
            wraps_substituted
        }
        _ => false,
    }
}

fn is_substituted_call(call: &ast::ExprCall, qualified: &str, name: &str) -> bool {
    call.args.is_empty()
        && call.keywords.is_empty()
        && expr_full_name(call.func.as_ref())
            .is_some_and(|callee| callee == qualified || callee == name)
}

/// `line` without the substituted calls, for checks that would read them as
/// calls of an unresolved import.
pub(super) fn without_calls(line: &str, spellings: &[String]) -> String {
    let mut rest = line.to_string();
    for needle in spellings {
        let mut from = 0;
        while let Some(found) = rest[from..].find(needle.as_str()) {
            let idx = from + found;
            let before = rest[..idx].chars().next_back();
            if before.is_some_and(|ch| is_python_identifier_char(ch) || ch == '.') {
                from = idx + 1;
                continue;
            }
            rest.replace_range(idx..idx + needle.len(), "0");
        }
    }
    rest
}

/// The dotted targets of every substitution in the test body, or None when any
/// substitution is not `with patch("<dotted>", return_value=<literal>):`.
fn literal_patch_targets(test: &PythonTest) -> Option<Vec<String>> {
    let mut targets = Vec::new();
    for line in test.body_text.lines() {
        let text = line.trim();
        // A test's own name may say "patched"; its decorators are checked apart.
        if text.starts_with("def ") || text.starts_with("async def ") {
            continue;
        }
        if mentions_substitution(text) {
            targets.push(literal_patch_header(text, &test.imports)?);
        } else if !is_plain_test_line(text) {
            return None;
        }
    }
    Some(targets)
}

/// Any other line of the test body is an `assert ...` or `name = ...` with no
/// way to rebind names behind the test's back: no dunder access, lambda,
/// import, `exec`/`eval`, `globals`/`locals`/`vars` or `sys.modules`.
fn is_plain_test_line(text: &str) -> bool {
    if text.is_empty() || text.starts_with('#') {
        return true;
    }
    const FORBIDDEN: &[&str] = &[
        "__", "lambda", "import", "exec", "eval", "globals", "locals", "vars", "modules",
        "getattr", "open(",
    ];
    let lower = text.to_ascii_lowercase();
    if FORBIDDEN.iter().any(|word| lower.contains(word)) {
        return false;
    }
    if text.starts_with("assert ") {
        return true;
    }
    // `name = value`: a plain identifier on the left, not `==`.
    text.split_once('=')
        .is_some_and(|(left, right)| is_identifier(left.trim()) && !right.starts_with('='))
}

fn mentions_substitution(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "patch", "mock", "setattr", "setitem", "delattr", "stub", "fake",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

/// `with patch("a.b.c", return_value=1700000000):` -> `a.b.c`.
fn literal_patch_header(line: &str, imports: &[PythonImport]) -> Option<String> {
    let item = line.strip_prefix("with ")?.strip_suffix(':')?;
    let Expr::Call(call) = parse_expression(item)? else {
        return None;
    };
    if !is_unittest_mock_patch(expr_full_name(call.func.as_ref())?.as_str(), imports) {
        return None;
    }
    let [Expr::Constant(target)] = call.args.as_slice() else {
        return None;
    };
    let ast::Constant::Str(target) = &target.value else {
        return None;
    };
    let [keyword] = call.keywords.as_slice() else {
        return None;
    };
    if keyword.arg.as_ref()?.as_str() != "return_value" {
        return None;
    }
    let Expr::Constant(value) = &keyword.value else {
        return None;
    };
    // A zero stand-in makes `x + v` and `x - v` the same value, hiding an
    // operator flip; a string stand-in raises on `-`, which the test sees.
    let usable = match &value.value {
        ast::Constant::Int(number) => number.to_string() != "0",
        ast::Constant::Str(_) => true,
        _ => false,
    };
    usable.then(|| target.clone())
}

/// `patch` imported from `unittest.mock`/`mock`, or `mock.patch` through an
/// import of the `mock` module, spelled by its own name.
fn is_unittest_mock_patch(name: &str, imports: &[PythonImport]) -> bool {
    let spelled = |import: &PythonImport, imported: &str| {
        import.imported == imported && (import.alias.is_empty() || import.alias == imported)
    };
    match name {
        "patch" => imports.iter().any(|import| {
            spelled(import, "patch")
                && matches!(import.source_module.as_str(), "unittest.mock" | "mock")
        }),
        "mock.patch" => imports.iter().any(|import| {
            spelled(import, "mock") && matches!(import.source_module.as_str(), "unittest" | "")
        }),
        _ => false,
    }
}

fn owner_line_texts<'a>(owner: &PythonOwner, owner_source: &'a str) -> Vec<&'a str> {
    owner_source
        .lines()
        .enumerate()
        .filter(|(index, _)| {
            let line = index + 1;
            line >= owner.start_line && line <= owner.end_line
        })
        .map(|(_, text)| text)
        .collect()
}

fn is_identifier(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(|ch: char| ch.is_ascii_digit())
        && text.chars().all(is_python_identifier_char)
}
