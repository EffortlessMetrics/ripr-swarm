//! Exact pattern pins through a test-local `assert_matches!` (#6616,
//! RIPR-SPEC-0229 "Exact pattern pins").
//!
//! ```text
//! macro_rules! assert_matches {
//!     ($value:expr, $pattern:pat) => {
//!         match $value {
//!             $pattern => {}
//!             other => panic!("{other:?} does not match {}", stringify!($pattern)),
//!         }
//!     };
//! }
//! #[test]
//! fn unknown_unit_is_rejected() {
//!     assert_matches!(parse_unit("q"), Err(UnitError::Unknown));
//! }
//! ```
//!
//! This is not a macro evaluator. It admits one invocation shape and
//! refuses everything else, so a refusal only leaves the assertion where
//! the token rule already put it:
//!
//! - the file parses cleanly, defines exactly one macro named
//!   `assert_matches` (a `macro_rules!` with no attributes, a direct item of
//!   the file or an inline module enclosing the test, written before it),
//!   imports no item of that name, carries no `macro_use` and holds no
//!   item-position macro call that could define another one;
//! - the macro's FIRST rule takes exactly `$v:expr, $p:pat` (optionally
//!   `$(,)?`), and the invocation is exactly an expression and a pattern
//!   separated by one top-level comma, so the first rule is the one that
//!   runs whichever later rules exist;
//! - that rule's transcriber is a `match $v { $p => {} <catch-all> =>
//!   panic!(..) }` (or `unreachable!`), whose panic arguments hold no exit
//!   token and call only non-returning macros: a mismatch panics and a
//!   match does nothing;
//! - the test is a plain `#[test] fn` with no parameters, return type,
//!   generics or qualifiers, in a supported item context, and the
//!   invocation is a top-level statement of its body;
//! - every statement before it is on an allowlist of shapes that complete
//!   normally or fail the test (a plain `let`, a call expression, a
//!   standard assertion macro, another admitted `assert_matches!`); anything
//!   else stops the walk, so nothing after it is admitted;
//! - the value operand is built only from calls, paths, literals and the
//!   other allowlisted expression nodes, and the pattern operand is exact:
//!   constructors, paths, literals and tuples, with no binding, wildcard,
//!   rest, range or alternative.
//!
//! The macro names each admitted pin relies on (`panic`, `stringify`, the
//! standard assertions before it) are returned so the caller can refuse a
//! workspace rebinding through the shared owner-pin binding authority.

use super::nesting::parse_clean_source_file;
use super::owner_pin::supported_item_context;
use super::ra::{LineIndex, slice_macro_call_text, slice_text};
use ra_ap_syntax::ast::{HasAttrs, HasGenericParams, HasName};
use ra_ap_syntax::{AstNode, NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken, ast};
use std::collections::{BTreeMap, BTreeSet};

/// Test identity: the `fn` token's line, the item's last line, its name.
type TestKey = (usize, usize, String);
/// Assertion identity within a test: its line and invocation text.
type PinKey = (usize, String);

/// One admitted `assert_matches!` invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PatternPin {
    /// The value operand's source text (`parse_unit("q")`).
    pub(crate) value: String,
    /// Standard macro names the pin relies on not being rebound.
    pub(crate) macros: BTreeSet<String>,
}

#[derive(Clone, Debug, Default)]
struct TestPins {
    body: String,
    pins: BTreeMap<PinKey, PatternPin>,
}

/// Every admitted exact pattern pin in one file, keyed by test.
#[derive(Clone, Debug, Default)]
pub(crate) struct PatternPins {
    tests: BTreeMap<TestKey, TestPins>,
}

impl PatternPins {
    /// The admitted pin for `assertion` in `test`, when the indexed body
    /// still matches the parsed source.
    pub(crate) fn pin(
        &self,
        test: (usize, usize, &str),
        body: &str,
        assertion: (usize, &str),
    ) -> Option<&PatternPin> {
        let facts = self
            .tests
            .get(&(test.0, test.1, test.2.to_string()))
            .filter(|facts| facts.body == body)?;
        facts.pins.get(&(assertion.0, assertion.1.to_string()))
    }
}

const MACRO_NAME: &str = "assert_matches";

/// Macros whose expansion cannot leave the enclosing function, allowed
/// inside a panic message or a preceding assertion's operands.
const NON_EXITING_OPERAND_MACROS: &[&str] = &[
    "column",
    "concat",
    "file",
    "format",
    "format_args",
    "line",
    "matches",
    "module_path",
    "stringify",
    "vec",
];

