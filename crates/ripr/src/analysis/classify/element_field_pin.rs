//! #7858: a field constructed inside an array the owner returns is observed
//! by a test that pins that element's field, by position or by membership.
//!
//! `fn rows(a, b) -> [Row; 2] { [Row { total: 0 }, Row { total: a + b }] }`
//! constructs the changed `total` in element 1. Two test shapes observe it:
//!
//! - `assert_eq!(rows[1].total, 33)` reads element 1 by its literal index;
//! - `assert!(rows.iter().any(|r| r.total == 33))` can only hold through
//!   element 1 when every other element builds `total` from an integer
//!   literal other than 33. When another element already holds 33, the test
//!   passes whatever element 1 holds, so it observes nothing.
//!
//! Both shapes need element identity, never a field-name coincidence. The
//! owner's body is the array and nothing else, the receiver is one immutable
//! binding of a direct owner call, and the assertion is a top-level statement
//! of a plain `#[test]` that cannot leave before it.

use super::super::rust_index::{FunctionSummary, TestSummary};
use crate::analysis::facts::FunctionContainer;
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::Probe;
use ra_ap_syntax::{
    AstNode, AstToken, SyntaxKind,
    ast::{self, HasArgList, HasAttrs, HasGenericArgs, HasName},
};

/// Whether `test` pins the field `probe` constructs inside the array `owner`
/// returns, through that element alone. `admitted` is the shared assertion
/// context validator (macro rebinding, escapes, module composition) for the
/// assertion on a test line; this reading never stands in for it.
pub(in crate::analysis) fn observes_returned_element_field(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    admitted: &dyn Fn(usize) -> bool,
) -> bool {
    // A bare `rows(..)` call names a free function; a method owner is
    // never called that way, so the call would be some other `rows`.
    if owner.item.container != FunctionContainer::Free {
        return false;
    }
    let Some(array) = ReturnedArray::read(probe, owner) else {
        return false;
    };
    // Exactly `#[test]`: the lexical fallback and harness-registered tests
    // carry no attributes, so an empty list cannot vouch for the test.
    if !matches!(test.attrs.as_slice(), [only] if only.trim() == "#[test]") {
        return false;
    }
    let source = test.body.as_str();
    let Some(parse) = parse_clean_source_file(source) else {
        return false;
    };
    let Some(function) = parse.tree().syntax().descendants().find_map(ast::Fn::cast) else {
        return false;
    };
    if function
        .attrs()
        .any(|attr| squeeze(&attr.syntax().text().to_string()) != "#[test]")
    {
        return false;
    }
    let Some(body) = function.body() else {
        return false;
    };
    // A test that may leave early, through syntax or inside any macro's
    // tokens, or declares an item that may shadow the owner, is not read.
    if body.syntax().descendants_with_tokens().any(|element| {
        matches!(
            element.kind(),
            SyntaxKind::RETURN_KW
                | SyntaxKind::BREAK_KW
                | SyntaxKind::CONTINUE_KW
                | SyntaxKind::QUESTION
        ) || element.as_node().is_some_and(|node| {
            ast::Item::can_cast(node.kind()) && !ast::MacroCall::can_cast(node.kind())
        })
    }) {
        return false;
    }
    let Some(statements) = body.stmt_list() else {
        return false;
    };
    // The pinning assertion runs first: before it the test may only bind
    // the owner's result, so nothing can rebind the receiver, settle the
    // test or leave before the assertion is evaluated.
    let mut rest = statements.statements().peekable();
    let mut receiver = None;
    if let Some(ast::Stmt::LetStmt(binding)) = rest.peek()
        && let Some(name) = owner_binding(binding, &owner.name)
    {
        if binds_more_than_once(&body, &name) {
            return false;
        }
        receiver = Some(name);
        rest.next();
    }
    let first = match rest.next() {
        Some(ast::Stmt::ExprStmt(statement)) => statement.expr(),
        Some(_) => return false,
        None => statements.tail_expr(),
    };
    let Some(ast::Expr::MacroExpr(call)) = first.map(unwrap_parens) else {
        return false;
    };
    let Some(call) = call.macro_call() else {
        return false;
    };
    let Some(line) = source
        .get(..usize::from(call.syntax().text_range().start()))
        .map(|before| test.start_line + before.matches('\n').count())
    else {
        return false;
    };
    array.pinned_by(&call, receiver.as_deref(), &owner.name) && admitted(line)
}

