//! RIPR-SPEC-0348 Rule 5: `Self` written for the implementing type's own
//! name, or the reverse, is not a behavior edit.
//!
//! Inside `impl Shell { .. }` or `impl Generator for Shell { .. }` the path
//! `Shell::Bash` and the path `Self::Bash` name the same item, so a line that
//! differs from its old line only by that spelling (the
//! `clippy::use_self` cleanup) changes nothing a test could notice. A
//! positional replacement pair counts only when both files prove it:
//!
//! - the two lines are byte-identical apart from identifiers where one side
//!   spells `Self` and the other spells one type name, the same name for
//!   every difference on the line;
//! - on each side the differing token is real code (not comment, string or
//!   macro input) and is the unqualified first segment of a path with no
//!   generic arguments;
//! - on each side the innermost enclosing `impl` implements exactly that
//!   bare name (`impl Shell`, `impl Trait for Shell`), not `Shell<T>`,
//!   `&Shell`, `Box<Shell>` or a qualified path;
//! - between the token and that `impl` there is only the associated item
//!   that holds it: a nested item would give `Self` another meaning, and a
//!   generic parameter, block-level item or `use` could rebind the name;
//! - the `impl` and that associated item carry only attributes that never
//!   read their tokens, so no attribute macro can observe the spelling;
//! - neither file declares the type with generic parameters (a defaulted
//!   parameter makes `Shell::new()` infer what `Self::new()` fixes).
//!
//! Anything else (a different type name, a line that also changes another
//! token, a token inside `matches!` or `assert!`, an `impl` for a reference)
//! stays probed.

use crate::analysis::diff::ChangedFile;
use ra_ap_syntax::ast::{HasGenericArgs, HasName};
use ra_ap_syntax::{AstNode, Edition, SourceFile, SyntaxKind, SyntaxNode, SyntaxToken, ast};
use std::collections::BTreeSet;

/// Attributes that never read the tokens of the item they sit on. Any other
/// attribute on the `impl` or on the associated item (`#[async_trait]`,
/// `#[instrument]`, a `cfg_attr` that may apply one) keeps the pair probed.
const TOKEN_BLIND_ATTRIBUTES: [&str; 13] = [
    "inline",
    "must_use",
    "allow",
    "expect",
    "warn",
    "deny",
    "cfg",
    "doc",
    "cold",
    "rustfmt",
    "track_caller",
    "deprecated",
    "automatically_derived",
];

const SELF_TYPE: &str = "Self";

/// Lines of one changed file that only respell the implementing type as
/// `Self` or back: the new-side lines of the added half and the old-side
/// lines of the removed half of each positional replacement pair.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct SelfTypeRespelledLines {
    pub(super) added: BTreeSet<usize>,
    pub(super) removed: BTreeSet<usize>,
}