/// Standard assertions that either complete or panic.
const STANDARD_ASSERTIONS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
];

/// The verified first rule of the file's `assert_matches!`.
struct VerifiedMacro {
    /// The node whose items the definition's textual scope covers.
    scope: SyntaxNode,
    defined_at: ra_ap_syntax::TextSize,
    /// The matcher ends with `$(,)?`.
    trailing_comma: bool,
    /// Standard macro names the transcriber invokes.
    macros: BTreeSet<String>,
}

/// Admitted exact pattern pins of every test in `source`. A source that does
/// not parse cleanly, or whose `assert_matches!` is not verified, yields
/// none.
pub(crate) fn pattern_pins(source: &str) -> PatternPins {
    let mut result = PatternPins::default();
    if !source.contains(MACRO_NAME) {
        return result;
    }
    let Some(parse) = parse_clean_source_file(source) else {
        return result;
    };
    let root = parse.tree();
    let Some(verified) = verified_macro(root.syntax()) else {
        return result;
    };
    let lines = LineIndex::new(source);
    let mut seen = BTreeMap::<TestKey, usize>::new();
    for function in root.syntax().descendants().filter_map(ast::Fn::cast) {
        let (Some(name), Some(token)) = (function.name(), function.fn_token()) else {
            continue;
        };
        let key = (
            lines.line(token.text_range().start()),
            lines.line_for_range_end(function.syntax().text_range().end()),
            name.text().to_string(),
        );
        *seen.entry(key.clone()).or_default() += 1;
        let Some(pins) = test_pins(&function, &verified, source, &lines) else {
            continue;
        };
        result.tests.insert(
            key,
            TestPins {
                body: slice_text(
                    source,
                    token.text_range().start(),
                    function.syntax().text_range().end(),
                ),
                pins,
            },
        );
    }
    result.tests.retain(|key, _| seen.get(key) == Some(&1));
    result
}

fn verified_macro(root: &SyntaxNode) -> Option<VerifiedMacro> {
    let mut definitions = Vec::new();
    for node in root.descendants() {
        if let Some(attr) = ast::Attr::cast(node.clone())
            && attr.syntax().text().to_string().contains("macro_use")
        {
            return None;
        }
        if ast::ExternCrate::can_cast(node.kind()) {
            return None;
        }
        if let Some(rename) = ast::Rename::cast(node.clone())
            && rename
                .name()
                .is_some_and(|name| name.text().trim_start_matches("r#") == MACRO_NAME)
        {
            return None;
        }
        if let Some(tree) = ast::UseTree::cast(node.clone())
            && tree
                .path()
                .and_then(|path| path.segment())
                .and_then(|segment| segment.name_ref())
                .is_some_and(|name| name.text().trim_start_matches("r#") == MACRO_NAME)
        {
            return None;
        }
        if let Some(call) = ast::MacroCall::cast(node.clone())
            && call.syntax().parent().is_some_and(|parent| {
                ast::SourceFile::can_cast(parent.kind()) || ast::ItemList::can_cast(parent.kind())
            })
        {
            // An item-position expansion may define another macro.
            return None;
        }
        if let Some(rules) = ast::MacroRules::cast(node.clone())
            && rules
                .name()
                .is_some_and(|name| name.text().trim_start_matches("r#") == MACRO_NAME)
        {
            definitions.push(rules);
        } else if let Some(definition) = ast::MacroDef::cast(node.clone())
            && definition
                .name()
                .is_some_and(|name| name.text().trim_start_matches("r#") == MACRO_NAME)
        {
            return None;
        }
    }
    let [definition] = definitions.as_slice() else {
        return None;
    };
    if definition.attrs().next().is_some() {
        return None;
    }
    let scope = definition.syntax().parent().filter(|parent| {
        ast::SourceFile::can_cast(parent.kind()) || ast::ItemList::can_cast(parent.kind())
    })?;
    let rules = definition.token_tree()?;
    let mut elements = rules
        .syntax()
        .children_with_tokens()
        .filter(|element| !element.kind().is_trivia());
    // Opening delimiter of the rule list.
    elements.next()?;
    let matcher = ast::TokenTree::cast(elements.next()?.into_node()?)?;
    if elements.next()?.as_token()?.text() != "=" || elements.next()?.as_token()?.text() != ">" {
        return None;
    }
    let transcriber = ast::TokenTree::cast(elements.next()?.into_node()?)?;
    let (value, pattern, trailing_comma) = first_rule_matcher(&matcher)?;
    let macros = panicking_transcriber(&transcriber, &value, &pattern)?;
    Some(VerifiedMacro {
        scope,
        defined_at: definition.syntax().text_range().end(),
        trailing_comma,
        macros,
    })
}