/// The owner's whole body is one array of struct literals, and the probe
/// constructs `field` in exactly one of them.
#[derive(Debug)]
struct ReturnedArray {
    field: String,
    index: usize,
    /// The integer literal each other element builds `field` from, or
    /// `None` when that element builds it from anything else.
    others: Vec<Option<i128>>,
}

impl ReturnedArray {
    fn read(probe: &Probe, owner: &FunctionSummary) -> Option<Self> {
        let source = owner.body.as_str();
        let parse = parse_clean_source_file(source)?;
        let function = parse
            .tree()
            .syntax()
            .descendants()
            .find_map(ast::Fn::cast)?;
        let body = function.body()?;
        let statements = body.stmt_list()?;
        if statements.statements().next().is_some() {
            return None;
        }
        let ast::Expr::ArrayExpr(array) = unwrap_parens(statements.tail_expr()?) else {
            return None;
        };
        if array.semicolon_token().is_some() {
            return None;
        }
        let wanted = squeeze(probe.expression.trim().trim_end_matches(','));
        let mut path = None;
        let mut field = None;
        let mut index = None;
        let mut fields = Vec::new();
        for (position, element) in array.exprs().enumerate() {
            let ast::Expr::RecordExpr(record) = element else {
                return None;
            };
            let list = record.record_expr_field_list()?;
            if list.spread().is_some() || list.dotdot_token().is_some() {
                return None;
            }
            let record_path = squeeze(&record.path()?.syntax().text().to_string());
            if path.get_or_insert_with(|| record_path.clone()) != &record_path {
                return None;
            }
            let mut probed = None;
            for initializer in list.fields() {
                let text = squeeze(&initializer.syntax().text().to_string());
                let line = owner.start_line
                    + source
                        .get(..usize::from(initializer.syntax().text_range().start()))?
                        .matches('\n')
                        .count();
                if text == wanted && line == probe.location.line {
                    probed = Some(initializer.field_name()?.text().to_string());
                }
            }
            if let Some(name) = probed {
                if index.replace(position).is_some() {
                    return None;
                }
                field = Some(name);
            }
            fields.push(list);
        }
        let field = field?;
        let index = index?;
        let mut others = Vec::new();
        for (position, list) in fields.iter().enumerate() {
            let mut named = list.fields().filter(|initializer| {
                initializer
                    .field_name()
                    .is_some_and(|name| name.text() == field.as_str())
            });
            let initializer = named.next()?;
            if named.next().is_some() {
                return None;
            }
            if position != index {
                others.push(initializer.expr().as_ref().and_then(integer_literal));
            }
        }
        Some(Self {
            field,
            index,
            others,
        })
    }

