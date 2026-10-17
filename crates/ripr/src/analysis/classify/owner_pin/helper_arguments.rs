//! RIPR-SPEC-0197 "Check-helper argument pins": a test-local check helper
//! whose assertion compares two of its own parameters reads, at the test's
//! eager call of it, as comparing the two arguments that call passes. No
//! general value flow: an argument is read as written, or through one
//! once-used `let` in the test's own block.

use super::{
    HelperLoan, RustIndex, TestSummary, bound_owner_initializer, is_plain_identifier,
    wrapper_may_be_rebound,
};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasName},
};

/// The parameter types a helper may compare with the standard `==`: the
/// primitive scalars, `String` and `&str`, each of which compares by value.
const BY_VALUE_TYPES: &[&str] = &[
    "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32",
    "u64", "u128", "usize", "String", "&str",
];

/// The test-scope texts the helper's `compared` parameters stand for at one
/// of the test's eager calls of it, or `None` when the helper does not
/// compare two distinct plain parameters of one by-value type, or no call
/// passes readable arguments. An argument that is a plain identifier is
/// read through its one `let` ([`bound_owner_initializer`]): bound once,
/// immutable, in the test's own block, and used only by this call.
pub(super) fn substituted_operands(
    test: &TestSummary,
    loan: &HelperLoan,
    compared: [&str; 2],
    owner: &str,
    index: &RustIndex,
) -> Option<[String; 2]> {
    let position = |name: &str| {
        loan.parameters
            .iter()
            .position(|param| param == name.trim())
    };
    let (left, right) = (position(compared[0])?, position(compared[1])?);
    if left == right {
        return None;
    }
    let types = parameter_types(&loan.helper, &loan.name)?;
    let (Some(left_type), Some(right_type)) = (types.get(left), types.get(right)) else {
        return None;
    };
    // `impl PartialEq<B> for A` may make `a == b` ignore its operands
    // when the two types differ, or when either is a workspace type.
    if left_type != right_type
        || !BY_VALUE_TYPES.contains(&left_type.as_str())
        || wrapper_may_be_rebound(left_type.trim_start_matches('&'), index)
    {
        return None;
    }
    loan.call_lines.iter().find_map(|line| {
        let text = test.body.lines().nth(line.checked_sub(test.start_line)?)?;
        let arguments = call_arguments(text, &loan.name)?;
        if arguments.len() != loan.parameters.len() {
            return None;
        }
        let read = |at: usize| -> Option<String> {
            let argument = arguments.get(at)?.trim();
            if is_plain_identifier(argument) {
                bound_owner_initializer(test, argument, owner).map(|init| init.trim().to_string())
            } else {
                Some(argument.to_string())
            }
        };
        Some([read(left)?, read(right)?])
    })
}

/// The declared type text of each of `name`'s parameters, in order.
fn parameter_types(helper: &str, name: &str) -> Option<Vec<String>> {
    let parse = parse_clean_source_file(helper)?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .find(|function| function.name().is_some_and(|found| found.text() == name))?;
    function
        .param_list()?
        .params()
        .map(|param| {
            let ty = param.ty()?.syntax().text().to_string();
            Some(ty.split_whitespace().collect::<String>())
        })
        .collect()
}

/// The argument texts of the one call statement `text` holds when that
/// statement is exactly `name(..);`.
fn call_arguments(text: &str, name: &str) -> Option<Vec<String>> {
    let parse = parse_clean_source_file(&format!("fn ripr_call() {{ {} }}", text.trim()))?;
    let function = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let statements = function.body()?.stmt_list()?;
    let mut statements = statements.statements();
    let (Some(ast::Stmt::ExprStmt(statement)), None) = (statements.next(), statements.next())
    else {
        return None;
    };
    let ast::Expr::CallExpr(call) = statement.expr()? else {
        return None;
    };
    let ast::Expr::PathExpr(callee) = call.expr()? else {
        return None;
    };
    let path = callee.path()?;
    if path.qualifier().is_some() || path.syntax().text() != name {
        return None;
    }
    Some(
        call.arg_list()?
            .args()
            .map(|argument| argument.syntax().text().to_string())
            .collect(),
    )
}