/// Non-trivia tokens inside a token tree, without its own delimiters.
fn inner_tokens(tree: &ast::TokenTree) -> Vec<SyntaxToken> {
    let mut tokens: Vec<_> = tree
        .syntax()
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .filter(|token| !token.kind().is_trivia())
        .collect();
    if tokens.len() < 2 {
        return Vec::new();
    }
    tokens.remove(0);
    tokens.pop();
    tokens
}

/// `$v:expr, $p:pat` with an optional `$(,)?`: the two metavariable names
/// and whether the trailing comma form is present.
fn first_rule_matcher(matcher: &ast::TokenTree) -> Option<(String, String, bool)> {
    let tokens = inner_tokens(matcher);
    let text: Vec<_> = tokens.iter().map(SyntaxToken::text).collect();
    let trailing_comma = match text.len() {
        9 => false,
        14 if text[9..] == ["$", "(", ",", ")", "?"] => true,
        _ => return None,
    };
    let head_ok = text[0] == "$"
        && tokens[1].kind() == SyntaxKind::IDENT
        && text[2] == ":"
        && text[3] == "expr"
        && text[4] == ","
        && text[5] == "$"
        && tokens[6].kind() == SyntaxKind::IDENT
        && text[7] == ":"
        && matches!(text[8], "pat" | "pat_param")
        && text[1] != text[6];
    head_ok.then(|| (text[1].to_string(), text[6].to_string(), trailing_comma))
}

const VALUE_PLACEHOLDER: &str = "__ripr_value";
const PATTERN_PLACEHOLDER: &str = "__RiprPattern";

/// Whether the rule's transcriber is a `match` on the value whose pattern
/// arm does nothing and whose catch-all arm panics. Returns the standard
/// macro names it invokes.
fn panicking_transcriber(
    transcriber: &ast::TokenTree,
    value: &str,
    pattern: &str,
) -> Option<BTreeSet<String>> {
    // Rebuild the transcriber with its own spacing (token trees keep `=>`
    // as two adjacent tokens), each metavariable replaced by a placeholder.
    let mut tokens: Vec<_> = transcriber
        .syntax()
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .collect();
    if tokens.len() < 2 {
        return None;
    }
    tokens.remove(0);
    tokens.pop();
    let mut text = String::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token.kind() == SyntaxKind::DOLLAR || token.text() == "$" {
            let name = tokens.get(index + 1)?;
            let replacement = if name.text() == value {
                VALUE_PLACEHOLDER
            } else if name.text() == pattern {
                PATTERN_PLACEHOLDER
            } else {
                return None;
            };
            text.push_str(replacement);
            index += 2;
            continue;
        }
        text.push_str(token.text());
        index += 1;
    }
    let source = format!("fn __ripr_expansion() {{ {text} }}");
    let parse = parse_clean_source_file(&source)?;
    let function = single_function(&parse.tree())?;
    let expression = single_expression(&function.body()?)?;
    let matched = ast::MatchExpr::cast(unwrap_blocks(expression.syntax().clone()))?;
    let scrutinee = matched.expr()?;
    if unparenthesized(scrutinee.syntax()).text() != VALUE_PLACEHOLDER {
        return None;
    }
    let arms: Vec<_> = matched.match_arm_list()?.arms().collect();
    let [pin, catch_all] = arms.as_slice() else {
        return None;
    };
    if pin.guard().is_some() || catch_all.guard().is_some() {
        return None;
    }
    if pin.pat()?.syntax().text() != PATTERN_PLACEHOLDER || !does_nothing(&pin.expr()?) {
        return None;
    }
    match catch_all.pat()? {
        ast::Pat::WildcardPat(_) => {}
        // A lowercase name binds; a capitalized one may name a constant.
        ast::Pat::IdentPat(binding)
            if binding.pat().is_none()
                && binding.name().is_some_and(|name| {
                    name.text()
                        .starts_with(|ch: char| ch.is_ascii_lowercase() || ch == '_')
                }) => {}
        _ => return None,
    }
    let panic = ast::MacroExpr::cast(unwrap_blocks(catch_all.expr()?.syntax().clone()))?;
    let call = panic.macro_call()?;
    let name = bare_macro_name(&call)?;
    if !matches!(name.as_str(), "panic" | "unreachable") {
        return None;
    }
    let mut macros = BTreeSet::from([name]);
    macros.extend(operand_macros(&call.token_tree()?)?);
    Some(macros)
}