    /// Whether the assertion `call` pins this element's field on the owner's
    /// result: `assert_eq!(recv[index].field, literal)` in either order, or
    /// `assert!(recv.iter().any(|r| r.field == literal))` when no other
    /// element can hold that literal.
    fn pinned_by(&self, call: &ast::MacroCall, receiver: Option<&str>, owner: &str) -> bool {
        // A bare macro name only: a qualified path may name any crate's
        // macro, and the shared validator decides what a bare name binds.
        let Some(path) = call.path() else {
            return false;
        };
        if path.qualifier().is_some() || path.coloncolon_token().is_some() {
            return false;
        }
        let Some(name) = path.segment() else {
            return false;
        };
        let Some(arguments) = macro_arguments(call) else {
            return false;
        };
        let is_receiver = |expression: &ast::Expr| match unwrap_parens(expression.clone()) {
            ast::Expr::PathExpr(path) => receiver.is_some_and(|receiver| {
                path.path()
                    .is_some_and(|path| path.syntax().text() == receiver)
            }),
            ast::Expr::CallExpr(call) => is_owner_call(&call, owner),
            _ => false,
        };
        match (
            name.syntax().text().to_string().as_str(),
            arguments.as_slice(),
        ) {
            ("assert_eq", [left, right]) => {
                [(left, right), (right, left)]
                    .into_iter()
                    .any(|(read, expected)| {
                        is_literal(expected) && self.indexed_read(read, &is_receiver)
                    })
            }
            ("assert", [condition]) => self.membership(condition, &is_receiver),
            _ => false,
        }
    }

    /// `recv[index].field` with the probed element's literal index.
    fn indexed_read(&self, read: &ast::Expr, is_receiver: &dyn Fn(&ast::Expr) -> bool) -> bool {
        let ast::Expr::FieldExpr(read) = unwrap_parens(read.clone()) else {
            return false;
        };
        if read
            .name_ref()
            .is_none_or(|name| name.text() != self.field.as_str())
        {
            return false;
        }
        let Some(ast::Expr::IndexExpr(element)) = read.expr().map(unwrap_parens) else {
            return false;
        };
        element.base().is_some_and(|base| is_receiver(&base))
            && element
                .index()
                .as_ref()
                .and_then(integer_literal)
                .is_some_and(|index| usize::try_from(index).is_ok_and(|index| index == self.index))
    }

    /// `recv.iter().any(|r| r.field == literal)`, or `into_iter`, or the
    /// literal first, where every other element builds the field from a
    /// different integer literal.
    fn membership(&self, condition: &ast::Expr, is_receiver: &dyn Fn(&ast::Expr) -> bool) -> bool {
        let ast::Expr::MethodCallExpr(any) = unwrap_parens(condition.clone()) else {
            return false;
        };
        if any.name_ref().is_none_or(|name| name.text() != "any")
            || any.generic_arg_list().is_some()
        {
            return false;
        }
        let Some(ast::Expr::MethodCallExpr(iter)) = any.receiver().map(unwrap_parens) else {
            return false;
        };
        if iter
            .name_ref()
            .is_none_or(|name| !matches!(name.text(), "iter" | "into_iter"))
            || iter
                .arg_list()
                .is_none_or(|list| list.args().next().is_some())
            || iter
                .receiver()
                .is_none_or(|receiver| !is_receiver(&receiver))
        {
            return false;
        }
        let Some(list) = any.arg_list() else {
            return false;
        };
        let arguments = list.args().collect::<Vec<_>>();
        let [ast::Expr::ClosureExpr(closure)] = arguments.as_slice() else {
            return false;
        };
        let Some(parameter) = sole_parameter(closure) else {
            return false;
        };
        let Some(ast::Expr::BinExpr(comparison)) = closure.body().map(unwrap_parens) else {
            return false;
        };
        if comparison.op_kind() != Some(ast::BinaryOp::CmpOp(ast::CmpOp::Eq { negated: false })) {
            return false;
        }
        let (Some(left), Some(right)) = (comparison.lhs(), comparison.rhs()) else {
            return false;
        };
        let reads_field = |expression: &ast::Expr| {
            let ast::Expr::FieldExpr(read) = unwrap_parens(expression.clone()) else {
                return false;
            };
            read.name_ref()
                .is_some_and(|name| name.text() == self.field.as_str())
                && matches!(
                    read.expr().map(unwrap_parens),
                    Some(ast::Expr::PathExpr(path))
                        if path.path().is_some_and(|path| path.syntax().text() == parameter.as_str())
                )
        };
        [(&left, &right), (&right, &left)]
            .into_iter()
            .any(|(read, expected)| {
                reads_field(read)
                    && integer_literal(expected).is_some_and(|expected| {
                        self.others
                            .iter()
                            .all(|other| other.is_some_and(|other| other != expected))
                    })
            })
    }
}

