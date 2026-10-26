//! Parser-confirmed declarations stay unknown, not executable fields.

use super::super::lexical::classify_changed_line;
use super::{ParserProbeShape, source_line_byte_range};
use crate::analysis::facts::FileData;
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::ProbeFamily;
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, ast};
use std::ops::Range;

pub(super) fn type_bound_lines(
    source: &str,
    lines: &[crate::analysis::diff::ChangedLine],
) -> Vec<bool> {
    let Some(parse) = parse_clean_source_file(source) else {
        return vec![false; lines.len()];
    };
    let root = parse.tree();
    lines
        .iter()
        .map(|line| {
            let Some(range) = source_line_byte_range(source, line.line) else {
                return false;
            };
            if source.get(range.clone()).map(str::trim) != Some(line.text.trim()) {
                return false;
            }
            let Some(span) = declaration_line_span(root.syntax(), range) else {
                return false;
            };
            root.syntax()
                .descendants()
                .filter(supported_bound_declaration)
                .any(|node| {
                    let range = node.text_range();
                    u32::from(range.start()) as usize <= span.start
                        && span.end <= u32::from(range.end()) as usize
                })
        })
        .collect()
}

/// Whether each line touches a type-bound construct, even when the line also
/// holds other code (`fn f<T: Clone>(`, `where T: Clone {`, `x: impl Clone,`).
/// Overlap, not containment: grouping a wrapped signature's lines must never
/// drop a bound that shares a line with parameters or the body opener. A
/// failed parse or unreadable line fails closed to `true` (#8999).
pub(super) fn header_bound_lines(
    source: &str,
    lines: &[crate::analysis::diff::ChangedLine],
) -> Vec<bool> {
    let Some(parse) = parse_clean_source_file(source) else {
        return vec![true; lines.len()];
    };
    let root = parse.tree();
    let bounds = root
        .syntax()
        .descendants()
        .filter(|node| {
            ast::WhereClause::can_cast(node.kind())
                || ast::WherePred::can_cast(node.kind())
                || ast::TypeBoundList::can_cast(node.kind())
        })
        .map(|node| {
            let range = node.text_range();
            u32::from(range.start()) as usize..u32::from(range.end()) as usize
        })
        .collect::<Vec<_>>();
    // Line start offsets once, so the pass stays linear in the file.
    let starts = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(at, _)| at + 1))
        .collect::<Vec<_>>();
    lines
        .iter()
        .map(|line| {
            let Some(&start) = line
                .new_side_line
                .checked_sub(1)
                .and_then(|at| starts.get(at))
            else {
                return true;
            };
            let end = starts
                .get(line.new_side_line)
                .copied()
                .unwrap_or(source.len());
            let range = start..end;
            bounds
                .iter()
                .any(|bound| bound.start < range.end && range.start < bound.end)
        })
        .collect()
}

/// Removed struct and enum-variant field declarations (#9183), judged on the
/// reconstructed old source with the added side's owner and exact-line rule:
/// the line must equal the old source line, every nontrivia token of it must
/// sit inside one supported field declaration, and the lexer must currently
/// read it as `field_construction`. A failed parse, a mismatched line, a
/// discriminant, macro syntax, a body constructor or a shared line stays
/// `false` and keeps its existing probe.
pub(super) fn field_declaration_lines(
    source: &str,
    lines: &[crate::analysis::diff::ChangedLine],
) -> Vec<bool> {
    let Some(parse) = parse_clean_source_file(source) else {
        return vec![false; lines.len()];
    };
    let root = parse.tree();
    lines
        .iter()
        .map(|line| {
            if !classify_changed_line(line.text.trim()).contains(&ProbeFamily::FieldConstruction) {
                return false;
            }
            let Some(range) = source_line_byte_range(source, line.line) else {
                return false;
            };
            if source.get(range.clone()).map(str::trim) != Some(line.text.trim()) {
                return false;
            }
            let Some(span) = declaration_line_span(root.syntax(), range) else {
                return false;
            };
            root.syntax()
                .descendants()
                .filter(supported_field_declaration)
                .any(|node| {
                    let range = node.text_range();
                    u32::from(range.start()) as usize <= span.start
                        && span.end <= u32::from(range.end()) as usize
                })
                // #10268: the same conservative recognition for a field of a
                // struct declared inside an item macro in the old source.
                || macro_struct_field_line(root.syntax(), &span)
        })
        .collect()
}

/// This fallback is used only after ordinary parser-owned expressions have
/// been considered. It never promotes exposure or drops the changed source.
/// Exact current-line equality prevents borrowing new-side context for a
/// different removed/stale line. Shared declaration/expression lines are not
/// eligible. Record fields containing const expressions or macros remain with
/// the existing analysis rather than acquiring a declaration-only disposition.
pub(super) fn declaration_shape<'a>(
    facts: &'a FileData,
    line: usize,
    changed_text: &str,
) -> Option<ParserProbeShape<'a>> {
    if facts.used_lexical_fallback
        || !classify_changed_line(changed_text).contains(&ProbeFamily::FieldConstruction)
    {
        return None;
    }
    let line_range = source_line_byte_range(&facts.source, line)?;
    let source_line = facts.source.get(line_range.clone())?;
    let text = source_line.trim();
    if text.is_empty() || text != changed_text.trim() {
        return None;
    }

    // Reuse the existing parser, not a colon or comment-delimiter heuristic.
    // Real expression shapes take precedence and do not enter this fallback.
    let parse = parse_clean_source_file(&facts.source)?;
    let root = parse.tree();
    let declaration = declaration_line_span(root.syntax(), line_range)?;
    for node in root.syntax().descendants().filter(supported_declaration) {
        let range = node.text_range();
        let start = u32::from(range.start()) as usize;
        let end = u32::from(range.end()) as usize;
        if start <= declaration.start && declaration.end <= end {
            return Some(ParserProbeShape {
                family: ProbeFamily::StaticUnknown,
                start_line: line,
                start_byte: declaration.start,
                text,
                standalone_call: false,
                unsafe_boundary: false,
            });
        }
    }
    if macro_struct_field_line(root.syntax(), &declaration) {
        return Some(ParserProbeShape {
            family: ProbeFamily::StaticUnknown,
            start_line: line,
            start_byte: declaration.start,
            text,
            standalone_call: false,
            unsafe_boundary: false,
        });
    }
    None
}

