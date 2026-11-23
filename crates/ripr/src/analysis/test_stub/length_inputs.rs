//! #8361: concrete inputs for a boundary on a string parameter's length.
//!
//! A boundary stub writes an integer parameter's boundary value
//! (`qty = 10` for `qty > 10`), but a comparison on a string's length left
//! the string as `todo!("ripr: choose `s`")`. This module writes a string
//! whose length sits exactly on the boundary when the comparison alone
//! decides it:
//!
//! - one side is `p.len()` or `p.chars().count()` for a `&str` parameter
//!   `p`, or a local bound once by `let n = p.len();` (or `.chars().count()`)
//!   before the comparison;
//! - the other side is a non-negative integer literal, a file-root `const`
//!   of integer type whose value is such a literal (for an owner outside any
//!   inline module, with no other item, import or item macro of that name),
//!   or the length of another `&str` parameter;
//! - the length is at most [`MAX_LENGTH`];
//! - the parameter is not `mut`, and nothing before the comparison rebinds,
//!   assigns or names it inside a macro call; a local or a `const` bound is
//!   only read when the owner's body calls no macro at all.
//!
//! The string is `x` repeated, so its byte length and char count agree. Two
//! parameters compared with each other get equal lengths and different text
//! (`"ab"`, `"cd"`), so a test can tell which one comes back. Anything else
//! keeps the labelled `todo!()`. The value is an input on the boundary, not
//! an expected value.

use super::super::seams::{RepoSeam, SeamKind};
use super::super::syntax::fn_signature::OwnerSignature;
use super::super::syntax::parse_clean_source_file;
use super::match_inputs::{is_str_ref, parse_integer, unraw, unstable_before_offset};
use ra_ap_syntax::ast::{self, HasArgList, HasGenericArgs, HasName};
use ra_ap_syntax::{AstNode, SourceFile, SyntaxKind, TextSize};

/// The longest boundary string written out.
const MAX_LENGTH: i128 = 64;

/// One side of the changed comparison.
enum Side {
    /// The length of this `&str` parameter.
    Length(String),
    /// A known non-negative integer.
    Number(i128),
}

/// `(binding, value)` for each `&str` parameter the boundary decides.
pub(super) fn length_inputs(
    seam: &RepoSeam,
    signature: &OwnerSignature,
    source: &str,
) -> Vec<(String, String)> {
    derive(seam, signature, source).unwrap_or_default()
}

fn derive(
    seam: &RepoSeam,
    signature: &OwnerSignature,
    source: &str,
) -> Option<Vec<(String, String)>> {
    if seam.kind() != SeamKind::PredicateBoundary {
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
    if unraw(owner.name()?.text()) != unraw(&signature.name) {
        return None;
    }
    let body = owner.body()?;
    // The changed comparison: the innermost relational or equality
    // expression at the seam whose text is the seam's expression.
    let wanted = normalized(
        seam.expression()
            .trim()
            .strip_prefix("if ")
            .unwrap_or(seam.expression()),
    );
    let comparison = body
        .syntax()
        .descendants()
        .filter_map(ast::BinExpr::cast)
        .filter(|bin| {
            bin.syntax().text_range().contains_inclusive(offset)
                && matches!(bin.op_kind(), Some(ast::BinaryOp::CmpOp(_)))
                && normalized(&bin.syntax().text().to_string()) == wanted
        })
        .min_by_key(|bin| bin.syntax().text_range().len())?;
    let at = comparison.syntax().text_range().start();
    let context = Context {
        owner: &owner,
        body: &body,
        file: &file,
        signature,
        at,
        has_macro: body
            .syntax()
            .descendants()
            .any(|node| node.kind() == SyntaxKind::MACRO_CALL),
    };
    let lhs = context.side(&comparison.lhs()?)?;
    let rhs = context.side(&comparison.rhs()?)?;
    let value = |length: i128| -> Option<String> {
        if !(0..=MAX_LENGTH).contains(&length) {
            return None;
        }
        Some(format!("\"{}\"", "x".repeat(usize::try_from(length).ok()?)))
    };
    match (lhs, rhs) {
        (Side::Length(param), Side::Number(length))
        | (Side::Number(length), Side::Length(param)) => Some(vec![(param, value(length)?)]),
        (Side::Length(left), Side::Length(right)) if left != right => Some(vec![
            (left, "\"ab\"".to_string()),
            (right, "\"cd\"".to_string()),
        ]),
        _ => None,
    }
}

struct Context<'a> {
    owner: &'a ast::Fn,
    body: &'a ast::BlockExpr,
    file: &'a SourceFile,
    signature: &'a OwnerSignature,
    /// Where the changed comparison starts.
    at: TextSize,
    /// The body calls a macro, which may bind a local or define a `const`
    /// of any name (`macro_rules!` items are not hygienic).
    has_macro: bool,
}