/// `let NAME = owner(..);` with a plain, immutable binding.
fn owner_binding(binding: &ast::LetStmt, owner: &str) -> Option<String> {
    let ast::Pat::IdentPat(pattern) = binding.pat()? else {
        return None;
    };
    if pattern.mut_token().is_some() || pattern.ref_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    if binding.let_else().is_some() {
        return None;
    }
    let ast::Expr::CallExpr(call) = unwrap_parens(binding.initializer()?) else {
        return None;
    };
    is_owner_call(&call, owner).then(|| pattern.name().map(|name| name.text().to_string()))?
}

/// A direct call of the owner by its own name, or through `super::`/`crate::`,
/// with literal or path arguments only, so evaluating them cannot leave the
/// test or settle it before the assertion.
fn is_owner_call(call: &ast::CallExpr, owner: &str) -> bool {
    let plain_arguments = call.arg_list().is_some_and(|list| {
        list.args().all(|argument| {
            integer_literal(&argument).is_some()
                || matches!(
                    unwrap_parens(argument),
                    ast::Expr::Literal(_) | ast::Expr::PathExpr(_)
                )
        })
    });
    if !plain_arguments {
        return false;
    }
    let Some(ast::Expr::PathExpr(path)) = call.expr() else {
        return false;
    };
    let Some(path) = path.path() else {
        return false;
    };
    let text = squeeze(&path.syntax().text().to_string());
    [String::new(), "super::".to_string(), "crate::".to_string()]
        .iter()
        .any(|prefix| text == format!("{prefix}{owner}"))
}

/// The receiver is bound once in the whole test body, so no shadowing
/// binding, closure or pattern parameter can stand in for it.
fn binds_more_than_once(body: &ast::BlockExpr, name: &str) -> bool {
    body.syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter(|pattern| pattern.name().is_some_and(|ident| ident.text() == name))
        .count()
        > 1
}

fn sole_parameter(closure: &ast::ClosureExpr) -> Option<String> {
    let list = closure.param_list()?;
    let parameters = list.params().collect::<Vec<_>>();
    let [parameter] = parameters.as_slice() else {
        return None;
    };
    if parameter.ty().is_some() {
        return None;
    }
    let ast::Pat::IdentPat(pattern) = parameter.pat()? else {
        return None;
    };
    if pattern.mut_token().is_some() || pattern.ref_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    pattern.name().map(|name| name.text().to_string())
}

/// The comma-separated arguments of an assertion macro, parsed as
/// expressions.
fn macro_arguments(call: &ast::MacroCall) -> Option<Vec<ast::Expr>> {
    let tree = call.token_tree()?;
    let text = tree.syntax().text().to_string();
    let inner = text.get(1..text.len().checked_sub(1)?)?;
    let parse = parse_clean_source_file(&format!("fn arguments() {{ arguments({inner}); }}"))?;
    let call = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::CallExpr::cast)?;
    Some(call.arg_list()?.args().collect())
}

fn is_literal(expression: &ast::Expr) -> bool {
    matches!(unwrap_parens(expression.clone()), ast::Expr::Literal(_))
        || integer_literal(expression).is_some()
}