/// A field line of a `struct` declared inside an item macro
/// (`pin_project! { pub struct X { buffered: Option<u8>, } }`). The macro body
/// is an opaque token tree, so no `RecordField` node exists; the line is a
/// declaration when every token sits directly in a `{ .. }` tree whose
/// preceding tokens in the same tree are a `struct` header (#10268). A
/// construction (`Self { a: 1 }`) has no `struct` keyword before its brace.
///
/// The line must read as pure type syntax, because a macro DSL could execute
/// an expression written where a type is expected. It is withheld for a
/// nested brace, a macro `!`, a paren group that is not a type (`compute()`:
/// only a `fn`/`Fn*`/`FnMut`/`FnOnce` argument list or a tuple type after
/// `: < , & * ( [ + ->` qualifies), and an array length that is anything but
/// a literal or a path (`[u8; N + 1]`). The header search runs to the
/// enclosing tree boundary and fails closed on a top-level `,`, so
/// `struct S(u8), T { a: 1 }` does not make `T { .. }` a declaration.
fn macro_struct_field_line(root: &SyntaxNode, span: &Range<usize>) -> bool {
    let Some(first) = root
        .token_at_offset((span.start as u32).into())
        .right_biased()
    else {
        return false;
    };
    let Some(body) = first.parent().filter(is_brace_token_tree) else {
        return false;
    };
    let in_span = |range: ra_ap_syntax::TextRange| {
        (u32::from(range.start()) as usize) >= span.start
            && (u32::from(range.end()) as usize) <= span.end
    };
    let tokens = body
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| in_span(token.text_range()))
        .filter(|token| !matches!(token.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT))
        .collect::<Vec<_>>();
    // One field per line: a top-level `,` before the last token means the line
    // holds a second field (`b: u8, c: u8,`) or something ambiguous, and a
    // top-level `=` is an initializer or discriminant, never a field type.
    let kind_at = |index: Option<usize>| {
        index
            .and_then(|at| tokens.get(at))
            .map(ra_ap_syntax::SyntaxToken::kind)
    };
    let mut angle_depth = 0usize;
    let mut top_level_colon = false;
    for (index, token) in tokens.iter().enumerate() {
        let top_level = angle_depth == 0 && token.parent().as_ref() == Some(&body);
        let previous = kind_at(index.checked_sub(1));
        let next = kind_at(Some(index + 1));
        // An allowlist: only tokens that can spell a field type are accepted,
        // so a literal, closure, operator, cast, `?`, range or member access
        // keeps its lexical probe (a bare path like `None` or `Foo` is
        // indistinguishable from a type and is the declared limit).
        let pure_type = match token.kind() {
            SyntaxKind::L_PAREN => type_paren_group(token),
            SyntaxKind::L_BRACK => literal_or_path_array_length(token),
            // `<` opens generic arguments only after a path (`Foo<..>`).
            SyntaxKind::L_ANGLE => {
                angle_depth += 1;
                matches!(
                    previous,
                    Some(
                        SyntaxKind::IDENT
                            | SyntaxKind::COLON2
                            | SyntaxKind::SELF_TYPE_KW
                            | SyntaxKind::CRATE_KW
                            | SyntaxKind::FOR_KW
                    )
                )
            }
            // `&` is a reference prefix, never a binary `a & b`/`a && b`.
            SyntaxKind::AMP => !matches!(
                previous,
                Some(
                    SyntaxKind::IDENT
                        | SyntaxKind::SELF_TYPE_KW
                        | SyntaxKind::SELF_KW
                        | SyntaxKind::CRATE_KW
                        | SyntaxKind::SUPER_KW
                        | SyntaxKind::R_PAREN
                        | SyntaxKind::R_BRACK
                        | SyntaxKind::INT_NUMBER
                )
            ),
            // `*` is a raw-pointer prefix: `*const T` / `*mut T`, never a product.
            SyntaxKind::STAR => matches!(next, Some(SyntaxKind::CONST_KW | SyntaxKind::MUT_KW)),
            // Inside a token tree `->` is `-` then `>`: the pair is an arrow.
            SyntaxKind::MINUS => next == Some(SyntaxKind::R_ANGLE),
            SyntaxKind::R_ANGLE if previous == Some(SyntaxKind::MINUS) => true,
            SyntaxKind::R_ANGLE => match angle_depth.checked_sub(1) {
                Some(depth) => {
                    angle_depth = depth;
                    true
                }
                None => false,
            },
            SyntaxKind::COLON => {
                top_level_colon |= top_level;
                true
            }
            SyntaxKind::COMMA => !top_level || index + 1 == tokens.len(),
            SyntaxKind::EQ => angle_depth > 0,
            SyntaxKind::SEMICOLON => !top_level,
            // A length literal belongs to `[T; 4]` only.
            SyntaxKind::INT_NUMBER => {
                previous == Some(SyntaxKind::SEMICOLON) && next == Some(SyntaxKind::R_BRACK)
            }
            // `+` joins bounds (`dyn Fn() + Send`), never arithmetic on a literal.
            SyntaxKind::PLUS => {
                matches!(
                    previous,
                    Some(
                        SyntaxKind::IDENT
                            | SyntaxKind::LIFETIME_IDENT
                            | SyntaxKind::R_ANGLE
                            | SyntaxKind::R_PAREN
                    )
                ) && matches!(
                    next,
                    Some(SyntaxKind::IDENT | SyntaxKind::LIFETIME_IDENT | SyntaxKind::DYN_KW)
                )
            }
            SyntaxKind::IDENT
            | SyntaxKind::LIFETIME_IDENT
            | SyntaxKind::SELF_TYPE_KW
            | SyntaxKind::SELF_KW
            | SyntaxKind::CRATE_KW
            | SyntaxKind::SUPER_KW
            | SyntaxKind::COLON2
            | SyntaxKind::R_PAREN
            | SyntaxKind::R_BRACK
            | SyntaxKind::THIN_ARROW
            | SyntaxKind::DYN_KW
            | SyntaxKind::MUT_KW
            | SyntaxKind::CONST_KW
            | SyntaxKind::IMPL_KW
            | SyntaxKind::FN_KW
            | SyntaxKind::UNSAFE_KW
            | SyntaxKind::EXTERN_KW
            | SyntaxKind::FOR_KW
            | SyntaxKind::PUB_KW
            | SyntaxKind::IN_KW
            | SyntaxKind::POUND
            | SyntaxKind::UNDERSCORE => true,
            _ => false,
        };
        if !pure_type {
            return false;
        }
    }
    if angle_depth != 0 || !top_level_colon {
        return false;
    }
    // The struct must sit directly in the token tree of one macro call: a
    // macro nested inside another macro's input is opaque twice over.
    let direct_in_macro_call = body
        .parent()
        .filter(|tree| tree.kind() == SyntaxKind::TOKEN_TREE)
        .and_then(|tree| tree.parent())
        .is_some_and(|owner| owner.kind() == SyntaxKind::MACRO_CALL);
    if !direct_in_macro_call {
        return false;
    }
    // Whatever follows the struct body must be another item or the end of the
    // tree; stray tokens make the whole input ambiguous.
    let mut next = body.next_sibling_or_token();
    while let Some(element) = next {
        match element.kind() {
            SyntaxKind::WHITESPACE | SyntaxKind::COMMENT | SyntaxKind::R_CURLY => {}
            SyntaxKind::IMPL_KW
            | SyntaxKind::STRUCT_KW
            | SyntaxKind::ENUM_KW
            | SyntaxKind::FN_KW
            | SyntaxKind::TRAIT_KW
            | SyntaxKind::UNSAFE_KW
            | SyntaxKind::USE_KW
            | SyntaxKind::MOD_KW
            | SyntaxKind::CONST_KW
            | SyntaxKind::STATIC_KW
            | SyntaxKind::TYPE_KW
            | SyntaxKind::PUB_KW
            | SyntaxKind::POUND => break,
            _ => return false,
        }
        next = element.next_sibling_or_token();
    }
    let mut angle_depth = 0usize;
    let mut previous = body.prev_sibling_or_token();
    while let Some(element) = previous {
        match element.kind() {
            SyntaxKind::STRUCT_KW => return true,
            SyntaxKind::R_ANGLE => angle_depth += 1,
            SyntaxKind::L_ANGLE => match angle_depth.checked_sub(1) {
                Some(depth) => angle_depth = depth,
                None => return false,
            },
            SyntaxKind::COMMA if angle_depth == 0 => return false,
            SyntaxKind::SEMICOLON
            | SyntaxKind::EQ
            | SyntaxKind::FN_KW
            | SyntaxKind::IMPL_KW
            | SyntaxKind::ENUM_KW
            | SyntaxKind::TRAIT_KW
            | SyntaxKind::FAT_ARROW => return false,
            SyntaxKind::TOKEN_TREE if element.as_node().is_some_and(is_brace_token_tree) => {
                return false;
            }
            _ => {}
        }
        previous = element.prev_sibling_or_token();
    }
    false
}

