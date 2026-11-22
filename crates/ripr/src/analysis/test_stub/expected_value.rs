//! #8361: what the owner returns for a stub whose inputs decide the path.
//!
//! When every input that steers the owner's control flow is a value the stub
//! already wrote, and the owner returns a constant on the path those inputs
//! select, the `expected` fill-in's label names that constant, and for a
//! relational boundary also the constant the owner returns with the changed
//! operator's neighbor (`>` as `>=`). The stub never writes either as the
//! expected value: the changed code may be the bug, so which one is intended
//! stays the developer's choice, and the stub still stops at that `todo!()`.
//!
//! The evaluation is deliberately small and fail-closed. The owner has no
//! receiver, type generics, `async`, attributes or `mut` parameter type, and
//! no `let`, assignment, closure, macro or nested item in its body, which is
//! walked statement by statement:
//!
//! - `if COND { .. }` statements, with or without `else`, blocks and
//!   `return EXPR;`; a statement-position block whose tail is not a `return`
//!   discards its value and refuses, as does any statement with an attribute
//!   or of any other kind;
//! - the tail may be a constant, an `if`/`else` chain, a block, or a `match`
//!   on a parameter whose arms up to the taken one are literal, unit-variant
//!   or `_` patterns without guards or attributes;
//! - a condition compares a parameter with an integer, bool, char or string
//!   literal (`<`, `<=`, `>`, `>=`, `==`, `!=`), possibly combined with `&&`,
//!   `||`, `!` and parentheses;
//! - a constant is a literal, a unit variant of a file-local enum (only in an
//!   inline test module), `None`, a tuple of constants, `Some`/`Ok`/`Err` of
//!   one constant, or a parameter the stub wrote. A file that redeclares,
//!   imports or renames `Some`, `Ok`, `Err` or `None` by name refuses.
//!
//! Anything else keeps the labelled `todo!()`.

use super::super::seams::RepoSeam;
use super::super::syntax::fn_signature::OwnerSignature;
use super::super::syntax::parse_clean_source_file;
use super::match_inputs::{full_use_path, mentions, parse_integer, unit_variants, unraw};
use ra_ap_syntax::ast::{self, HasArgList, HasAttrs, HasName};
use ra_ap_syntax::{AstNode, SourceFile, SyntaxKind, TextRange, TextSize};

/// A value an input or a pattern names.
#[derive(Clone, PartialEq)]
enum Val {
    Int(i128),
    Bool(bool),
    /// A char or string literal without escapes, by its text.
    Text(String),
    /// A unit variant of a file-local enum, by its `Type::Variant` text.
    Variant(String),
}

struct Context<'a> {
    file: &'a SourceFile,
    /// `(parameter, written text, value)` for each input the stub wrote.
    inputs: Vec<(String, String, Option<Val>)>,
    allow_paths: bool,
    /// The changed comparison, evaluated with its neighbor operator
    /// (`>` as `>=`) when set.
    flip: Option<TextRange>,
}