fn integer_literal(expression: &ast::Expr) -> Option<i128> {
    let (negative, literal) = match unwrap_parens(expression.clone()) {
        ast::Expr::PrefixExpr(prefix) if prefix.op_kind() == Some(ast::UnaryOp::Neg) => {
            let ast::Expr::Literal(literal) = prefix.expr()? else {
                return None;
            };
            (true, literal)
        }
        ast::Expr::Literal(literal) => (false, literal),
        _ => return None,
    };
    let ast::LiteralKind::IntNumber(number) = literal.kind() else {
        return None;
    };
    let text = number.text().replace('_', "");
    let digits = INTEGER_SUFFIXES
        .iter()
        .find_map(|suffix| text.strip_suffix(suffix))
        .unwrap_or(&text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value = digits.parse::<i128>().ok()?;
    Some(if negative { -value } else { value })
}

const INTEGER_SUFFIXES: &[&str] = &[
    "i128", "isize", "i16", "i32", "i64", "i8", "u128", "usize", "u16", "u32", "u64", "u8",
];

fn unwrap_parens(expression: ast::Expr) -> ast::Expr {
    let mut expression = expression;
    while let ast::Expr::ParenExpr(inner) = &expression {
        let Some(next) = inner.expr() else {
            break;
        };
        expression = next;
    }
    expression
}

fn squeeze(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DeltaKind, ProbeFamily, ProbeId, SourceLocation};
    use std::path::PathBuf;

    const PROBE: &str = "total: a + b + 0";

    /// A file whose owner `rows` returns `array` on line 6 and whose one
    /// test runs `test`, under `attrs`.
    fn source(array: &str, attrs: &str, test: &str) -> String {
        format!(
            "pub struct Row {{\n    pub total: u32,\n}}\n\n\
             pub fn rows(a: u32, b: u32) -> [Row; 2] {{\n    {array}\n}}\n\n\
             #[cfg(test)]\nmod tests {{\n    use super::*;\n\n    {attrs}\n    fn pins() {{\n{test}\n    }}\n}}\n"
        )
    }

    fn probe() -> Probe {
        Probe {
            id: ProbeId("probe:src/lib.rs:6:rows".to_string()),
            location: SourceLocation::new("src/lib.rs", 6, 24),
            owner: None,
            family: ProbeFamily::FieldConstruction,
            delta: DeltaKind::Value,
            before: Some("total: a + b".to_string()),
            after: Some(PROBE.to_string()),
            expression: PROBE.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    const ARRAY: &str = "[Row { total: 0 }, Row { total: a + b + 0 }]";
    const ANY: &str =
        "        let rows = rows(30, 3);\n        assert!(rows.iter().any(|r| r.total == 33));";

    /// `Ok(None)` when the owner's array cannot be read, else whether the
    /// test pins the probed element.
    fn pinned(array: &str, attrs: &str, test: &str) -> Result<Option<bool>, String> {
        let facts = crate::analysis::rust_index::summarize_file(
            PathBuf::from("src/lib.rs"),
            source(array, attrs, test),
        );
        let owner = facts
            .functions
            .iter()
            .find(|function| function.name == "rows")
            .ok_or("owner indexed")?;
        let Some(read) = ReturnedArray::read(&probe(), owner) else {
            return Ok(None);
        };
        if read.field != "total" || read.index != 1 {
            return Err(format!("unexpected read {read:?}"));
        }
        let test = facts
            .tests
            .iter()
            .find(|test| test.name == "pins")
            .ok_or("test indexed")?;
        Ok(Some(observes_returned_element_field(
            &probe(),
            owner,
            test,
            &|_| true,
        )))
    }

    #[test]
    fn a_test_pinning_the_probed_element_observes_its_field() -> Result<(), String> {
        for test in [
            ANY,
            "        let rows = rows(30, 3);\n        assert!(rows.iter().any(|r| 33 == r.total));",
            "        let rows = rows(30, 3);\n        assert!(rows.into_iter().any(|r| r.total == 33u32));",
            "        let rows = rows(30, 3);\n        assert_eq!(rows[1].total, 33);",
            "        let rows = rows(30, 3);\n        assert_eq!(33, rows[1].total);",
            "        assert_eq!(rows(30, 3)[1].total, 33);",
            "        assert!(super::rows(30, 3).iter().any(|r| r.total == 33));",
            "        let rows = crate::rows(30, 3);\n        assert_eq!(rows[1].total, 33)",
        ] {
            if pinned(ARRAY, "#[test]", test)? != Some(true) {
                return Err(format!("expected a pin: {test}"));
            }
        }
        Ok(())
    }

    #[test]
    fn a_membership_another_element_may_satisfy_observes_nothing() -> Result<(), String> {
        for array in [
            "[Row { total: 33 }, Row { total: a + b + 0 }]",
            "[Row { total: a }, Row { total: a + b + 0 }]",
            "[Row { total: 33_u32 }, Row { total: a + b + 0 }]",
        ] {
            if pinned(array, "#[test]", ANY)? != Some(false) {
                return Err(format!("expected no pin: {array}"));
            }
        }
        // A negative sibling literal is compared by value: `-33` equal to
        // the expected `-33` refuses, `-33` beside an expected 33 does not.
        let negative = "[Row { total: -33 }, Row { total: a + b + 0 }]";
        let membership = |literal: &str| {
            format!(
                "        let rows = rows(30, 3);\n        assert!(rows.iter().any(|r| r.total == {literal}));"
            )
        };
        if pinned(negative, "#[test]", &membership("-33"))? != Some(false) {
            return Err("a sibling equal to the negative literal still pins".to_string());
        }
        if pinned(negative, "#[test]", &membership("33"))? != Some(true) {
            return Err("a sibling of the opposite sign refused the pin".to_string());
        }
        Ok(())
    }

    #[test]
    fn a_test_that_does_not_pin_the_element_alone_observes_nothing() -> Result<(), String> {
        let refused = [
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert!(rows.iter().all(|r| r.total == 33));",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert!(rows.iter().any(|r| r.total != 33));",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert!(rows.iter().any(|r| r.total >= 33));",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert!(!rows.iter().any(|r| r.total == 33));",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert!(rows.iter().any(|r| r.total == 33), \"{}\", 1);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert!(rows.iter().any(|r: &Row| r.total == 33));",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert_eq!(rows[0].total, 0);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert_ne!(rows[1].total, 34);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let expected = 33;\n        assert_eq!(rows[1].total, expected);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        let rows = [Row { total: 33 }];\n        assert!(rows.iter().any(|r| r.total == 33));",
            ),
            (
                "#[test]",
                "        let mut rows = rows(30, 3);\n        rows[1].total = 33;\n        assert_eq!(rows[1].total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        if rows.len() == 2 { return; }\n        assert_eq!(rows[1].total, 33);",
            ),
            (
                "#[test]",
                "        fn rows(_: u32, _: u32) -> [Row; 2] { [Row { total: 33 }, Row { total: 33 }] }\n        assert_eq!(rows(30, 3)[1].total, 33);",
            ),
            (
                "#[test]",
                "        let other = other(30, 3);\n        assert_eq!(other[1].total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert_eq!(rows[1].total, 33, \"sum\");",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        fake!(rows);\n        assert_eq!(rows[1].total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        std::process::exit(0);\n        assert_eq!(rows[1].total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        assert_eq!(rows[1].total, 33);\n        settle!(return);",
            ),
            (
                "#[test]",
                "        let rows = rows(30, 3);\n        lenient::assert_eq!(rows[1].total, 33);",
            ),
            (
                "#[test]",
                "        let rows = rows({ std::process::exit(0) }, 3);\n        assert_eq!(rows[1].total, 33);",
            ),
            (
                "#[test]",
                "        assert_eq!(rows(30, { std::process::exit(0) })[1].total, 33);",
            ),
            ("#[test]\n    #[should_panic]", ANY),
            ("#[test]\n    #[ignore]", ANY),
        ];
        for (attrs, test) in refused {
            if pinned(ARRAY, attrs, test)? != Some(false) {
                return Err(format!("expected no pin: {attrs} {test}"));
            }
        }
        Ok(())
    }

    /// The lexical fallback and harness-registered tests carry no
    /// attributes, and the shared validator decides for the assertion line.
    #[test]
    fn an_unattributed_test_or_a_refused_assertion_context_observes_nothing() -> Result<(), String>
    {
        let facts = crate::analysis::rust_index::summarize_file(
            PathBuf::from("src/lib.rs"),
            source(ARRAY, "#[test]", ANY),
        );
        let owner = facts
            .functions
            .iter()
            .find(|function| function.name == "rows")
            .ok_or("owner indexed")?;
        let test = facts
            .tests
            .iter()
            .find(|test| test.name == "pins")
            .ok_or("test indexed")?;
        let asked = std::cell::Cell::new(None);
        let admitted = |line: usize| {
            asked.set(Some(line));
            true
        };
        if !observes_returned_element_field(&probe(), owner, test, &admitted) {
            return Err("premise: the attributed test pins the element".to_string());
        }
        // `assert!` sits on file line 16, after `let rows = rows(30, 3);`.
        if asked.get() != Some(16) {
            return Err(format!(
                "validator asked for {:?}, not line 16",
                asked.get()
            ));
        }
        if observes_returned_element_field(&probe(), owner, test, &|_| false) {
            return Err("a refused assertion context still pins".to_string());
        }
        let mut bare = test.clone();
        bare.attrs.clear();
        if observes_returned_element_field(&probe(), owner, &bare, &|_| true) {
            return Err("a test without attributes still pins".to_string());
        }
        Ok(())
    }

    /// A method owner: the test's bare `rows(..)` calls the free function
    /// of the same name, which is not the owner (#8059 review).
    #[test]
    fn a_bare_call_never_reaches_a_method_owner() -> Result<(), String> {
        let source = "pub struct Row {\n    pub total: u32,\n}\npub struct Table;\n\
             impl Table {\n    pub fn rows(a: u32, b: u32) -> [Row; 2] {\n        \
             [Row { total: 0 }, Row { total: a + b + 0 }]\n    }\n}\n\
             pub fn rows(_: u32, _: u32) -> [Row; 2] { [Row { total: 33 }, Row { total: 33 }] }\n\n\
             #[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn pins() {\n\
             ANY\n    }\n}\n"
            .replace("ANY", ANY);
        let facts =
            crate::analysis::rust_index::summarize_file(PathBuf::from("src/lib.rs"), source);
        let mut probe = probe();
        probe.location.line = 7;
        let owner = facts
            .functions
            .iter()
            .find(|function| {
                function.name == "rows"
                    && matches!(function.item.container, FunctionContainer::Inherent { .. })
            })
            .ok_or("method owner indexed")?;
        if ReturnedArray::read(&probe, owner).is_none() {
            return Err("premise: the method's array reads".to_string());
        }
        let test = facts
            .tests
            .iter()
            .find(|test| test.name == "pins")
            .ok_or("test indexed")?;
        if observes_returned_element_field(&probe, owner, test, &|_| true) {
            return Err("a bare call credited a method owner".to_string());
        }
        Ok(())
    }

    #[test]
    fn an_owner_that_is_not_one_array_of_one_struct_is_not_read() -> Result<(), String> {
        for array in [
            "let rows = [Row { total: 0 }, Row { total: a + b + 0 }]; rows",
            "[Row { total: a + b + 0 }; 2]",
            "[Row { total: 0 }, Other { total: a + b + 0 }]",
            "[Row { total: 0, ..base() }, Row { total: a + b + 0 }]",
            "[Row { ..base() }, Row { total: a + b + 0 }]",
            "[make(), Row { total: a + b + 0 }]",
            "[Row { total: a + b + 0 }, Row { total: a + b + 0 }]",
        ] {
            if pinned(array, "#[test]", ANY)?.is_some() {
                return Err(format!("expected no read: {array}"));
            }
        }
        Ok(())
    }
}