fn single_function(file: &ast::SourceFile) -> Option<ast::Fn> {
    let items: Vec<_> = file.syntax().children().collect();
    let [item] = items.as_slice() else {
        return None;
    };
    ast::Fn::cast(item.clone())
}

/// The block's only expression: its tail with no statements, or its only
/// expression statement with no tail.
fn single_expression(block: &ast::BlockExpr) -> Option<ast::Expr> {
    let list = block.stmt_list()?;
    let statements: Vec<_> = list.statements().collect();
    match (statements.as_slice(), list.tail_expr()) {
        ([], Some(tail)) => Some(tail),
        ([ast::Stmt::ExprStmt(statement)], None) => statement.expr(),
        _ => None,
    }
}

/// Strips plain `{ expr }` blocks (no label or qualifier) around one
/// expression.
fn unwrap_blocks(mut node: SyntaxNode) -> SyntaxNode {
    while let Some(block) = ast::BlockExpr::cast(node.clone()) {
        if block.label().is_some()
            || block.async_token().is_some()
            || block.unsafe_token().is_some()
            || block.const_token().is_some()
            || block.try_block_modifier().is_some()
            || block.gen_token().is_some()
        {
            break;
        }
        let Some(inner) = single_expression(&block) else {
            break;
        };
        node = inner.syntax().clone();
    }
    node
}

fn unparenthesized(node: &SyntaxNode) -> SyntaxNode {
    let mut node = node.clone();
    while let Some(paren) = ast::ParenExpr::cast(node.clone()) {
        let Some(inner) = paren.expr() else {
            break;
        };
        node = inner.syntax().clone();
    }
    node
}

/// `{}` or `()`.
fn does_nothing(expression: &ast::Expr) -> bool {
    match expression {
        ast::Expr::BlockExpr(block) => {
            block.label().is_none()
                && block.async_token().is_none()
                && block.unsafe_token().is_none()
                && block.const_token().is_none()
                && block.try_block_modifier().is_none()
                && block.gen_token().is_none()
                && block.stmt_list().is_some_and(|list| {
                    list.statements().next().is_none() && list.tail_expr().is_none()
                })
        }
        ast::Expr::TupleExpr(tuple) => tuple.fields().next().is_none(),
        _ => false,
    }
}

fn bare_macro_name(call: &ast::MacroCall) -> Option<String> {
    let path = call.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    let segment = path.segment()?;
    let name = segment.name_ref()?.text().to_string();
    (segment.syntax().text() == name.as_str()).then_some(name)
}

/// The macros a token tree's operands invoke, when none can leave the
/// enclosing function: no `return`, `break`, `continue`, `yield` or `?`
/// token, and every nested invocation names a non-exiting macro.
fn operand_macros(tree: &ast::TokenTree) -> Option<BTreeSet<String>> {
    let tokens = inner_tokens(tree);
    if tokens.iter().any(|token| {
        matches!(
            token.text(),
            "return" | "break" | "continue" | "yield" | "?"
        )
    }) {
        return None;
    }
    let mut macros = BTreeSet::new();
    for window in tokens.windows(3) {
        if window[0].kind() == SyntaxKind::IDENT
            && window[1].text() == "!"
            && matches!(window[2].text(), "(" | "[" | "{")
        {
            let name = window[0].text();
            if !NON_EXITING_OPERAND_MACROS.contains(&name) {
                return None;
            }
            macros.insert(name.to_string());
        }
    }
    Some(macros)
}