/// The literal text `operand` stands for as an expected value: a literal
/// (`4`, `-1.5`, `'x'`, `true`, `"text"`), or `String::from("text")` read as
/// `"text"` while no workspace item may rebind `String`.
pub(super) fn expected_literal(operand: &str, index: &RustIndex) -> Option<String> {
    let parse = parse_clean_source_file(&format!("const RIPR: () = {};", operand.trim()))?;
    let expression = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Const::cast)?
        .body()?;
    let literal = |expression: &ast::Expr| match expression {
        ast::Expr::Literal(_) => Some(expression.syntax().text().to_string()),
        _ => None,
    };
    match &expression {
        ast::Expr::Literal(_) => literal(&expression),
        ast::Expr::PrefixExpr(negated)
            if negated.op_kind() == Some(ast::UnaryOp::Neg)
                && negated.expr().is_some_and(|inner| {
                    matches!(&inner, ast::Expr::Literal(found)
                        if matches!(found.kind(), ast::LiteralKind::IntNumber(_) | ast::LiteralKind::FloatNumber(_)))
                }) =>
        {
            Some(expression.syntax().text().to_string())
        }
        ast::Expr::CallExpr(call) => {
            let ast::Expr::PathExpr(callee) = call.expr()? else {
                return None;
            };
            if callee.path()?.syntax().text() != "String::from"
                || wrapper_may_be_rebound("String", index)
            {
                return None;
            }
            let mut arguments = call.arg_list()?.args();
            let (Some(ast::Expr::Literal(found)), None) = (arguments.next(), arguments.next())
            else {
                return None;
            };
            matches!(found.kind(), ast::LiteralKind::String(_))
                .then(|| found.syntax().text().to_string())
        }
        _ => None,
    }
}

/// trap-shine-debug-exact-control, generalized: the call `operand` renders
/// when it is `format!(template, call)` with one `{}` or `{:?}` placeholder
/// and literal text around it, and the string literal `expected` holds that
/// same text around a non-empty middle. A primitive scalar's std `Display`
/// and `Debug` text differs for every value but NaN, and the fixed text
/// around it fixes where the middle starts and ends, so the comparison pins
/// the value; a middle of `NaN` is refused (#7905). Templates and expected
/// literals with escapes other than `\"` and `\\`, raw strings, width,
/// precision, alternate and hex flags, positional or named arguments and
/// extra arguments are refused.
pub(super) fn rendered_template_call<'a>(operand: &'a str, expected: &str) -> Option<&'a str> {
    let inner = operand
        .trim()
        .strip_prefix("format!")?
        .trim_start()
        .strip_prefix('(')?
        .strip_suffix(')')?;
    let (template, call) = template_and_call(inner)?;
    let (prefix, suffix) = template_parts(&string_contents(template)?)?;
    let expected = string_contents(expected)?;
    let middle = expected
        .strip_prefix(prefix.as_str())?
        .strip_suffix(suffix.as_str())?;
    (!middle.is_empty() && middle != "NaN").then_some(call)
}

/// The template and the one argument of a `format!` argument list.
fn template_and_call(inner: &str) -> Option<(&str, &str)> {
    let parse = parse_clean_source_file(&format!("fn ripr_args() {{ ripr({inner}); }}"))?;
    let call = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::CallExpr::cast)?;
    let arguments: Vec<ast::Expr> = call.arg_list()?.args().collect();
    let [template, argument] = arguments.as_slice() else {
        return None;
    };
    // Offsets into `inner`: the wrapper text before it is fixed.
    let base = "fn ripr_args() { ripr(".len();
    let span = |expression: &ast::Expr| {
        let range = expression.syntax().text_range();
        let start = usize::from(range.start()).checked_sub(base)?;
        let end = usize::from(range.end()).checked_sub(base)?;
        inner.get(start..end)
    };
    Some((span(template)?, span(argument)?))
}

/// The decoded text of a plain `"..."` literal whose only escapes are `\"`
/// and `\\`.
fn string_contents(text: &str) -> Option<String> {
    let inner = text.trim().strip_prefix('"')?.strip_suffix('"')?;
    let mut decoded = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.next()? {
                escaped @ ('"' | '\\') => decoded.push(escaped),
                _ => return None,
            },
            '"' => return None,
            ch => decoded.push(ch),
        }
    }
    Some(decoded)
}

