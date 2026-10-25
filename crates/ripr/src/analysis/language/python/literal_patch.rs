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
use super::source_facts::parse_module_result;
use super::{PythonImport, PythonOwner, PythonTest, expr_full_name};
use rustpython_parser::ast::{self, Expr, Mod, Stmt};
use std::path::Path;

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
    let targets = literal_patch_targets(test, owner)?;
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

/// The dotted targets of every substitution in the test body, or None unless
/// the whole test is a plain function (no defaults, `*args`, nested defs or
/// decorators) whose statements are `assert`s, `name = <expr>` and
/// `with patch("<dotted>", return_value=<literal>):` blocks of those, and whose
/// expressions can call only the owner and a few builtins. A test that could
/// rebind names behind the patch (a `;`, a chained or attribute assignment, a
/// helper call, a default argument evaluated at definition) is not admitted.
fn literal_patch_targets(test: &PythonTest, owner: &PythonOwner) -> Option<Vec<String>> {
    let Ok(Mod::Module(module)) = parse_module_result(Path::new("t.py"), &test.body_text) else {
        return None;
    };
    let [Stmt::FunctionDef(function)] = module.body.as_slice() else {
        return None;
    };
    let arguments = &function.args;
    if !function.decorator_list.is_empty()
        || arguments.vararg.is_some()
        || arguments.kwarg.is_some()
        || !arguments.kwonlyargs.is_empty()
        || !arguments.posonlyargs.is_empty()
        || arguments
            .args
            .iter()
            .any(|argument| argument.default.is_some())
    {
        return None;
    }
    let mut targets = Vec::new();
    for statement in &function.body {
        match statement {
            Stmt::With(with) => {
                let [item] = with.items.as_slice() else {
                    return None;
                };
                if item.optional_vars.is_some() {
                    return None;
                }
                targets.push(patch_target(&item.context_expr, &test.imports)?);
                if with.body.is_empty()
                    || !with.body.iter().all(|inner| plain_statement(inner, owner))
                {
                    return None;
                }
            }
            other => {
                if !plain_statement(other, owner) {
                    return None;
                }
            }
        }
    }
    Some(targets)
}

/// `assert <expr>`, `name = <expr>`, `pass` or a docstring.
fn plain_statement(statement: &Stmt, owner: &PythonOwner) -> bool {
    match statement {
        Stmt::Assert(assert) => {
            plain_expression(&assert.test, owner)
                && assert
                    .msg
                    .as_ref()
                    .is_none_or(|message| plain_expression(message, owner))
        }
        Stmt::Assign(assign) => {
            matches!(assign.targets.as_slice(), [Expr::Name(_)])
                && plain_expression(&assign.value, owner)
        }
        Stmt::Pass(_) => true,
        Stmt::Expr(expr) => matches!(expr.value.as_ref(), Expr::Constant(_)),
        _ => false,
    }
}

/// Builtins an assertion may call without changing any name.
const SAFE_CALLS: &[&str] = &[
    "int", "str", "len", "float", "bool", "list", "tuple", "dict", "set", "sorted", "round", "abs",
    "sum", "min", "max", "repr",
];

/// Names, constants, operators and calls of the owner or a safe builtin.
fn plain_expression(expr: &Expr, owner: &PythonOwner) -> bool {
    let all = |items: &[Expr]| items.iter().all(|item| plain_expression(item, owner));
    match expr {
        Expr::Name(_) | Expr::Constant(_) => true,
        Expr::Compare(compare) => {
            plain_expression(&compare.left, owner) && all(&compare.comparators)
        }
        Expr::BoolOp(operation) => all(&operation.values),
        Expr::UnaryOp(operation) => plain_expression(&operation.operand, owner),
        Expr::BinOp(operation) => {
            plain_expression(&operation.left, owner) && plain_expression(&operation.right, owner)
        }
        Expr::Tuple(tuple) => all(&tuple.elts),
        Expr::List(list) => all(&list.elts),
        Expr::JoinedStr(joined) => all(&joined.values),
        Expr::FormattedValue(formatted) => {
            formatted.format_spec.is_none() && plain_expression(&formatted.value, owner)
        }
        Expr::Attribute(attribute) => plain_expression(&attribute.value, owner),
        Expr::Subscript(subscript) => {
            plain_expression(&subscript.value, owner) && plain_expression(&subscript.slice, owner)
        }
        Expr::Call(call) => {
            let callee_ok = match call.func.as_ref() {
                Expr::Name(name) => {
                    name.id.as_str() == owner.name || SAFE_CALLS.contains(&name.id.as_str())
                }
                Expr::Attribute(attribute) => {
                    attribute.attr.as_str() == owner.name
                        && expr_full_name(attribute.value.as_ref()).is_some()
                }
                _ => false,
            };
            callee_ok
                && all(&call.args)
                && call
                    .keywords
                    .iter()
                    .all(|keyword| keyword.arg.is_some() && plain_expression(&keyword.value, owner))
        }
        _ => false,
    }
}

fn mentions_substitution(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "patch", "mock", "setattr", "setitem", "delattr", "stub", "fake",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

/// `patch("a.b.c", return_value=1700000000)` -> `a.b.c`.
fn patch_target(expr: &Expr, imports: &[PythonImport]) -> Option<String> {
    let Expr::Call(call) = expr else {
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
    // operator flip, and an empty string stand-in hides a reorder of the call
    // among string parts; a non-empty string raises on `-` or shows in the
    // asserted value, which the test sees.
    let usable = match &value.value {
        ast::Constant::Int(number) => number.to_string() != "0",
        ast::Constant::Str(text) => !text.is_empty(),
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
    // The spelled name must be bound exactly once (a second `from x import
    // patch` would make it something else).
    let bindings = |word: &str| {
        imports
            .iter()
            .filter(|import| import.imported == word || import.alias == word)
            .count()
    };
    match name {
        "patch" => {
            bindings("patch") == 1
                && imports.iter().any(|import| {
                    spelled(import, "patch")
                        && matches!(import.source_module.as_str(), "unittest.mock" | "mock")
                })
        }
        "mock.patch" => {
            bindings("mock") == 1
                && imports.iter().any(|import| {
                    spelled(import, "mock")
                        && matches!(import.source_module.as_str(), "unittest" | "")
                })
        }
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