/// What the owner returns for the stub's inputs, for the label of the
/// `expected` fill-in.
#[derive(Debug, PartialEq)]
pub(super) struct ExpectedHint {
    /// The constant the changed code returns.
    pub(super) changed: String,
    /// `(neighbor operator, constant)` with the changed comparison's
    /// operator replaced by its neighbor, for a relational boundary.
    pub(super) neighbor: Option<(&'static str, String)>,
}

/// What a statement does to the walk.
enum Flow {
    /// The owner returns this constant.
    Return(String),
    /// Control continues with the next statement.
    Next,
}

/// The constant the owner returns for `inputs` (`(binding, value text)`),
/// and for a relational boundary what it returns with the neighbor
/// operator, or `None` when the path or the value is not decided.
pub(super) fn expected_value(
    seam: &RepoSeam,
    signature: &OwnerSignature,
    source: &str,
    inputs: &[(String, String)],
    allow_paths: bool,
) -> Option<ExpectedHint> {
    if signature.receiver.is_some() || signature.has_type_generics || signature.is_async {
        return None;
    }
    let parse = parse_clean_source_file(source)?;
    let file = parse.tree();
    let offset = TextSize::try_from(seam.byte_offset()).ok()?;
    let owner = file
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|function| function.syntax().text_range().contains(offset))
        .max_by_key(|function| function.syntax().text_range().start())?;
    if owner.name()?.text() != signature.name.as_str() || owner.attrs().next().is_some() {
        return None;
    }
    // Every parameter is a plain, immutable binding of a non-`&mut` type.
    for param in owner.param_list()?.params() {
        let ast::Pat::IdentPat(ident) = param.pat()? else {
            return None;
        };
        if ident.mut_token().is_some() || ident.ref_token().is_some() || ident.pat().is_some() {
            return None;
        }
        let ty = param.ty()?;
        let mutable = ty
            .syntax()
            .descendants_with_tokens()
            .any(|element| element.kind() == SyntaxKind::MUT_KW);
        if mutable {
            return None;
        }
    }
    let body = owner.body()?;
    // A binding, assignment, closure or macro anywhere in the body could
    // change what a parameter or path names on the evaluated path, and an
    // attribute anywhere (`#[cfg]` on a statement, arm or tuple field) can
    // remove code the evaluator would otherwise count.
    let disturbs = body.syntax().descendants().any(|node| {
        matches!(
            node.kind(),
            SyntaxKind::ATTR
                | SyntaxKind::LET_STMT
                | SyntaxKind::MACRO_CALL
                | SyntaxKind::CLOSURE_EXPR
                | SyntaxKind::FN
                | SyntaxKind::CONST
                | SyntaxKind::STATIC
        ) || ast::BinExpr::cast(node.clone())
            .is_some_and(|bin| matches!(bin.op_kind(), Some(ast::BinaryOp::Assignment { .. })))
    });
    if disturbs {
        return None;
    }
    // `Some`, `Ok`, `Err` and `None` keep their prelude meaning.
    let prelude_shadowed = ["Some", "Ok", "Err", "None"].iter().any(|name| {
        file.syntax().descendants().any(|node| {
            matches!(
                node.kind(),
                SyntaxKind::USE_TREE
                    | SyntaxKind::RENAME
                    | SyntaxKind::MACRO_RULES
                    | SyntaxKind::MACRO_DEF
            ) && mentions(&node, name)
                || (ast::Item::can_cast(node.kind()) || ast::Variant::can_cast(node.kind()))
                    && node
                        .children()
                        .find_map(ast::Name::cast)
                        .is_some_and(|n| unraw(n.text()) == *name)
        })
    });
    // A glob import from elsewhere could bring in a `None` or `Some` the
    // file does not declare; only `super::*` / `self::*` stay in view.
    let foreign_glob = file
        .syntax()
        .descendants()
        .filter_map(ast::UseTree::cast)
        .any(|tree| {
            tree.star_token().is_some() && {
                let full = full_use_path(&tree);
                !(full.len() == 2
                    && full
                        .first()
                        .is_some_and(|first| first == "super" || first == "self"))
            }
        });
    if prelude_shadowed || foreign_glob {
        return None;
    }
    let mut context = Context {
        file: &file,
        inputs: inputs
            .iter()
            .map(|(name, text)| (name.clone(), text.clone(), input_value(text, &file)))
            .collect(),
        allow_paths,
        flip: None,
    };
    let changed = match context.block(&body.stmt_list()?, true)? {
        Flow::Return(value) => value,
        Flow::Next => return None,
    };
    // The innermost relational comparison at the seam, if any.
    let boundary = body
        .syntax()
        .descendants()
        .filter_map(ast::BinExpr::cast)
        .filter(|bin| bin.syntax().text_range().contains(offset))
        .filter(|bin| {
            matches!(
                bin.op_kind(),
                Some(ast::BinaryOp::CmpOp(ast::CmpOp::Ord { .. }))
            )
        })
        .max_by_key(|bin| bin.syntax().text_range().start());
    let neighbor = boundary.and_then(|bin| {
        let Some(ast::BinaryOp::CmpOp(ast::CmpOp::Ord { ordering, strict })) = bin.op_kind() else {
            return None;
        };
        context.flip = Some(bin.syntax().text_range());
        let value = match context.block(&body.stmt_list()?, true)? {
            Flow::Return(value) => value,
            Flow::Next => return None,
        };
        Some((operator(ordering, !strict), value))
    });
    Some(ExpectedHint { changed, neighbor })
}

