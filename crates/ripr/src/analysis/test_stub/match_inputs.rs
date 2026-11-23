//! #8361: concrete inputs for a match-arm stub.
//!
//! A match-arm stub used to leave every parameter as
//! `todo!("ripr: choose `ev` so the `..` arm runs")`, even when the arm's own
//! pattern names the value: `Event::Fail => ..` on `match ev`, or `2 => ..` on
//! `match n`. This module reads the changed arm and writes that value when the
//! pattern alone decides it:
//!
//! - the scrutinee is a bare parameter, or a tuple of bare parameters and
//!   receiver fields (`(self.state, ev)`); a parameter whose position is `_`
//!   stays a fill-in, and a receiver field the arm constrains is reported back
//!   so the receiver line can name the state the arm needs;
//! - the arm has no guard, and each parameter's sub-pattern is an integer,
//!   char, bool or (for `&str`) string literal, written as is, or a
//!   two-segment path `Type::Variant` naming a unit variant of the one enum
//!   the file declares under the parameter's declared type name (an
//!   or-pattern uses its first alternative);
//! - no earlier arm could take the value first: each earlier arm must differ
//!   from this one on some position by a provably different value (integers
//!   by parsed value, unescaped char and string literals by text, distinct
//!   unit variants of that same enum). Anything else, such as a constant, a
//!   range or a binding, counts as possibly equal;
//! - the owner's parameter is not declared `mut`, is not rebound or assigned
//!   before the match, and is not named inside a macro call before it.
//!
//! Anything else keeps the labelled `todo!()`. The value is an input that
//! reaches the arm once the receiver is in the state the arm names, not an
//! expected value.

use super::super::seams::RepoSeam;
use super::super::syntax::fn_signature::OwnerSignature;
use super::super::syntax::parse_clean_source_file;
use ra_ap_syntax::ast::{self, HasAttrs, HasName};
use ra_ap_syntax::{AstNode, SourceFile, SyntaxKind, SyntaxNode, TextSize};

/// What the changed arm's own pattern decides.
#[derive(Debug, Default)]
pub(super) struct MatchArmInputs {
    /// `(binding, value)` for each parameter the pattern decides.
    pub(super) inputs: Vec<(String, String)>,
    /// "`self.field` matches `Pattern`" for each receiver field the arm
    /// constrains, spelled for a comment; empty when the arm reads no state.
    pub(super) state: Vec<String>,
}

/// One element of the scrutinee.
enum Slot {
    /// A parameter, by name, declared type (whitespace removed) and declared
    /// type as written.
    Param(String, String, String),
    /// A receiver field (`self.state`), by its text: state, not an input.
    State(String),
}

/// A value a pattern alternative provably names.
#[derive(PartialEq)]
enum Value {
    Int(i128),
    /// A bool, or a char or string literal without escapes or raw prefix.
    Text(String),
    /// A unit variant of a file-local enum: `(enum, variant)`.
    Variant(String, String),
}

/// Paths (`Type::Variant`) are only offered when `allow_paths` (the stub sits
/// in a child module that sees the owner's items through `use super::*`).
pub(super) fn match_arm_inputs(
    seam: &RepoSeam,
    signature: &OwnerSignature,
    source: &str,
    allow_paths: bool,
) -> MatchArmInputs {
    derive(seam, signature, source, allow_paths).unwrap_or_default()
}