/// The admitted pins of one test, or `None` when it is not a plain test.
fn test_pins(
    function: &ast::Fn,
    verified: &VerifiedMacro,
    source: &str,
    lines: &LineIndex,
) -> Option<BTreeMap<PinKey, PatternPin>> {
    let attributes: Vec<_> = function.attrs().collect();
    let [attribute] = attributes.as_slice() else {
        return None;
    };
    let plain_test = attribute
        .syntax()
        .text()
        .to_string()
        .split_whitespace()
        .collect::<String>()
        == "#[test]";
    let start = function.syntax().text_range().start();
    if !plain_test
        || function.async_token().is_some()
        || function.const_token().is_some()
        || function.unsafe_token().is_some()
        || function.generic_param_list().is_some()
        || function.where_clause().is_some()
        || function.ret_type().is_some()
        || function.param_list()?.params().next().is_some()
        || function.param_list()?.self_param().is_some()
        || !supported_item_context(function.syntax())
        || verified.defined_at > start
        || !function
            .syntax()
            .ancestors()
            .any(|ancestor| ancestor == verified.scope)
    {
        return None;
    }
    let list = function.body()?.stmt_list()?;
    let mut statements: Vec<SyntaxNode> = list
        .statements()
        .map(|statement| statement.syntax().clone())
        .collect();
    if let Some(tail) = list.tail_expr() {
        statements.push(tail.syntax().clone());
    }
    let mut pins = BTreeMap::new();
    let mut duplicates = BTreeSet::new();
    let mut relied = BTreeSet::new();
    for statement in statements {
        let call = statement_macro_call(&statement);
        if let Some(call) = call.as_ref()
            && bare_macro_name(call).as_deref() == Some(MACRO_NAME)
        {
            let Some((value, exact)) = invocation_operands(call, verified) else {
                break;
            };
            if exact {
                let range = call.syntax().text_range();
                let key = (
                    lines.line(range.start()),
                    slice_macro_call_text(source, range.start(), range.end()),
                );
                let mut macros = verified.macros.clone();
                macros.extend(relied.iter().cloned());
                if pins
                    .insert(key.clone(), PatternPin { value, macros })
                    .is_some()
                {
                    duplicates.insert(key);
                }
            }
            continue;
        }
        if let Some(call) = call {
            let Some(name) = bare_macro_name(&call) else {
                break;
            };
            if !STANDARD_ASSERTIONS.contains(&name.as_str()) {
                break;
            }
            let Some(nested) = call.token_tree().and_then(|tree| operand_macros(&tree)) else {
                break;
            };
            relied.insert(name);
            relied.extend(nested);
            continue;
        }
        if !completes_or_fails(&statement) {
            break;
        }
    }
    for key in duplicates {
        pins.remove(&key);
    }
    Some(pins)
}

/// The macro call a statement (or tail expression) consists of.
fn statement_macro_call(statement: &SyntaxNode) -> Option<ast::MacroCall> {
    if let Some(call) = ast::MacroCall::cast(statement.clone()) {
        return Some(call);
    }
    let expression = match ast::ExprStmt::cast(statement.clone()) {
        Some(statement) => statement.expr()?,
        None => ast::Expr::cast(statement.clone())?,
    };
    ast::MacroExpr::cast(expression.syntax().clone())?.macro_call()
}

/// A `let` binding a plain name (or `_`) from an allowlisted expression, or
/// an allowlisted expression statement. Every other statement, including
/// items, `let ... else` and any control flow, is refused.
fn completes_or_fails(statement: &SyntaxNode) -> bool {
    if let Some(binding) = ast::LetStmt::cast(statement.clone()) {
        let plain = match binding.pat() {
            Some(ast::Pat::IdentPat(pattern)) => pattern.pat().is_none(),
            Some(ast::Pat::WildcardPat(_)) => true,
            _ => false,
        };
        return plain
            && binding.let_else().is_none()
            && binding
                .initializer()
                .is_some_and(|value| allowlisted_expression(value.syntax()));
    }
    if let Some(statement) = ast::ExprStmt::cast(statement.clone()) {
        return statement
            .expr()
            .is_some_and(|value| allowlisted_expression(value.syntax()));
    }
    false
}