fn previous_significant_token(
    token: &ra_ap_syntax::SyntaxToken,
) -> Option<ra_ap_syntax::SyntaxToken> {
    let mut previous = token.prev_token();
    while let Some(candidate) = previous {
        if !matches!(
            candidate.kind(),
            SyntaxKind::WHITESPACE | SyntaxKind::COMMENT
        ) {
            return Some(candidate);
        }
        previous = candidate.prev_token();
    }
    None
}

/// A `(` that opens type syntax: a `fn`/`Fn*` parameter list, or a tuple type
/// where a type may start. A `(` after any other identifier is a call.
fn type_paren_group(open: &ra_ap_syntax::SyntaxToken) -> bool {
    let Some(previous) = previous_significant_token(open) else {
        return false;
    };
    match previous.kind() {
        SyntaxKind::FN_KW
        | SyntaxKind::PUB_KW
        | SyntaxKind::COLON
        | SyntaxKind::L_ANGLE
        | SyntaxKind::COMMA
        | SyntaxKind::AMP
        | SyntaxKind::STAR
        | SyntaxKind::L_PAREN
        | SyntaxKind::L_BRACK
        | SyntaxKind::PLUS
        | SyntaxKind::THIN_ARROW => true,
        SyntaxKind::IDENT => matches!(previous.text(), "Fn" | "FnMut" | "FnOnce"),
        _ => false,
    }
}