fn derive(
    seam: &RepoSeam,
    signature: &OwnerSignature,
    source: &str,
    allow_paths: bool,
) -> Option<MatchArmInputs> {
    let parse = parse_clean_source_file(source)?;
    let file = parse.tree();
    let offset = TextSize::try_from(seam.byte_offset()).ok()?;
    let arm = file
        .syntax()
        .descendants()
        .filter_map(ast::MatchArm::cast)
        .filter(|arm| arm.syntax().text_range().contains(offset))
        .max_by_key(|arm| arm.syntax().text_range().start())?;
    // A match-arm seam sits at the arm's `=>`; an offset inside the arm's
    // body (a nested `match` header, say) is not this arm.
    if arm.guard().is_some() || offset > arm.fat_arrow_token()?.text_range().end() {
        return None;
    }
    let arm_list = arm.syntax().parent().and_then(ast::MatchArmList::cast)?;
    let match_expr = arm_list.syntax().parent().and_then(ast::MatchExpr::cast)?;
    let owner = match_expr.syntax().ancestors().find_map(ast::Fn::cast)?;
    // The names must resolve against this owner's own signature, and a match
    // inside a closure sees other bindings.
    if owner.name()?.text() != signature.name.as_str()
        || match_expr
            .syntax()
            .ancestors()
            .take_while(|node| node != owner.syntax())
            .any(|node| ast::ClosureExpr::can_cast(node.kind()))
    {
        return None;
    }
    // An attribute (`#[cfg(not(test))]`) on this or an earlier arm may
    // remove it from the test build.
    if arm_list
        .arms()
        .take_while(|other| {
            other.syntax().text_range().start() <= arm.syntax().text_range().start()
        })
        .any(|other| other.attrs().next().is_some())
    {
        return None;
    }
    let (slots, is_tuple) = scrutinee_slots(&match_expr.expr()?, signature)?;
    let pattern_slots = |pat: ast::Pat| -> Option<Vec<ast::Pat>> {
        if !is_tuple {
            return Some(vec![pat]);
        }
        let ast::Pat::TuplePat(tuple) = pat else {
            return None;
        };
        let fields: Vec<ast::Pat> = tuple.fields().collect();
        (fields.len() == slots.len() && !fields.iter().any(|f| matches!(f, ast::Pat::RestPat(_))))
            .then_some(fields)
    };
    let ours = pattern_slots(arm.pat()?)?;
    let mut found = MatchArmInputs::default();
    // Per position: the one value this stub makes that position hold.
    let mut chosen: Vec<Option<Value>> = Vec::new();
    for (slot, pat) in slots.iter().zip(&ours) {
        match slot {
            Slot::Param(name, ty, raw) => {
                let first = first_alternative(pat);
                match value_for(&first, ty, raw, allow_paths, &file) {
                    Some((text, value)) => {
                        if unstable_before(&owner, name, &match_expr) {
                            return None;
                        }
                        found.inputs.push((name.clone(), text));
                        chosen.push(Some(value));
                    }
                    None => chosen.push(None),
                }
            }
            Slot::State(field) => {
                if catches_all(pat, &file) {
                    chosen.push(None);
                    continue;
                }
                let text = normalized(&pat.syntax().text().to_string());
                let plain = !text.is_empty()
                    && text
                        .chars()
                        .all(|c| c.is_alphanumeric() || "_:|-".contains(c));
                let field_plain = field
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '.');
                found.state.push(if plain && field_plain {
                    format!("`{field}` matches `{text}`")
                } else if field_plain {
                    format!("`{field}` matches this arm's pattern")
                } else {
                    "the receiver state matches this arm's pattern".to_string()
                });
                // The receiver is put in this arm's state, so an earlier arm
                // is excluded only by a value this arm alone names there.
                chosen.push(match pat {
                    ast::Pat::OrPat(_) => None,
                    other => exact_value(other, None, &file),
                });
            }
        }
    }
    if found.inputs.is_empty() {
        return None;
    }
    for earlier in arm_list.arms() {
        if earlier.syntax() == arm.syntax() {
            break;
        }
        let theirs = pattern_slots(earlier.pat()?)?;
        let excluded = slots
            .iter()
            .zip(&theirs)
            .zip(&chosen)
            .any(|((slot, pat), ours)| {
                let Some(ours) = ours else {
                    return false;
                };
                let ty = match slot {
                    Slot::Param(_, ty, _) => Some(ty.as_str()),
                    Slot::State(_) => None,
                };
                alternatives(pat).is_some_and(|alternatives| {
                    alternatives.iter().all(|alternative| {
                        exact_value(alternative, ty, &file)
                            .is_some_and(|theirs| distinct(&theirs, ours))
                    })
                })
            });
        if !excluded {
            return None;
        }
    }
    Some(found)
}