impl Context<'_> {
    fn side(&self, expr: &ast::Expr) -> Option<Side> {
        if let Some(param) = self.length_of(expr) {
            return Some(Side::Length(param));
        }
        match expr {
            ast::Expr::ParenExpr(paren) => self.side(&paren.expr()?),
            ast::Expr::Literal(literal) => match literal.kind() {
                ast::LiteralKind::IntNumber(_) => {
                    let value = parse_integer(&literal.syntax().text().to_string())?;
                    (value >= 0).then_some(Side::Number(value))
                }
                _ => None,
            },
            ast::Expr::PathExpr(path) => {
                let name = single_segment(path)?;
                if let Some(param) = self.local_length(&name) {
                    return Some(Side::Length(param));
                }
                self.constant(&name).map(Side::Number)
            }
            _ => None,
        }
    }

    /// The `&str` parameter whose length `expr` is: `p.len()` or
    /// `p.chars().count()`.
    fn length_of(&self, expr: &ast::Expr) -> Option<String> {
        let ast::Expr::MethodCallExpr(call) = expr else {
            return None;
        };
        if call.generic_arg_list().is_some() || call.arg_list()?.args().next().is_some() {
            return None;
        }
        let method = call.name_ref()?.text().to_string();
        let receiver = call.receiver()?;
        let target = match method.as_str() {
            "len" => receiver,
            "count" => {
                let ast::Expr::MethodCallExpr(chars) = receiver else {
                    return None;
                };
                if chars.name_ref()?.text() != "chars"
                    || chars.generic_arg_list().is_some()
                    || chars.arg_list()?.args().next().is_some()
                {
                    return None;
                }
                chars.receiver()?
            }
            _ => return None,
        };
        let ast::Expr::PathExpr(path) = target else {
            return None;
        };
        let name = single_segment(&path)?;
        let param = self
            .signature
            .params
            .iter()
            .find(|param| param.name.as_deref().map(unraw) == Some(unraw(&name)))?;
        if !is_str_ref(&param.ty) || unstable_before_offset(self.owner, &name, self.at) {
            return None;
        }
        param.name.clone()
    }

    /// The parameter whose length the local `name` holds: the body binds
    /// `name` exactly once, by an immutable `let name = <length>;` before
    /// the comparison (so it cannot be assigned). A parameter of that name
    /// is not a local.
    fn local_length(&self, name: &str) -> Option<String> {
        let is_param = self
            .signature
            .params
            .iter()
            .any(|param| param.name.as_deref().map(unraw) == Some(unraw(name)));
        if is_param || self.has_macro {
            return None;
        }
        let bindings = self
            .body
            .syntax()
            .descendants()
            .filter_map(ast::IdentPat::cast)
            .filter(|ident| ident.name().is_some_and(|n| unraw(n.text()) == unraw(name)))
            .collect::<Vec<_>>();
        let [ident] = bindings.as_slice() else {
            return None;
        };
        if ident.mut_token().is_some() || ident.ref_token().is_some() || ident.pat().is_some() {
            return None;
        }
        let statement = ast::LetStmt::cast(ident.syntax().parent()?)?;
        if statement.let_else().is_some()
            || statement.syntax().text_range().end() > self.at
            || statement.syntax().parent()?.parent()? != *self.body.syntax()
        {
            return None;
        }
        self.length_of(&statement.initializer()?)
    }

    /// The value of the file-root `const name: <integer> = <literal>;`, when
    /// it is the only item of that name in the file and nothing imports or
    /// renames another one in.
    fn constant(&self, name: &str) -> Option<i128> {
        let named = |node: &ra_ap_syntax::SyntaxNode| {
            node.children()
                .find_map(ast::Name::cast)
                .is_some_and(|n| unraw(n.text()) == unraw(name))
        };
        // A bare name in an owner inside a module resolves in that module.
        if self.has_macro
            || self
                .owner
                .syntax()
                .ancestors()
                .any(|node| node.kind() == SyntaxKind::MODULE)
        {
            return None;
        }
        let mentions = |node: &ra_ap_syntax::SyntaxNode| {
            node.descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .any(|token| unraw(token.text()) == unraw(name))
        };
        let mut found = None;
        for node in self.file.syntax().descendants() {
            let item_macro = node.kind() == SyntaxKind::MACRO_CALL
                && node.parent().is_some_and(|parent| {
                    matches!(
                        parent.kind(),
                        SyntaxKind::SOURCE_FILE | SyntaxKind::ITEM_LIST
                    )
                });
            match node.kind() {
                SyntaxKind::USE_TREE | SyntaxKind::RENAME if mentions(&node) => return None,
                SyntaxKind::MACRO_RULES | SyntaxKind::MACRO_DEF if mentions(&node) => return None,
                SyntaxKind::MACRO_CALL if item_macro && mentions(&node) => return None,
                _ if ast::Item::can_cast(node.kind()) && named(&node) => {
                    if found.is_some() || node.parent()?.kind() != SyntaxKind::SOURCE_FILE {
                        return None;
                    }
                    let constant = ast::Const::cast(node.clone())?;
                    let ty = constant.ty()?.syntax().text().to_string();
                    if !is_integer_type(ty.trim()) {
                        return None;
                    }
                    let ast::Expr::Literal(literal) = constant.body()? else {
                        return None;
                    };
                    let value = parse_integer(&literal.syntax().text().to_string())?;
                    if value < 0 {
                        return None;
                    }
                    found = Some(value);
                }
                _ => {}
            }
        }
        found
    }
}

fn single_segment(path: &ast::PathExpr) -> Option<String> {
    let path = path.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    let segment = path.segment()?;
    if segment.generic_arg_list().is_some() {
        return None;
    }
    Some(segment.name_ref()?.text().to_string())
}

fn is_integer_type(ty: &str) -> bool {
    matches!(
        ty,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
    )
}

fn normalized(text: &str) -> String {
    text.split_whitespace().collect()
}