/// The literal text before and after the one `{}` or `{:?}` placeholder of
/// a decoded format template, with `{{` and `}}` read as braces.
fn template_parts(template: &str) -> Option<(String, String)> {
    let (mut before, mut after) = (String::new(), String::new());
    let mut placeholders = 0usize;
    let mut rest = template;
    while let Some(ch) = rest.chars().next() {
        let out = if placeholders == 0 {
            &mut before
        } else {
            &mut after
        };
        if let Some(tail) = rest.strip_prefix("{{") {
            out.push('{');
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("}}") {
            out.push('}');
            rest = tail;
        } else if let Some(tail) = rest
            .strip_prefix("{}")
            .or_else(|| rest.strip_prefix("{:?}"))
        {
            placeholders += 1;
            rest = tail;
        } else if matches!(ch, '{' | '}') {
            return None;
        } else {
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    (placeholders == 1).then_some((before, after))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::rust_index::summarize_file;

    #[test]
    fn reads_only_a_bare_call_statement() {
        assert_eq!(
            call_arguments("    check(actual, expected);", "check"),
            Some(vec!["actual".to_string(), "expected".to_string()])
        );
        for (text, name) in [
            ("let _ = check(a, b);", "check"),
            ("m::check(a, b);", "check"),
            ("check(a, b); check(b, a);", "check"),
            ("recheck(a, b);", "check"),
        ] {
            assert_eq!(call_arguments(text, name), None, "{text}");
        }
    }

    #[test]
    fn reads_a_template_around_one_placeholder() {
        let operand = r#"format!("{{\"score\":{}}}", score(2))"#;
        assert_eq!(
            rendered_template_call(operand, r#""{\"score\":4}""#),
            Some("score(2)")
        );
        assert_eq!(
            rendered_template_call(r#"format!("{:?}", f(1))"#, r#""7""#),
            Some("f(1)")
        );
        for (operand, expected) in [
            // The expected text does not hold the template's text.
            (operand, r#""{\"total\":4}""#),
            // An empty middle, or NaN, names no single value.
            (operand, r#""{\"score\":}""#),
            (operand, r#""{\"score\":NaN}""#),
            // Two placeholders, positional and formatted placeholders.
            (r#"format!("{}{}", f(1), f(2))"#, r#""12""#),
            (r#"format!("{0}", f(1))"#, r#""1""#),
            (r#"format!("{:>3}", f(1))"#, r#""  1""#),
            (r#"format!("{x}")"#, r#""1""#),
            // Escapes other than quote and backslash.
            (r#"format!("a\n{}", f(1))"#, r#""a\n1""#),
            (r#"format!(r"{}", f(1))"#, r#""1""#),
        ] {
            assert_eq!(rendered_template_call(operand, expected), None, "{operand}");
        }
    }

    #[test]
    fn reads_only_literal_expected_values() {
        let plain = RustIndex::default();
        for (operand, literal) in [
            ("4", "4"),
            ("-1.5", "-1.5"),
            ("'x'", "'x'"),
            ("true", "true"),
            (r#""a\"b""#, r#""a\"b""#),
            (r#"String::from("4")"#, r#""4""#),
        ] {
            assert_eq!(
                expected_literal(operand, &plain).as_deref(),
                Some(literal),
                "{operand}"
            );
        }
        for operand in [
            "four",
            "score(2)",
            "-four",
            r#""4".to_string()"#,
            r#"String::from(four)"#,
            r#"String::from("4", "5")"#,
            "String::from(4)",
            r#"std::string::String::from("4")"#,
        ] {
            assert_eq!(expected_literal(operand, &plain), None, "{operand}");
        }
        let mut shadowed = RustIndex::default();
        let path = std::path::PathBuf::from("src/lib.rs");
        let facts = summarize_file(path.clone(), "pub struct String(u8);\n".to_string());
        shadowed.insert_file_only(path, facts);
        assert_eq!(expected_literal(r#"String::from("4")"#, &shadowed), None);
        assert_eq!(expected_literal("4", &shadowed).as_deref(), Some("4"));
    }

    #[test]
    fn reads_parameter_types_without_spacing() {
        assert_eq!(
            parameter_types("fn check(a: & str, b: String) { }", "check"),
            Some(vec!["&str".to_string(), "String".to_string()])
        );
    }
}