/// A `[` that is a slice/array type: any length after its `;` must be a
/// literal or a plain path, never an expression (`N + 1`, `cap()`).
fn literal_or_path_array_length(open: &ra_ap_syntax::SyntaxToken) -> bool {
    let Some(tree) = open.parent() else {
        return false;
    };
    let mut after_semicolon = false;
    // A slice/array type holds exactly one element type: a comma outside any
    // `<..>` (`[A, B]`) makes it an element list, which keeps its probe.
    let mut angle_depth = 0usize;
    let mut previous = None;
    // `[]` and `[;3]` hold no element type, and `[T;]` no length: not types.
    let mut element_parts = 0usize;
    let mut length_parts = 0usize;
    for element in tree.children_with_tokens() {
        let Some(token) = element.as_token() else {
            if after_semicolon {
                return false;
            }
            element_parts += 1;
            continue;
        };
        let kind = token.kind();
        if !matches!(
            kind,
            SyntaxKind::L_BRACK
                | SyntaxKind::R_BRACK
                | SyntaxKind::WHITESPACE
                | SyntaxKind::COMMENT
                | SyntaxKind::SEMICOLON
        ) {
            if after_semicolon {
                length_parts += 1;
            } else {
                element_parts += 1;
            }
        }
        match kind {
            SyntaxKind::L_ANGLE => angle_depth += 1,
            // `->` inside a bracketed fn type is `-` then `>`, not a close.
            SyntaxKind::R_ANGLE if previous != Some(SyntaxKind::MINUS) => {
                angle_depth = angle_depth.saturating_sub(1);
            }
            SyntaxKind::COMMA if angle_depth == 0 => return false,
            _ => {}
        }
        if !matches!(kind, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT) {
            previous = Some(kind);
        }
        match token.kind() {
            SyntaxKind::SEMICOLON => after_semicolon = true,
            SyntaxKind::WHITESPACE | SyntaxKind::COMMENT | SyntaxKind::R_BRACK => {}
            SyntaxKind::IDENT
            | SyntaxKind::SELF_TYPE_KW
            | SyntaxKind::CRATE_KW
            | SyntaxKind::INT_NUMBER
            | SyntaxKind::COLON
            | SyntaxKind::COLON2
                if after_semicolon => {}
            _ if after_semicolon => return false,
            _ => {}
        }
    }
    element_parts > 0 && (!after_semicolon || length_parts > 0)
}

fn is_brace_token_tree(node: &SyntaxNode) -> bool {
    node.kind() == SyntaxKind::TOKEN_TREE
        && node
            .first_token()
            .is_some_and(|token| token.kind() == SyntaxKind::L_CURLY)
}

fn supported_declaration(node: &SyntaxNode) -> bool {
    // Where predicates (including lifetime and higher-ranked bounds) and
    // generic parameters are type syntax. Admission still requires every
    // nontrivia token of this exact changed line to fit its declaration owner;
    // shared runtime bodies and macro token trees cannot borrow its authority.
    if supported_bound_declaration(node) {
        return true;
    }
    if ast::Param::can_cast(node.kind()) || ast::SelfParam::can_cast(node.kind()) {
        return node
            .parent()
            .and_then(ast::ParamList::cast)
            .and_then(|list| list.syntax().parent())
            .is_some_and(|parent| ast::Fn::can_cast(parent.kind()));
    }
    supported_field_declaration(node)
}

/// A named struct or enum-variant field declaration, or a whole one-line
/// variant with named fields (`Bad { offset: usize },`). Type syntax, never a
/// construction (#8999). Union, tuple and macro contexts have separate
/// contracts; a real `RecordExprField` never matches. A discriminant, macro
/// syntax or any expression inside the node withholds this authority.
fn supported_field_declaration(node: &SyntaxNode) -> bool {
    let holds_executable = |node: &SyntaxNode| {
        node.descendants().any(|child| {
            ast::Expr::can_cast(child.kind()) || ast::MacroCall::can_cast(child.kind())
        })
    };
    if ast::Variant::can_cast(node.kind()) {
        return node
            .children()
            .any(|child| ast::RecordFieldList::can_cast(child.kind()))
            && !holds_executable(node);
    }
    if !ast::RecordField::can_cast(node.kind()) {
        return false;
    }
    let owned_by_declaration = node
        .parent()
        .and_then(ast::RecordFieldList::cast)
        .and_then(|list| list.syntax().parent())
        .is_some_and(|parent| {
            ast::Struct::can_cast(parent.kind()) || ast::Variant::can_cast(parent.kind())
        });
    owned_by_declaration && !holds_executable(node)
}