pub(super) fn self_type_respelled_lines(
    changed: &ChangedFile,
    new_source: &str,
    old_source: &str,
) -> SelfTypeRespelledLines {
    let mut result = SelfTypeRespelledLines::default();
    let added_text = changed
        .added_lines
        .iter()
        .map(|line| (line.new_side_line, line.text.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let candidates = changed
        .replacement_counterparts()
        .into_iter()
        .filter_map(|(new_line, removed)| {
            let added = added_text.get(&new_line)?;
            let swaps = self_type_swaps(added, &removed.text)?;
            Some((new_line, removed.line, swaps))
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return result;
    }
    // Parse only when a line reads as a candidate. A file the parser would
    // refuse or cannot read cleanly proves nothing.
    let (Some(new), Some(old)) = (ParsedSide::new(new_source), ParsedSide::new(old_source)) else {
        return result;
    };
    for (new_line, old_line, swaps) in candidates {
        if new.declares_generic(&swaps.type_name) || old.declares_generic(&swaps.type_name) {
            continue;
        }
        let established = swaps.columns.iter().all(|swap| {
            new.names_impl_self_type(new_line, swap.added_column, swap.added, &swaps.type_name)
                && old.names_impl_self_type(
                    old_line,
                    swap.removed_column,
                    swap.removed,
                    &swaps.type_name,
                )
        });
        if established {
            result.added.insert(new_line);
            result.removed.insert(old_line);
        }
    }
    result
}

/// The identifier differences of one candidate pair.
struct SelfTypeSwaps<'a> {
    /// The one type name every difference trades with `Self`.
    type_name: String,
    columns: Vec<Swap<'a>>,
}

/// One differing identifier: its spelling and byte column on each line.
struct Swap<'a> {
    added: &'a str,
    added_column: usize,
    removed: &'a str,
    removed_column: usize,
}

/// The `Self`/type-name swaps that are the only difference between `added`
/// and `removed`, or `None` when the lines differ any other way.
fn self_type_swaps<'a>(added: &'a str, removed: &'a str) -> Option<SelfTypeSwaps<'a>> {
    if added == removed || !(has_word(added, SELF_TYPE) || has_word(removed, SELF_TYPE)) {
        return None;
    }
    let added_pieces = pieces(added);
    let removed_pieces = pieces(removed);
    if added_pieces.len() != removed_pieces.len() {
        return None;
    }
    let mut type_name: Option<&str> = None;
    let mut columns = Vec::new();
    for (&(added_column, new), &(removed_column, old)) in added_pieces.iter().zip(&removed_pieces) {
        if new == old {
            continue;
        }
        let name = match (new == SELF_TYPE, old == SELF_TYPE) {
            (true, false) => old,
            (false, true) => new,
            _ => return None,
        };
        if !is_identifier(name) || type_name.is_some_and(|known| known != name) {
            return None;
        }
        type_name = Some(name);
        columns.push(Swap {
            added: new,
            added_column,
            removed: old,
            removed_column,
        });
    }
    Some(SelfTypeSwaps {
        type_name: type_name?.to_owned(),
        columns,
    })
}

/// `line` as identifier-shaped words and single other characters, each with
/// its byte column. Whitespace is kept, so equal pieces mean equal bytes.
fn pieces(line: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut word_start: Option<usize> = None;
    for (at, ch) in line.char_indices() {
        if is_word_char(ch) {
            word_start.get_or_insert(at);
            continue;
        }
        if let Some(start) = word_start.take() {
            out.push((start, &line[start..at]));
        }
        out.push((at, &line[at..at + ch.len_utf8()]));
    }
    if let Some(start) = word_start {
        out.push((start, &line[start..]));
    }
    out
}

fn is_word_char(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

fn is_identifier(word: &str) -> bool {
    word.chars()
        .next()
        .is_some_and(|first| first == '_' || first.is_alphabetic())
        && word.chars().all(is_word_char)
}

fn has_word(line: &str, word: &str) -> bool {
    pieces(line).iter().any(|(_, piece)| *piece == word)
}

/// One side of the file as the parser reads it.
struct ParsedSide {
    root: SyntaxNode,
    /// Byte offset where each 1-based line starts (`line_starts[0]` is line 1).
    line_starts: Vec<usize>,
}

impl ParsedSide {
    fn new(source: &str) -> Option<Self> {
        if crate::analysis::syntax::rust_nesting_refusal(source).is_some() {
            return None;
        }
        let parse = SourceFile::parse(source, Edition::CURRENT);
        if !parse.errors().is_empty() {
            return None;
        }
        let mut line_starts = vec![0];
        line_starts.extend(source.match_indices('\n').map(|(at, _)| at + 1));
        Some(Self {
            root: parse.tree().syntax().clone(),
            line_starts,
        })
    }

    /// Whether this file declares `name` as a type with generic parameters.
    fn declares_generic(&self, name: &str) -> bool {
        self.root.descendants().any(|node| {
            let (declared, generics) = if let Some(item) = ast::Struct::cast(node.clone()) {
                (item_name(&item), generic_params(&item))
            } else if let Some(item) = ast::Enum::cast(node.clone()) {
                (item_name(&item), generic_params(&item))
            } else if let Some(item) = ast::Union::cast(node.clone()) {
                (item_name(&item), generic_params(&item))
            } else if let Some(item) = ast::TypeAlias::cast(node) {
                (item_name(&item), generic_params(&item))
            } else {
                return false;
            };
            generics && declared.as_deref() == Some(name)
        })
    }

    /// Whether the token spelled `spelled` at `column` of 1-based `line` is
    /// an unqualified path start that names the self type of its innermost
    /// `impl`, and that `impl` implements exactly the bare `type_name`.
    fn names_impl_self_type(
        &self,
        line: usize,
        column: usize,
        spelled: &str,
        type_name: &str,
    ) -> bool {
        let Some(token) = self.token_at(line, column) else {
            return false;
        };
        let expected_kind = if spelled == SELF_TYPE {
            SyntaxKind::SELF_TYPE_KW
        } else {
            SyntaxKind::IDENT
        };
        if token.text() != spelled || token.kind() != expected_kind {
            return false;
        }
        unqualified_path_start(&token) && governed_by_impl_of(&token, type_name)
    }

    fn token_at(&self, line: usize, column: usize) -> Option<SyntaxToken> {
        let start = *self.line_starts.get(line.checked_sub(1)?)?;
        let offset = u32::try_from(start.checked_add(column)?).ok()?;
        self.root.token_at_offset(offset.into()).right_biased()
    }
}

fn item_name(item: &impl ast::HasName) -> Option<String> {
    item.name().map(|name| name.text().to_string())
}

fn generic_params(item: &impl ast::HasGenericParams) -> bool {
    item.generic_param_list().is_some()
}

/// Whether `token` is the whole first segment of a path: no qualifier
/// before it (`crate::Shell`, `<T as Tr>::Shell`) and no generic arguments
/// on it (`Shell::<T>`).
fn unqualified_path_start(token: &SyntaxToken) -> bool {
    let Some(segment) = token
        .parent()
        .and_then(ast::NameRef::cast)
        .and_then(|name| name.syntax().parent())
        .and_then(ast::PathSegment::cast)
    else {
        return false;
    };
    if segment.generic_arg_list().is_some() || segment.coloncolon_token().is_some() {
        return false;
    }
    segment
        .syntax()
        .parent()
        .and_then(ast::Path::cast)
        .is_some_and(|path| path.qualifier().is_none())
}

/// Whether the innermost `impl` around `token` implements the bare
/// `type_name` and nothing between the two can change what `Self` or the
/// name means.
fn governed_by_impl_of(token: &SyntaxToken, type_name: &str) -> bool {
    let mut holder: Option<SyntaxNode> = None;
    for node in token.parent_ancestors() {
        if let Some(implementation) = ast::Impl::cast(node.clone()) {
            return impl_names_bare_type(&implementation, type_name)
                && token_blind_attributes(&implementation)
                && holder
                    .as_ref()
                    .is_none_or(|item| held_directly(item, &implementation));
        }
        match node.kind() {
            // Macro input is not code the parser resolved.
            SyntaxKind::TOKEN_TREE | SyntaxKind::MACRO_CALL | SyntaxKind::MACRO_RULES => {
                return false;
            }
            SyntaxKind::BLOCK_EXPR | SyntaxKind::STMT_LIST if block_declares_items(&node) => {
                return false;
            }
            _ => {}
        }
        if rebinds_name(&node, type_name) {
            return false;
        }
        if ast::Item::can_cast(node.kind()) || ast::AssocItem::can_cast(node.kind()) {
            // One item may hold the token: the associated item itself.
            if holder.is_some()
                || !holder_is_token_blind(&node)
                || binds_pattern_named(&node, type_name)
            {
                return false;
            }
            holder = Some(node);
        }
    }
    false
}

/// `impl Shell` or `impl Trait for Shell`: the self type is one bare name.
fn impl_names_bare_type(implementation: &ast::Impl, type_name: &str) -> bool {
    let Some(ast::Type::PathType(self_type)) = implementation.self_ty() else {
        return false;
    };
    let Some(path) = self_type.path() else {
        return false;
    };
    path.qualifier().is_none()
        && path.segment().is_some_and(|segment| {
            segment.generic_arg_list().is_none()
                && segment.coloncolon_token().is_none()
                && segment
                    .name_ref()
                    .is_some_and(|name| name.text() == type_name)
        })
}

fn held_directly(item: &SyntaxNode, implementation: &ast::Impl) -> bool {
    implementation
        .assoc_item_list()
        .is_some_and(|list| item.parent().as_ref() == Some(list.syntax()))
}

/// A block that declares an item or imports a name can rebind the type name
/// for everything inside it.
fn block_declares_items(node: &SyntaxNode) -> bool {
    node.children()
        .any(|child| ast::Item::can_cast(child.kind()))
}

/// A generic parameter of `node` named like the type shadows it.
fn rebinds_name(node: &SyntaxNode, type_name: &str) -> bool {
    node.children()
        .filter_map(ast::GenericParamList::cast)
        .flat_map(|list| list.generic_params())
        .any(|param| match param {
            ast::GenericParam::TypeParam(param) => {
                param.name().is_some_and(|name| name.text() == type_name)
            }
            ast::GenericParam::ConstParam(param) => {
                param.name().is_some_and(|name| name.text() == type_name)
            }
            ast::GenericParam::LifetimeParam(_) => false,
        })
}

/// A binding spelled like the type (`let Shell = ..`) shadows it as a value.
fn binds_pattern_named(item: &SyntaxNode, type_name: &str) -> bool {
    item.descendants()
        .filter_map(ast::IdentPat::cast)
        .any(|pattern| pattern.name().is_some_and(|name| name.text() == type_name))
}

fn holder_is_token_blind(node: &SyntaxNode) -> bool {
    node.children()
        .filter_map(ast::Attr::cast)
        .all(|attribute| attribute_is_token_blind(&attribute))
}

fn token_blind_attributes(item: &impl ast::HasAttrs) -> bool {
    item.attrs()
        .all(|attribute| attribute_is_token_blind(&attribute))
}

fn attribute_is_token_blind(attribute: &ast::Attr) -> bool {
    let Some(path) = attribute.meta().and_then(|meta| meta.path()) else {
        return false;
    };
    let spelled = path.syntax().text().to_string();
    // A tool lint namespace (`clippy::use_self`) appears only as the
    // argument of `allow`/`warn`, never as the attribute itself.
    TOKEN_BLIND_ATTRIBUTES.contains(&spelled.as_str())
}