/// The scrutinee's slots, and whether it is a tuple (`(n,)` included).
fn scrutinee_slots(expr: &ast::Expr, signature: &OwnerSignature) -> Option<(Vec<Slot>, bool)> {
    let slot = |expr: &ast::Expr| -> Option<Slot> {
        match expr {
            ast::Expr::PathExpr(path) => {
                let path = path.path()?;
                if path.qualifier().is_some() {
                    return None;
                }
                let name = path.segment()?.name_ref()?.text().to_string();
                let param = signature.params.iter().find(|param| {
                    param
                        .name
                        .as_deref()
                        .is_some_and(|declared| unraw(declared) == unraw(&name))
                })?;
                Some(Slot::Param(name, normalized(&param.ty), param.ty.clone()))
            }
            ast::Expr::FieldExpr(field) => {
                let ast::Expr::PathExpr(base) = field.expr()? else {
                    return None;
                };
                let base = base.path()?;
                (base.qualifier().is_none()
                    && base.segment()?.self_token().is_some()
                    && signature.receiver.is_some())
                .then(|| Slot::State(normalized(&field.syntax().text().to_string())))
            }
            _ => None,
        }
    };
    let is_tuple = matches!(expr, ast::Expr::TupleExpr(_));
    let slots = match expr {
        ast::Expr::TupleExpr(tuple) => tuple
            .fields()
            .map(|field| slot(&field))
            .collect::<Option<Vec<_>>>()?,
        other => {
            let only = slot(other)?;
            if !matches!(only, Slot::Param(..)) {
                return None;
            }
            vec![only]
        }
    };
    // A parameter named twice (`(n, n)`) is not decided by one position.
    let mut names: Vec<&str> = slots
        .iter()
        .filter_map(|slot| match slot {
            Slot::Param(name, ..) => Some(unraw(name)),
            Slot::State(_) => None,
        })
        .collect();
    let count = names.len();
    names.sort_unstable();
    names.dedup();
    (names.len() == count).then_some((slots, is_tuple))
}

fn first_alternative(pat: &ast::Pat) -> ast::Pat {
    match pat {
        ast::Pat::OrPat(or) => or.pats().next().unwrap_or_else(|| pat.clone()),
        other => other.clone(),
    }
}

/// The alternatives of a literal, path, or or-pattern of those; `None` for
/// anything that may match more.
fn alternatives(pat: &ast::Pat) -> Option<Vec<ast::Pat>> {
    match pat {
        ast::Pat::OrPat(or) => {
            let mut all = Vec::new();
            for alternative in or.pats() {
                all.extend(alternatives(&alternative)?);
            }
            Some(all)
        }
        ast::Pat::LiteralPat(_) | ast::Pat::PathPat(_) => Some(vec![pat.clone()]),
        _ => None,
    }
}

/// The text to write and the value it names, for a parameter of type `ty`.
fn value_for(
    pat: &ast::Pat,
    ty: &str,
    raw: &str,
    allow_paths: bool,
    file: &SourceFile,
) -> Option<(String, Value)> {
    match pat {
        ast::Pat::LiteralPat(literal) => {
            let fits = match literal.literal()?.kind() {
                ast::LiteralKind::IntNumber(_) => is_integer_type(ty),
                ast::LiteralKind::Char(_) => ty == "char",
                ast::LiteralKind::Bool(_) => ty == "bool",
                ast::LiteralKind::String(_) => is_str_ref(raw),
                _ => false,
            };
            // The literal is written exactly as the arm spells it.
            let text = pat.syntax().text().to_string();
            if !fits || text.chars().any(char::is_control) {
                return None;
            }
            let value = exact_value(pat, Some(ty), file)?;
            Some((text, value))
        }
        ast::Pat::PathPat(_) if allow_paths && !ty.starts_with('&') && ty != "Self" => {
            let value = exact_value(pat, Some(ty), file)?;
            let Value::Variant(enum_name, variant) = &value else {
                return None;
            };
            Some((format!("{enum_name}::{variant}"), value))
        }
        _ => None,
    }
}

/// The value one pattern alternative provably names: an integer literal by
/// its parsed value, a bool, a char or string literal without escapes, or a
/// unit variant of the one enum the file declares under `ty` (or, when `ty`
/// is unknown, under the path's own qualifier).
fn exact_value(pat: &ast::Pat, ty: Option<&str>, file: &SourceFile) -> Option<Value> {
    match pat {
        ast::Pat::LiteralPat(literal) => {
            let text = pat.syntax().text().to_string();
            match literal.literal()?.kind() {
                ast::LiteralKind::IntNumber(_) => parse_integer(&text).map(Value::Int),
                ast::LiteralKind::Bool(_) => Some(Value::Text(text)),
                ast::LiteralKind::Char(_) | ast::LiteralKind::String(_) => {
                    let plain =
                        (text.starts_with('\'') || text.starts_with('"')) && !text.contains('\\');
                    plain.then_some(Value::Text(text))
                }
                _ => None,
            }
        }
        ast::Pat::PathPat(path) => {
            let path = path.path()?;
            let qualifier = path.qualifier()?;
            let has_generics = |segment: ast::PathSegment| {
                segment
                    .syntax()
                    .children()
                    .any(|child| ast::GenericArgList::can_cast(child.kind()))
            };
            if qualifier.qualifier().is_some()
                || has_generics(path.segment()?)
                || has_generics(qualifier.segment()?)
            {
                return None;
            }
            let enum_name = qualifier.segment()?.name_ref()?.text().to_string();
            let variant = path.segment()?.name_ref()?.text().to_string();
            if ty.is_some_and(|ty| ty != enum_name) || enum_name == "Self" {
                return None;
            }
            unit_variants(file, &enum_name)?
                .contains(&variant)
                .then_some(Value::Variant(enum_name, variant))
        }
        _ => None,
    }
}