/// Every node is a call, path, literal, operator, field, index, reference,
/// cast, tuple, array, record literal or a type inside a path. No block,
/// closure, branch, loop, exit, `?`, `.await` or macro.
fn allowlisted_expression(node: &SyntaxNode) -> bool {
    node.descendants().all(|node| {
        matches!(
            node.kind(),
            SyntaxKind::CALL_EXPR
                | SyntaxKind::METHOD_CALL_EXPR
                | SyntaxKind::ARG_LIST
                | SyntaxKind::PATH_EXPR
                | SyntaxKind::PATH
                | SyntaxKind::PATH_SEGMENT
                | SyntaxKind::NAME_REF
                | SyntaxKind::LITERAL
                | SyntaxKind::PAREN_EXPR
                | SyntaxKind::TUPLE_EXPR
                | SyntaxKind::ARRAY_EXPR
                | SyntaxKind::REF_EXPR
                | SyntaxKind::PREFIX_EXPR
                | SyntaxKind::BIN_EXPR
                | SyntaxKind::FIELD_EXPR
                | SyntaxKind::INDEX_EXPR
                | SyntaxKind::CAST_EXPR
                | SyntaxKind::RECORD_EXPR
                | SyntaxKind::RECORD_EXPR_FIELD_LIST
                | SyntaxKind::RECORD_EXPR_FIELD
                | SyntaxKind::GENERIC_ARG_LIST
                | SyntaxKind::TYPE_ARG
                | SyntaxKind::PATH_TYPE
                | SyntaxKind::REF_TYPE
                | SyntaxKind::TUPLE_TYPE
                | SyntaxKind::ARRAY_TYPE
                | SyntaxKind::SLICE_TYPE
                | SyntaxKind::INFER_TYPE
        )
    })
}

/// The value operand's text and whether the pattern is exact, when the
/// invocation is one expression and one pattern the verified first rule
/// takes. `None` refuses the invocation.
fn invocation_operands(call: &ast::MacroCall, verified: &VerifiedMacro) -> Option<(String, bool)> {
    let tree = call.token_tree()?;
    let children: Vec<_> = tree
        .syntax()
        .children_with_tokens()
        .filter(|element| !element.kind().is_trivia())
        .collect();
    // Drop the delimiters; any of `()`, `[]` and `{}` invokes the same rule.
    let inner = children.get(1..children.len().checked_sub(1)?)?;
    let mut groups: Vec<Vec<&ra_ap_syntax::SyntaxElement>> = vec![Vec::new()];
    for element in inner {
        if element.kind() == SyntaxKind::COMMA {
            groups.push(Vec::new());
        } else {
            groups.last_mut()?.push(element);
        }
    }
    let operands = match groups.as_slice() {
        [value, pattern] => [value, pattern],
        [value, pattern, last] if verified.trailing_comma && last.is_empty() => [value, pattern],
        _ => return None,
    };
    let offset = tree.syntax().text_range().start();
    let tree_text = tree.syntax().text().to_string();
    let span = |group: &[&ra_ap_syntax::SyntaxElement]| -> Option<String> {
        let from = usize::from(group.first()?.text_range().start() - offset);
        let to = usize::from(group.last()?.text_range().end() - offset);
        tree_text.get(from..to).map(str::to_string)
    };
    let (value, pattern) = (span(operands[0])?, span(operands[1])?);
    let value_source = format!("fn __ripr_operand() {{ {value} }}");
    let parsed = parse_clean_source_file(&value_source)?;
    let function = single_function(&parsed.tree())?;
    let list = function.body()?.stmt_list()?;
    let tail = list.tail_expr()?;
    if list.statements().next().is_some()
        || tail.syntax().text() != value.as_str()
        || !allowlisted_expression(tail.syntax())
    {
        return None;
    }
    let pattern_source = format!("fn __ripr_operand() {{ let {pattern}; }}");
    let parsed = parse_clean_source_file(&pattern_source)?;
    let function = single_function(&parsed.tree())?;
    let list = function.body()?.stmt_list()?;
    let statements: Vec<_> = list.statements().collect();
    let [ast::Stmt::LetStmt(binding)] = statements.as_slice() else {
        return None;
    };
    let parsed_pattern = binding.pat()?;
    if list.tail_expr().is_some()
        || binding.initializer().is_some()
        || binding.ty().is_some()
        || parsed_pattern.syntax().text() != pattern.as_str()
    {
        return None;
    }
    Some((value, exact_pattern(&parsed_pattern)))
}

/// A pattern that matches exactly one value: constructors, paths, literals
/// and tuples of exact patterns. A binding (`x`, `None` read as a binding
/// candidate), `_`, `..`, a range, an alternative, a reference or a record
/// pattern is not exact.
fn exact_pattern(pattern: &ast::Pat) -> bool {
    match pattern {
        ast::Pat::LiteralPat(_) | ast::Pat::PathPat(_) => true,
        ast::Pat::ParenPat(inner) => inner.pat().is_some_and(|inner| exact_pattern(&inner)),
        ast::Pat::TuplePat(tuple) => tuple.fields().all(|field| exact_pattern(&field)),
        ast::Pat::TupleStructPat(constructor) => {
            constructor.path().is_some() && constructor.fields().all(|field| exact_pattern(&field))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests;