fn operator(ordering: ast::Ordering, strict: bool) -> &'static str {
    match (ordering, strict) {
        (ast::Ordering::Less, true) => "<",
        (ast::Ordering::Less, false) => "<=",
        (ast::Ordering::Greater, true) => ">",
        (ast::Ordering::Greater, false) => ">=",
    }
}

impl Context<'_> {
    /// A block's flow. `value` says whether the block's tail is the value
    /// the owner returns (the body, or a branch in tail position); in a
    /// statement position only a `return` tail returns, and any other tail
    /// value is discarded, so it refuses.
    fn block(&self, list: &ast::StmtList, value: bool) -> Option<Flow> {
        if list.attrs().next().is_some() {
            return None;
        }
        for statement in list.statements() {
            let ast::Stmt::ExprStmt(statement) = statement else {
                return None;
            };
            let expr = statement.expr()?;
            if has_attrs(&expr) {
                return None;
            }
            if let Flow::Return(value) = self.statement(&expr)? {
                return Some(Flow::Return(value));
            }
        }
        match list.tail_expr() {
            Some(tail) if has_attrs(&tail) => None,
            Some(ast::Expr::ReturnExpr(ret)) => self.constant(&ret.expr()?).map(Flow::Return),
            Some(tail) if value => self.tail(&tail).map(Flow::Return),
            Some(_) => None,
            None => Some(Flow::Next),
        }
    }

    /// An expression statement: an `if` or a `return`.
    fn statement(&self, expr: &ast::Expr) -> Option<Flow> {
        match expr {
            ast::Expr::IfExpr(if_expr) => self.if_flow(if_expr, false),
            ast::Expr::ReturnExpr(ret) => self.constant(&ret.expr()?).map(Flow::Return),
            ast::Expr::BlockExpr(block) if block.modifier().is_none() => {
                self.block(&block.stmt_list()?, false)
            }
            _ => None,
        }
    }

    fn if_flow(&self, if_expr: &ast::IfExpr, value: bool) -> Option<Flow> {
        if if_expr.attrs().next().is_some() {
            return None;
        }
        let then = if_expr.then_branch()?;
        if self.condition(&if_expr.condition()?)? {
            return self.block(&then.stmt_list()?, value);
        }
        match if_expr.else_branch() {
            None => Some(Flow::Next),
            Some(ast::ElseBranch::Block(block)) => self.block(&block.stmt_list()?, value),
            Some(ast::ElseBranch::IfExpr(nested)) => self.if_flow(&nested, value),
        }
    }

    /// The constant a tail expression evaluates to.
    fn tail(&self, expr: &ast::Expr) -> Option<String> {
        match expr {
            ast::Expr::IfExpr(if_expr) => match self.if_flow(if_expr, true)? {
                Flow::Return(value) => Some(value),
                Flow::Next => None,
            },
            ast::Expr::BlockExpr(block) if block.modifier().is_none() => {
                match self.block(&block.stmt_list()?, true)? {
                    Flow::Return(value) => Some(value),
                    Flow::Next => None,
                }
            }
            ast::Expr::MatchExpr(match_expr) => self.match_value(match_expr),
            ast::Expr::ReturnExpr(ret) => self.constant(&ret.expr()?),
            ast::Expr::ParenExpr(paren) => self.tail(&paren.expr()?),
            other => self.constant(other),
        }
    }

    /// The arm a `match` on a written parameter takes, evaluated.
    fn match_value(&self, match_expr: &ast::MatchExpr) -> Option<String> {
        let scrutinee = self.param_value(&match_expr.expr()?)?;
        for arm in match_expr.match_arm_list()?.arms() {
            if arm.guard().is_some() || arm.attrs().next().is_some() {
                return None;
            }
            let pat = arm.pat()?;
            if self.pattern_matches(&pat, &scrutinee)? {
                return self.tail(&arm.expr()?);
            }
        }
        None
    }

    /// Whether `pat` matches `value`; `None` when not decided.
    fn pattern_matches(&self, pat: &ast::Pat, value: &Val) -> Option<bool> {
        match pat {
            ast::Pat::OrPat(or) => {
                for alternative in or.pats() {
                    if self.pattern_matches(&alternative, value)? {
                        return Some(true);
                    }
                }
                Some(false)
            }
            ast::Pat::WildcardPat(_) => Some(true),
            ast::Pat::LiteralPat(_) | ast::Pat::PathPat(_) => {
                let theirs = pattern_value(pat, self.file)?;
                pattern_same(&theirs, value)
            }
            _ => None,
        }
    }

    fn condition(&self, expr: &ast::Expr) -> Option<bool> {
        match expr {
            ast::Expr::ParenExpr(paren) => self.condition(&paren.expr()?),
            ast::Expr::PrefixExpr(prefix) => {
                if prefix.op_kind()? != ast::UnaryOp::Not {
                    return None;
                }
                self.condition(&prefix.expr()?).map(|value| !value)
            }
            ast::Expr::Literal(_) | ast::Expr::PathExpr(_) => match self.operand(expr)? {
                Val::Bool(value) => Some(value),
                _ => None,
            },
            ast::Expr::BinExpr(bin) => {
                let (lhs, rhs) = (bin.lhs()?, bin.rhs()?);
                match bin.op_kind()? {
                    ast::BinaryOp::LogicOp(ast::LogicOp::And) => {
                        Some(self.condition(&lhs)? && self.condition(&rhs)?)
                    }
                    ast::BinaryOp::LogicOp(ast::LogicOp::Or) => {
                        Some(self.condition(&lhs)? || self.condition(&rhs)?)
                    }
                    ast::BinaryOp::CmpOp(op) => {
                        let op = match op {
                            ast::CmpOp::Ord { ordering, strict }
                                if self.flip == Some(bin.syntax().text_range()) =>
                            {
                                ast::CmpOp::Ord {
                                    ordering,
                                    strict: !strict,
                                }
                            }
                            other => other,
                        };
                        compare(op, &self.operand(&lhs)?, &self.operand(&rhs)?)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// A literal or a written parameter, as a value.
    fn operand(&self, expr: &ast::Expr) -> Option<Val> {
        match expr {
            ast::Expr::ParenExpr(paren) => self.operand(&paren.expr()?),
            ast::Expr::Literal(_) => literal_value(&expr.syntax().text().to_string(), expr),
            ast::Expr::PathExpr(_) => self.param_value(expr),
            ast::Expr::PrefixExpr(prefix) if prefix.op_kind()? == ast::UnaryOp::Neg => {
                let ast::Expr::Literal(_) = prefix.expr()? else {
                    return None;
                };
                match self.operand(&prefix.expr()?)? {
                    Val::Int(value) => Some(Val::Int(-value)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The value of a written parameter named by a bare path.
    fn param_value(&self, expr: &ast::Expr) -> Option<Val> {
        let name = self.param_name(expr)?;
        self.inputs
            .iter()
            .find(|(input, _, _)| unraw(input) == unraw(&name))
            .and_then(|(_, _, value)| value.clone())
    }

    fn param_name(&self, expr: &ast::Expr) -> Option<String> {
        let ast::Expr::PathExpr(path) = expr else {
            return None;
        };
        let path = path.path()?;
        if path.qualifier().is_some() {
            return None;
        }
        Some(path.segment()?.name_ref()?.text().to_string())
    }

    /// The text of a constant expression, as the test writes it.
    fn constant(&self, expr: &ast::Expr) -> Option<String> {
        match expr {
            ast::Expr::Literal(_) => {
                let text = expr.syntax().text().to_string();
                literal_value(&text, expr)?;
                Some(text)
            }
            ast::Expr::PrefixExpr(prefix) if prefix.op_kind()? == ast::UnaryOp::Neg => {
                let ast::Expr::Literal(_) = prefix.expr()? else {
                    return None;
                };
                self.constant(&prefix.expr()?)
                    .map(|text| format!("-{text}"))
            }
            ast::Expr::ParenExpr(paren) => self.constant(&paren.expr()?),
            ast::Expr::TupleExpr(tuple) => {
                let fields = tuple
                    .fields()
                    .map(|field| self.constant(&field))
                    .collect::<Option<Vec<_>>>()?;
                Some(match fields.as_slice() {
                    [only] => format!("({only},)"),
                    _ => format!("({})", fields.join(", ")),
                })
            }
            ast::Expr::PathExpr(path) => {
                let path_node = path.path()?;
                if path_node.qualifier().is_none() {
                    let name = path_node.segment()?.name_ref()?.text().to_string();
                    if name == "None" {
                        return Some(name);
                    }
                    return self
                        .inputs
                        .iter()
                        .find(|(input, _, _)| unraw(input) == unraw(&name))
                        .map(|(_, text, _)| text.clone());
                }
                if !self.allow_paths {
                    return None;
                }
                match pattern_like_path(&path_node, self.file)? {
                    Val::Variant(text) => Some(text),
                    _ => None,
                }
            }
            ast::Expr::CallExpr(call) => {
                let callee = self.param_name(&call.expr()?)?;
                if !matches!(callee.as_str(), "Some" | "Ok" | "Err") {
                    return None;
                }
                let args: Vec<ast::Expr> = call.arg_list()?.args().collect();
                let [only] = args.as_slice() else {
                    return None;
                };
                Some(format!("{callee}({})", self.constant(only)?))
            }
            _ => None,
        }
    }
}

/// A literal's value: integers by parsed value, bools, and char or string
/// literals without escapes or raw prefix. Floats and byte literals are not
/// values here.
fn literal_value(text: &str, expr: &ast::Expr) -> Option<Val> {
    let ast::Expr::Literal(literal) = expr else {
        return None;
    };
    if text.chars().any(char::is_control) {
        return None;
    }
    match literal.kind() {
        ast::LiteralKind::IntNumber(_) => parse_integer(text).map(Val::Int),
        ast::LiteralKind::Bool(value) => Some(Val::Bool(value)),
        ast::LiteralKind::Char(_) | ast::LiteralKind::String(_) => {
            let plain = (text.starts_with('\'') || text.starts_with('"')) && !text.contains('\\');
            plain.then(|| Val::Text(text.to_string()))
        }
        _ => None,
    }
}

/// The value an input text names: an integer, a bool, a plain char or
/// string literal, or a unit variant of a file-local enum.
fn input_value(text: &str, file: &SourceFile) -> Option<Val> {
    let text = text.trim();
    if text.chars().any(char::is_control) {
        return None;
    }
    if text.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
        return parse_integer(text).map(Val::Int);
    }
    match text {
        "true" => return Some(Val::Bool(true)),
        "false" => return Some(Val::Bool(false)),
        _ => {}
    }
    if text.starts_with('\'') || text.starts_with('"') {
        return (!text.contains('\\')).then(|| Val::Text(text.to_string()));
    }
    let (enum_name, variant) = text.split_once("::")?;
    let ident =
        |part: &str| !part.is_empty() && part.chars().all(|c| c.is_alphanumeric() || c == '_');
    if !ident(enum_name) || !ident(variant) || enum_name == "Self" {
        return None;
    }
    unit_variants(file, enum_name)?
        .iter()
        .any(|known| known == variant)
        .then(|| Val::Variant(text.to_string()))
}

/// The value of a literal or path pattern.
fn pattern_value(pat: &ast::Pat, file: &SourceFile) -> Option<Val> {
    match pat {
        ast::Pat::LiteralPat(literal) => {
            let text = pat.syntax().text().to_string();
            match literal.literal()?.kind() {
                ast::LiteralKind::IntNumber(_) => parse_integer(&text).map(Val::Int),
                ast::LiteralKind::Bool(value) => Some(Val::Bool(value)),
                ast::LiteralKind::Char(_) | ast::LiteralKind::String(_) => {
                    let plain = (text.starts_with('\'') || text.starts_with('"'))
                        && !text.contains('\\')
                        && !text.chars().any(char::is_control);
                    plain.then_some(Val::Text(text))
                }
                _ => None,
            }
        }
        ast::Pat::PathPat(path) => pattern_like_path(&path.path()?, file),
        _ => None,
    }
}

/// A two-segment path naming a unit variant of the one file-local enum of
/// that name.
fn pattern_like_path(path: &ast::Path, file: &SourceFile) -> Option<Val> {
    let qualifier = path.qualifier()?;
    if qualifier.qualifier().is_some() {
        return None;
    }
    let generic = |segment: ast::PathSegment| {
        segment
            .syntax()
            .children()
            .any(|child| ast::GenericArgList::can_cast(child.kind()))
    };
    let (outer, inner) = (qualifier.segment()?, path.segment()?);
    if generic(outer.clone()) || generic(inner.clone()) {
        return None;
    }
    let enum_name = outer.name_ref()?.text().to_string();
    let variant = inner.name_ref()?.text().to_string();
    if enum_name == "Self" {
        return None;
    }
    unit_variants(file, &enum_name)?
        .contains(&variant)
        .then(|| Val::Variant(format!("{enum_name}::{variant}")))
}

/// Whether an expression carries an outer attribute (`#[cfg(..)] return 1;`).
fn has_attrs(expr: &ast::Expr) -> bool {
    expr.syntax()
        .children()
        .any(|child| child.kind() == SyntaxKind::ATTR)
}

/// Whether a pattern's value matches, which compares variants
/// structurally (unlike `==`, which may be a hand-written `PartialEq`).
fn pattern_same(pattern: &Val, value: &Val) -> Option<bool> {
    match (pattern, value) {
        (Val::Variant(a), Val::Variant(b)) => {
            let enum_of = |text: &str| text.split("::").next().map(str::to_string);
            (enum_of(a) == enum_of(b)).then_some(a == b)
        }
        _ => same(pattern, value),
    }
}

/// Whether two values are equal under `==`, when that is decided.
fn same(a: &Val, b: &Val) -> Option<bool> {
    match (a, b) {
        (Val::Int(a), Val::Int(b)) => Some(a == b),
        (Val::Bool(a), Val::Bool(b)) => Some(a == b),
        (Val::Text(a), Val::Text(b)) => Some(a == b),
        // A variant's `==` may be a hand-written `PartialEq`; only a match
        // pattern compares variants structurally.
        _ => None,
    }
}

fn compare(op: ast::CmpOp, a: &Val, b: &Val) -> Option<bool> {
    use ast::{CmpOp, Ordering};
    match op {
        CmpOp::Eq { negated } => same(a, b).map(|equal| equal != negated),
        CmpOp::Ord { ordering, strict } => {
            let (Val::Int(a), Val::Int(b)) = (a, b) else {
                return None;
            };
            Some(match (ordering, strict) {
                (Ordering::Less, true) => a < b,
                (Ordering::Less, false) => a <= b,
                (Ordering::Greater, true) => a > b,
                (Ordering::Greater, false) => a >= b,
            })
        }
    }
}