fn supported_type_bound(node: &SyntaxNode) -> bool {
    (ast::WhereClause::can_cast(node.kind())
        || ast::WherePred::can_cast(node.kind())
        || ast::TypeParam::can_cast(node.kind())
        || ast::LifetimeParam::can_cast(node.kind()))
        && !node.descendants().any(|child| {
            ast::Expr::can_cast(child.kind()) || ast::MacroCall::can_cast(child.kind())
        })
}

fn supported_bound_declaration(node: &SyntaxNode) -> bool {
    if supported_type_bound(node) {
        return true;
    }
    // An inline bound occupies only part of its item. The complete item may
    // own a changed line only when it contains no executable or macro syntax.
    // Function bodies and code sharing the item line cannot borrow this range.
    (ast::Struct::can_cast(node.kind())
        || ast::Impl::can_cast(node.kind())
        || ast::TypeAlias::can_cast(node.kind()))
        && node.descendants().any(|child| supported_type_bound(&child))
        && !node.descendants().any(|child| {
            ast::Expr::can_cast(child.kind()) || ast::MacroCall::can_cast(child.kind())
        })
}

/// Bound the line by parser-owned nontrivia tokens, excluding its trailing
/// separator. Comment text is preserved in the shape, but cannot extend its
/// containment range. Real code after a comment remains in range.
fn declaration_line_span(root: &SyntaxNode, line: Range<usize>) -> Option<Range<usize>> {
    let mut start = None;
    let mut end = None;
    for token in root
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        let range = token.text_range();
        let token_start = u32::from(range.start()) as usize;
        let token_end = u32::from(range.end()) as usize;
        if token_start >= line.end {
            break;
        }
        if token_end <= line.start
            || matches!(token.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
        {
            continue;
        }
        // A multiline nontrivia token cannot be a complete declaration-only line.
        if token_start < line.start || token_end > line.end {
            return None;
        }
        if start.is_none() {
            start = Some(token_start);
        }
        if token.kind() != SyntaxKind::COMMA {
            end = Some(token_end);
        }
    }
    let start = start?;
    let end = end?;
    Some(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
    use std::path::Path;

    const SOURCE: &str = "struct Path;\nfn project(\n    out: &Path,\n) {}\n";

    fn declaration_facts(source: &str) -> Result<FileData, String> {
        let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), source)?;
        Ok(FileData {
            path: facts.path,
            returns: facts.returns,
            literals: facts.literals,
            probe_shapes: facts.probe_shapes,
            used_lexical_fallback: facts.used_lexical_fallback,
            module_declarations: facts.module_declarations,
            unresolved_property_macros: facts.unresolved_property_macros,
            role_provenance: facts.role_provenance,
            source: facts.source,
            item_scopes: facts.item_scopes,
            macro_candidates: facts.macro_candidates,
        })
    }

    #[test]
    fn macro_wrapped_struct_field_is_a_declaration_but_a_construction_is_not() -> Result<(), String>
    {
        let source = "pin_project! {\n    pub struct Task<B> {\n        #[pin]\n        rx: Rx<B>,\n        buffered: Option<Cursor<Box<[u8]>>>,\n    }\n}\nfn make() {\n    wrap! {\n        Task {\n            buffered: None,\n        }\n    }\n}\n";
        let facts = declaration_facts(source)?;
        for (line, text) in [
            (4, "rx: Rx<B>,"),
            (5, "buffered: Option<Cursor<Box<[u8]>>>,"),
        ] {
            let shape = declaration_shape(&facts, line, text)
                .ok_or_else(|| format!("macro struct field not a declaration: {text}"))?;
            assert_eq!(shape.family, ProbeFamily::StaticUnknown);
            assert_eq!(shape.start_line, line);
        }
        assert!(
            declaration_shape(&facts, 11, "buffered: None,").is_none(),
            "a construction inside a macro keeps its probe"
        );
        let decl = |source: &str, line: usize, text: &str| -> Result<bool, String> {
            Ok(declaration_shape(&declaration_facts(source)?, line, text).is_some())
        };
        // Type syntax stays a declaration: fn pointers, Fn bounds, tuples, paths.
        for body in [
            "cb: fn(u8) -> u8,",
            "boxed: Option<Box<dyn Fn(u8) + Send>>,",
            "pair: (u8, Foo),",
            "buf: [u8; 4],",
            "buf: [u8; N],",
            "r: &'a str,",
            "b: Box<dyn Fn(u8) -> u8>,",
            "pub(crate) v: Vec<u8>,",
            "c: [A; N],",
            "m: HashMap<K, V>,",
            "p: *const u8,",
            "s: [T],",
            "s: [T; N],",
            "s: [T; 4],",
            "s: [Foo<A, B>],",
            "s: [fn(u8) -> u8],",
            "u: (),",
            "g: Foo<>,",
            "h: fn(),",
            "s: [(u8, u8)],",
            "h: Box<dyn for<'a> Fn(&'a u8)>,",
            "q: &&'a str,",
            "pub(super) x: u8,",
            "pub(self) z: u8,",
        ] {
            let source = format!("m! {{\n    struct S {{\n        {body}\n    }}\n}}\n");
            assert!(
                decl(&source, 3, body)?,
                "type syntax must be a declaration: {body}"
            );
        }
        // Anything that could execute stays a lexical field_construction.
        for body in [
            "buf: [u8; cap()],",
            "buf: [u8; N + 1],",
            "inner: Wrap!(u8),",
            "value: compute(),",
            "value: Foo::<u8>(),",
            "value: Foo { a: 1 },",
            "value: [compute(); 3],",
            "b: u8, c: u8,",
            "a: u8 = 3,",
            "a: u8; b",
            "f: pub_fn(x),",
            "f: [A, B],",
            "f: [],",
            "f: [;3],",
            "f: [;],",
            "f: [T;],",
            "f: <>,",
            "f: [A, B; 3],",
            "f: a * b,",
            "f: a - b,",
            "f: a & b,",
            "f: |x| x,",
            "f: BASE + 1,",
            "f: a | b,",
            "f: 17,",
            "f: \"s\",",
            "f: true,",
            "f: 1.5,",
            "f: 'a',",
            "f: a.b,",
            "f: x as u8,",
            "f: -1,",
            "f: 1..2,",
            "f: a?,",
            "f: a && b,",
            "f: a < b,",
            "f: (1, 2),",
            "f: [1, 2],",
            "f: [0u8; 3],",
        ] {
            let source = format!("m! {{\n    struct S {{\n        {body}\n    }}\n}}\n");
            assert!(
                !decl(&source, 3, body)?,
                "expression-bearing field keeps its probe: {body}"
            );
        }
        // The header search stops at every non-struct owner and identifier.
        for (header, why) in [
            ("fn go()", "fn body"),
            ("impl K", "impl body"),
            ("enum E", "enum body"),
            ("trait T", "trait body"),
            ("let x =", "assignment"),
            ("x =>", "match arm"),
            ("let y = 1; Foo", "after a statement"),
            ("my_struct", "identifier ending in struct"),
            ("struct S(u8), T", "tuple struct then literal"),
            ("struct S { z: u8 } T", "closed struct then literal"),
        ] {
            let source = format!("m! {{\n    {header} {{\n        a: u8,\n    }}\n}}\n");
            assert!(
                !decl(&source, 3, "a: u8,")?,
                "{why}: `{header}` must not own a declaration"
            );
        }
        // A long generic header is searched to the tree boundary, not capped.
        let generics = (0..40)
            .map(|i| format!("A{i}: Clone"))
            .collect::<Vec<_>>()
            .join(", ");
        let source =
            format!("m! {{\n    pub struct S<{generics}> {{\n        a: u8,\n    }}\n}}\n");
        assert!(
            decl(&source, 3, "a: u8,")?,
            "long struct header must still be found"
        );
        Ok(())
    }

    #[test]
    fn bound_fallback_rejects_const_expression_and_macro_authority() -> Result<(), String> {
        for text in ["T: Trait<{ construct() }>,", "T: Trait<types!()>,"] {
            let source = format!("fn f<T>()\nwhere\n    {text}\n{{}}\n");
            let facts = declaration_facts(&source)?;
            assert!(
                declaration_shape(&facts, 3, text).is_none(),
                "expression or macro acquired declaration authority: {text}"
            );
        }
        Ok(())
    }

    #[test]
    fn parameter_shape_preserves_exact_coordinates_and_unknown_family() -> Result<(), String> {
        for source in [SOURCE.to_string(), SOURCE.replace('\n', "\r\n")] {
            let facts = declaration_facts(&source)?;
            let shape = declaration_shape(&facts, 3, "    out: &Path,")
                .ok_or_else(|| "missing parser-confirmed parameter".to_string())?;
            assert_eq!(shape.family, ProbeFamily::StaticUnknown);
            assert_eq!(shape.start_line, 3);
            assert_eq!(Some(shape.start_byte), source.find("out: &Path,"));
            assert_eq!(shape.text, "out: &Path,");
            assert!(!shape.standalone_call);
            assert!(!shape.unsafe_boundary);
        }
        Ok(())
    }

    #[test]
    fn parameter_shape_requires_current_parser_owned_source() -> Result<(), String> {
        let mut facts = declaration_facts(SOURCE)?;
        assert!(declaration_shape(&facts, 3, "out: &Other,").is_none());
        assert!(declaration_shape(&facts, 0, "out: &Path,").is_none());
        assert!(declaration_shape(&facts, 30, "out: &Path,").is_none());
        facts.used_lexical_fallback = true;
        assert!(declaration_shape(&facts, 3, "out: &Path,").is_none());
        facts.used_lexical_fallback = false;
        facts.source = "struct Path;\nfn project(\n    out: &Path,\n".into();
        assert!(declaration_shape(&facts, 3, "out: &Path,").is_none());
        facts.source = std::sync::Arc::from("");
        assert!(declaration_shape(&facts, 3, "out: &Path,").is_none());
        Ok(())
    }

    #[test]
    fn parameter_shape_rejects_non_fn_context_and_shared_body() -> Result<(), String> {
        for (source, line, text) in [
            (
                "struct Path;\nstruct Wrap<'a> { out: &'a Path }\nfn p() -> Wrap<'static> {\n    Wrap {\n        out: &Path,\n    }\n}\n",
                5,
                "out: &Path,",
            ),
            (
                "struct Path;\ntype Handler = fn(\n    out: &Path,\n);\n",
                3,
                "out: &Path,",
            ),
            (
                "struct Path;\nfn project(\n    out: &Path,) -> i32 { 7 }\n",
                3,
                "out: &Path,) -> i32 { 7 }",
            ),
            (
                "struct Path;\nfn project() { let callback = |\n    out: &Path,\n| 7; }\n",
                3,
                "out: &Path,",
            ),
            (
                "struct Path;\nfn project(\n    out: &Path, /* note */) -> i32 { 7 }\n",
                3,
                "out: &Path, /* note */) -> i32 { 7 }",
            ),
            (
                "struct Path;\nfn project(\n    out: &Path, /* note */ other: &Path,\n) {}\n",
                3,
                "out: &Path, /* note */ other: &Path,",
            ),
        ] {
            let facts = declaration_facts(source)?;
            assert!(
                declaration_shape(&facts, line, text).is_none(),
                "non-parameter or shared line was reclassified: {source}"
            );
        }
        Ok(())
    }

    #[test]
    fn parameter_comment_trivia_preserves_text_and_byte_coordinates() -> Result<(), String> {
        for suffix in [
            ", // output: never executable",
            ", /* output */",
            ", /* outer /* nested */ note */ // tail",
            " /* no comma on last parameter */",
        ] {
            for newline in ["\n", "\r\n"] {
                let text = format!("out: &Path{suffix}");
                let source = format!("// λ\nstruct Path;\nfn project(\n    {text}\n) {{}}\n")
                    .replace('\n', newline);
                let facts = declaration_facts(&source)?;
                let shape = declaration_shape(&facts, 4, &text)
                    .ok_or_else(|| format!("commented parameter was lost: {text}"))?;
                assert_eq!(shape.family, ProbeFamily::StaticUnknown);
                assert_eq!(shape.start_line, 4);
                assert_eq!(Some(shape.start_byte), source.find("out: &Path"));
                assert_eq!(shape.text, text);
                assert!(!shape.unsafe_boundary);
            }
        }
        Ok(())
    }

    #[test]
    fn record_fields_preserve_trivia_visibility_and_byte_identity() -> Result<(), String> {
        for text in [
            "value: Marker,",
            "pub value: Marker, // field",
            "pub(crate) value: Marker, /* outer /* nested */ tail */",
            "value: Marker /* final field */",
        ] {
            for newline in ["\n", "\r\n"] {
                let source = format!("// λ\nstruct Marker;\nstruct Packet {{\n    {text}\n}}\n")
                    .replace('\n', newline);
                let facts = declaration_facts(&source)?;
                let shape = declaration_shape(&facts, 4, text)
                    .ok_or_else(|| format!("record declaration was not retained: {text}"))?;
                assert_eq!(shape.family, ProbeFamily::StaticUnknown);
                assert_eq!(shape.text, text);
                assert_eq!(shape.start_line, 4);
                assert_eq!(Some(shape.start_byte), source.find(text));
                assert!(!shape.standalone_call);
                assert!(!shape.unsafe_boundary);
            }
        }
        Ok(())
    }

    #[test]
    fn record_field_fallback_requires_current_valid_source() -> Result<(), String> {
        let source = "struct Packet {\n    value: u8,\n}\n";
        let mut facts = declaration_facts(source)?;
        assert!(declaration_shape(&facts, 2, "value: u8,").is_some());
        assert!(declaration_shape(&facts, 2, "value: u16,").is_none());
        assert!(declaration_shape(&facts, 0, "value: u8,").is_none());
        assert!(declaration_shape(&facts, 99, "value: u8,").is_none());
        facts.used_lexical_fallback = true;
        assert!(declaration_shape(&facts, 2, "value: u8,").is_none());
        facts.used_lexical_fallback = false;
        facts.source = "struct Packet {\n    value: u8,\n".into();
        assert!(declaration_shape(&facts, 2, "value: u8,").is_none());
        facts.source = std::sync::Arc::from("");
        assert!(declaration_shape(&facts, 2, "value: u8,").is_none());
        Ok(())
    }

    #[test]
    fn record_field_fallback_rejects_shared_or_unsupported_contexts() -> Result<(), String> {
        for (source, line, text) in [
            (
                "struct Packet {\n    first: u8, /* separator */ second: u8,\n}\n",
                2,
                "first: u8, /* separator */ second: u8,",
            ),
            (
                "struct Packet { value: u8 } fn make() -> Packet { Packet { value: 7 } }\n",
                1,
                "struct Packet { value: u8 } fn make() -> Packet { Packet { value: 7 } }",
            ),
            ("union Packet {\n    value: u8,\n}\n", 2, "value: u8,"),
            (
                "const fn width() -> usize { 4 }\nstruct Packet {\n    value: [u8; width()],\n}\n",
                3,
                "value: [u8; width()],",
            ),
            (
                "struct Packet {\n    value: field_type!(),\n}\n",
                2,
                "value: field_type!(),",
            ),
        ] {
            let facts = declaration_facts(source)?;
            assert!(
                declaration_shape(&facts, line, text).is_none(),
                "shared or unsupported field context was admitted: {text}"
            );
        }
        Ok(())
    }

    /// #8999: enum variant fields (multi-line and one-line variants) are
    /// type declarations with exact coordinates, LF and CRLF.
    #[test]
    fn enum_variant_fields_preserve_trivia_and_byte_identity() -> Result<(), String> {
        for (text, line) in [
            ("value: u8,", 5),
            ("pub_ok: Marker, // field", 5),
            ("Short { offset: usize },", 7),
        ] {
            for newline in ["\n", "\r\n"] {
                let source = format!(
                    "// λ\nstruct Marker;\nenum Packet {{\n    Data {{\n    {}\n    }},\n    {}\n}}\n",
                    if line == 5 { text } else { "value: u8," },
                    if line == 7 { text } else { "Plain," },
                )
                .replace('\n', newline);
                let facts = declaration_facts(&source)?;
                let shape = declaration_shape(&facts, line, text)
                    .ok_or_else(|| format!("variant declaration was not retained: {text}"))?;
                assert_eq!(shape.family, ProbeFamily::StaticUnknown);
                assert_eq!(shape.text, text);
                assert_eq!(shape.start_line, line);
                assert_eq!(Some(shape.start_byte), source.find(text));
                assert!(!shape.standalone_call);
                assert!(!shape.unsafe_boundary);
            }
        }
        Ok(())
    }

    /// #8999 negatives: executable or shared syntax keeps its real probes.
    #[test]
    fn enum_variant_declaration_authority_rejects_expressions_and_shared_lines()
    -> Result<(), String> {
        for (source, line, text) in [
            // Expression inside the field type.
            (
                "const N: usize = 1;\nenum E {\n    A { x: [u8; N + 1] },\n}\n",
                3,
                "A { x: [u8; N + 1] },",
            ),
            (
                "const N: usize = 1;\nenum E {\n    A {\n        x: [u8; N + 1],\n    },\n}\n",
                4,
                "x: [u8; N + 1],",
            ),
            // Declaration shares its line with a constructing body.
            (
                "enum E { A { x: u8 } } fn f() -> E { E::A { x: 7 } }\n",
                1,
                "enum E { A { x: u8 } } fn f() -> E { E::A { x: 7 } }",
            ),
            // The constructing body itself is never a declaration.
            (
                "enum E { A { x: u8 } }\nfn f() -> E {\n    E::A {\n        x: 7,\n    }\n}\n",
                4,
                "x: 7,",
            ),
            // Tuple variants carry no named field.
            ("enum E {\n    A(u8, u8),\n}\n", 2, "A(u8, u8),"),
            // A discriminant is expression syntax on the variant.
            ("enum E {\n    A { x: u8 } = 1,\n}\n", 2, "A { x: u8 } = 1,"),
            // A macro-typed field is macro syntax.
            (
                "macro_rules! m { () => { u8 } }\nenum E {\n    A { x: m!() },\n}\n",
                3,
                "A { x: m!() },",
            ),
            // An attribute carrying an expression is not plain declaration text.
            (
                "enum E {\n    #[doc = concat!(\"a\", \"b\")] A { x: u8 },\n}\n",
                2,
                "#[doc = concat!(\"a\", \"b\")] A { x: u8 },",
            ),
        ] {
            let facts = declaration_facts(source)?;
            assert!(
                declaration_shape(&facts, line, text).is_none(),
                "admitted as a variant declaration: {text}"
            );
        }
        Ok(())
    }

    fn removed(line: usize, text: &str) -> crate::analysis::diff::ChangedLine {
        crate::analysis::diff::ChangedLine {
            line,
            new_side_line: line,
            text: text.into(),
        }
    }

    #[test]
    fn removed_field_declarations_need_exact_old_line_and_one_owner() {
        let struct_src = "struct P {\n    offset: usize,\n    a: u8, b: u8,\n}\n";
        let enum_src = "enum E {\n    A {\n        x: u8,\n    },\n    B { y: u8 },\n}\n";
        for newline in ["\n", "\r\n"] {
            let struct_src = struct_src.replace('\n', newline);
            let enum_src = enum_src.replace('\n', newline);
            let lines = [removed(2, "offset: usize,"), removed(3, "a: u8, b: u8,")];
            assert_eq!(field_declaration_lines(&struct_src, &lines), [true, false]);
            // The variant's opening line is not read as a construction, so
            // it never takes the old-source disposition.
            let lines = [
                removed(2, "A {"),
                removed(3, "x: u8,"),
                removed(5, "B { y: u8 },"),
            ];
            assert_eq!(
                field_declaration_lines(&enum_src, &lines),
                [false, true, true]
            );
        }
        // A candidate-side line that is not this old line, or one outside the
        // file, never borrows old-source authority.
        let lines = [removed(2, "other: usize,"), removed(0, "offset: usize,")];
        assert_eq!(field_declaration_lines(struct_src, &lines), [false, false]);
        let lines = [removed(99, "offset: usize,")];
        assert_eq!(field_declaration_lines(struct_src, &lines), [false]);
        // An unparsable old source refuses everything.
        let lines = [removed(2, "offset: usize,")];
        assert_eq!(
            field_declaration_lines("struct P {\n    offset: usize,\n", &lines),
            [false]
        );
    }

    #[test]
    fn removed_field_declarations_reject_expressions_bodies_and_macros() {
        for (source, line, text) in [
            (
                "fn f() -> P {\n    P { offset: 1 }\n}\n",
                2,
                "P { offset: 1 }",
            ),
            (
                "#[repr(u8)]\nenum E {\n    A { x: u8 } = 3,\n}\n",
                3,
                "A { x: u8 } = 3,",
            ),
            (
                "macro_rules! m { () => { u8 } }\nstruct S {\n    x: m!(),\n}\n",
                3,
                "x: m!(),",
            ),
            ("union U {\n    x: u8,\n}\n", 2, "x: u8,"),
            ("fn f(\n    out: &P,\n) {}\n", 2, "out: &P,"),
        ] {
            let lines = [removed(line, text)];
            assert_eq!(
                field_declaration_lines(source, &lines),
                [false],
                "admitted removed field declaration: {text}"
            );
        }
    }
}