fn distinct(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => a != b,
        (Value::Text(a), Value::Text(b)) => a != b,
        (Value::Variant(ea, va), Value::Variant(eb, vb)) => ea == eb && va != vb,
        _ => false,
    }
}

/// The unit variants of the one enum the file declares as `name`, when
/// nothing else in the file could give that name another meaning: another
/// item of the name, a rename to it, an import of it other than through
/// `super::` or `self::`, a glob import other than `super::*` / `self::*`,
/// or an item-position macro call or macro definition that mentions it.
pub(super) fn unit_variants(file: &SourceFile, name: &str) -> Option<Vec<String>> {
    let mut found = None;
    for node in file.syntax().descendants() {
        let kind = node.kind();
        match kind {
            SyntaxKind::ENUM
            | SyntaxKind::STRUCT
            | SyntaxKind::UNION
            | SyntaxKind::TYPE_ALIAS
            | SyntaxKind::TRAIT
            | SyntaxKind::CONST
            | SyntaxKind::STATIC
            | SyntaxKind::MODULE => {
                let named = node
                    .children()
                    .find_map(ast::Name::cast)
                    .is_some_and(|n| n.text() == name);
                if !named {
                    continue;
                }
                let item = ast::Enum::cast(node)?;
                if found.is_some() {
                    return None;
                }
                let variants = item
                    .variant_list()?
                    .variants()
                    .filter(|variant| variant.field_list().is_none())
                    .filter_map(|variant| variant.name().map(|n| n.text().to_string()))
                    .collect::<Vec<_>>();
                found = Some(variants);
            }
            SyntaxKind::RENAME | SyntaxKind::MACRO_RULES | SyntaxKind::MACRO_DEF
                if mentions(&node, name) =>
            {
                return None;
            }
            SyntaxKind::MACRO_CALL => {
                let item_position = node.parent().is_some_and(|parent| {
                    matches!(
                        parent.kind(),
                        SyntaxKind::ITEM_LIST | SyntaxKind::SOURCE_FILE
                    )
                });
                if item_position && mentions(&node, name) {
                    return None;
                }
            }
            SyntaxKind::USE_TREE => {
                let Some(tree) = ast::UseTree::cast(node) else {
                    continue;
                };
                // Only a leaf tree (an import or a glob) brings a name in.
                if tree.use_tree_list().is_some() {
                    continue;
                }
                let full = full_use_path(&tree);
                // `use super::Name;`, `use self::Name;` or their globs.
                let local = full.len() == 2
                    && full
                        .first()
                        .is_some_and(|first| first == "super" || first == "self");
                if tree.star_token().is_some() {
                    if !local {
                        return None;
                    }
                } else if full.last().is_some_and(|last| last == name) && !local {
                    return None;
                }
            }
            _ => {}
        }
    }
    found
}

/// The segments of a leaf use tree's full path, outermost first, with `*`
/// for a glob.
pub(super) fn full_use_path(tree: &ast::UseTree) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = Some(tree.clone());
    while let Some(tree) = current {
        let mut own = Vec::new();
        if tree.star_token().is_some() {
            own.push("*".to_string());
        }
        if let Some(path) = tree.path() {
            let mut parts: Vec<String> = path
                .segments()
                .map(|segment| normalized(&segment.syntax().text().to_string()))
                .collect();
            parts.extend(own);
            own = parts;
        }
        own.extend(segments);
        segments = own;
        current = tree
            .syntax()
            .parent()
            .and_then(|list| list.parent())
            .and_then(ast::UseTree::cast);
    }
    segments
}

/// `_`, or a plain lowercase binding. A bare identifier can also name a
/// constant or an imported unit variant (`RUNNING`), so an uppercase one, or
/// one the file declares as a `const` or `static`, does not count.
pub(super) fn catches_all(pat: &ast::Pat, file: &SourceFile) -> bool {
    match pat {
        ast::Pat::WildcardPat(_) => true,
        ast::Pat::IdentPat(ident) => {
            let Some(name) = ident.name() else {
                return false;
            };
            let text = name.text().to_string();
            let name = unraw(&text);
            ident.pat().is_none()
                && name.starts_with(|c: char| c.is_lowercase() || c == '_')
                && !file.syntax().descendants().any(|node| {
                    matches!(node.kind(), SyntaxKind::CONST | SyntaxKind::STATIC)
                        && node
                            .children()
                            .find_map(ast::Name::cast)
                            .is_some_and(|n| unraw(n.text()) == name)
                })
        }
        _ => false,
    }
}

/// Whether the parameter `name` may hold another value when the match
/// starts: it is declared `mut`, or something before the match rebinds it
/// (a `let`, closure or pattern binding), assigns it, or names it inside a
/// macro call.
fn unstable_before(owner: &ast::Fn, name: &str, match_expr: &ast::MatchExpr) -> bool {
    let Some(body) = owner.body() else {
        return true;
    };
    let declared_mut = owner
        .param_list()
        .into_iter()
        .flat_map(|list| list.params())
        .filter_map(|param| match param.pat()? {
            ast::Pat::IdentPat(ident) => Some(ident),
            _ => None,
        })
        .any(|ident| {
            ident.name().is_some_and(|n| unraw(n.text()) == unraw(name))
                && ident.mut_token().is_some()
        });
    // A value item of the name anywhere in the body shadows the parameter
    // for the whole block.
    let local_item = body.syntax().descendants().any(|node| {
        matches!(
            node.kind(),
            SyntaxKind::CONST | SyntaxKind::STATIC | SyntaxKind::FN
        ) && node
            .children()
            .find_map(ast::Name::cast)
            .is_some_and(|n| unraw(n.text()) == unraw(name))
    });
    if declared_mut || local_item {
        return true;
    }
    let start = match_expr.syntax().text_range().start();
    body.syntax()
        .descendants()
        .filter(|node| node.text_range().start() < start)
        .any(|node| binds_or_assigns(&node, name))
}

fn binds_or_assigns(node: &SyntaxNode, name: &str) -> bool {
    if let Some(ident) = ast::IdentPat::cast(node.clone()) {
        return ident.name().is_some_and(|n| unraw(n.text()) == unraw(name));
    }
    if let Some(bin) = ast::BinExpr::cast(node.clone()) {
        let assigns = matches!(bin.op_kind(), Some(ast::BinaryOp::Assignment { .. }));
        return assigns && bin.lhs().is_some_and(|lhs| mentions(lhs.syntax(), name));
    }
    node.kind() == SyntaxKind::MACRO_CALL && mentions(node, name)
}

/// Whether an identifier token spelled `name` appears under `node`.
pub(super) fn mentions(node: &SyntaxNode, name: &str) -> bool {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .any(|token| token.kind() == SyntaxKind::IDENT && unraw(token.text()) == unraw(name))
}

/// An integer literal's value: optional `-`, `0x`/`0o`/`0b` radix,
/// underscores and a type suffix.
pub(super) fn parse_integer(text: &str) -> Option<i128> {
    let text: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .collect();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.as_str()),
    };
    let (radix, digits) = if let Some(rest) = digits.strip_prefix("0x") {
        (16, rest)
    } else if let Some(rest) = digits.strip_prefix("0o") {
        (8, rest)
    } else if let Some(rest) = digits.strip_prefix("0b") {
        (2, rest)
    } else {
        (10, digits)
    };
    // A suffix starts with `u` or `i`, which are not hex digits.
    let suffixes = [
        "u128", "i128", "usize", "isize", "u16", "u32", "u64", "i16", "i32", "i64", "u8", "i8",
    ];
    let digits = suffixes
        .iter()
        .find_map(|suffix| digits.strip_suffix(suffix))
        .unwrap_or(digits);
    let value = i128::from_str_radix(digits, radix).ok()?;
    Some(if negative { -value } else { value })
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

/// `&str` or `&'a str`, as written.
fn is_str_ref(raw: &str) -> bool {
    let Some(rest) = raw.trim().strip_prefix('&') else {
        return false;
    };
    let rest = rest.trim_start();
    let rest = match rest.strip_prefix('\'') {
        Some(lifetime) => lifetime.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_'),
        None => rest,
    };
    rest.trim() == "str"
}

/// An identifier without its `r#` raw prefix, for comparing names.
pub(super) fn unraw(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}

fn normalized(text: &str) -> String {
    text.split_whitespace().collect()
}
