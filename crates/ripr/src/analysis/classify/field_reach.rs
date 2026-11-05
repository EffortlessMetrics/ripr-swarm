//! Who can name a written field (#11870 remainder).
//!
//! A weak read of a written field starts a repair only where the related test
//! can legitimately spell the read. A plain `pub` field is nameable anywhere.
//! A private field is nameable inside the module that declares it, which this
//! reading takes as the declaring file (an inline test module); a `pub(crate)`
//! field also from any other file under the same crate's `src/` (unit tests in
//! a child module file), never from `tests/`, `benches/` or `examples/`, which
//! are separate crates. Any other restriction (`pub(super)`, `pub(in ..)`) is
//! read as private. A declaration this reading cannot place (the owner's impl
//! type is not a unique record struct in the owner's file) imposes no
//! restriction here: the repository seam gate admits a non-public field only
//! through the same policy, and a diff finding keeps its earlier reading.

use crate::analysis::facts::FunctionImplContext;
use crate::analysis::rust_index::{FunctionSummary, RustIndex};
use ra_ap_syntax::ast::{self, HasName, HasVisibility};
use ra_ap_syntax::{AstNode, SyntaxKind};
use std::path::{Component, Path};

/// How far a declared field's name reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis) enum FieldReach {
    /// Plain `pub`.
    Public,
    /// `pub(crate)`.
    Crate,
    /// Private, or restricted to a module path ripr does not model.
    Module,
}

/// The declared reach of `field` on the owner's impl type, when the owner's
/// file declares that type exactly once as a record struct with that field.
pub(in crate::analysis) fn declared_field_reach(
    owner: &FunctionSummary,
    field: &str,
    index: &RustIndex,
) -> Option<FieldReach> {
    let FunctionImplContext::Impl { self_type } = &owner.impl_context else {
        return None;
    };
    let type_name = self_type.split('<').next().unwrap_or(self_type).trim();
    let facts = index.files().get(owner.file.as_path())?;
    let parse = crate::analysis::syntax::parse_clean_source_file(&facts.source)?;
    let mut declared = parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Struct::cast)
        .filter(|item| item.name().is_some_and(|name| name.text() == type_name));
    let item = declared.next()?;
    if declared.next().is_some() {
        return None;
    }
    let ast::FieldList::RecordFieldList(fields) = item.field_list()? else {
        return None;
    };
    let mut named = fields.fields().filter(|candidate| {
        candidate
            .name()
            .is_some_and(|name| name.text().trim_start_matches("r#") == field)
    });
    let declared_field = named.next()?;
    if named.next().is_some() {
        return None;
    }
    Some(match declared_field.visibility() {
        None => FieldReach::Module,
        Some(visibility) => {
            let text: String = visibility
                .syntax()
                .descendants_with_tokens()
                .filter(|element| element.kind() != SyntaxKind::WHITESPACE)
                .filter_map(|element| element.into_token())
                .map(|token| token.text().to_string())
                .collect();
            match text.as_str() {
                "pub" => FieldReach::Public,
                "pub(crate)" => FieldReach::Crate,
                _ => FieldReach::Module,
            }
        }
    })
}

/// Whether a test in `test_file` can spell a read of a field of `reach`
/// declared in `owner_file`.
pub(in crate::analysis) fn test_can_name_field(
    reach: FieldReach,
    owner_file: &Path,
    test_file: &Path,
) -> bool {
    match reach {
        FieldReach::Public => true,
        FieldReach::Module => test_file == owner_file,
        FieldReach::Crate => test_file == owner_file || same_library_crate(owner_file, test_file),
    }
}

/// Whether both files sit under one `src/` directory outside `src/bin/` and
/// `src/main.rs` (a binary is its own crate).
fn same_library_crate(owner_file: &Path, test_file: &Path) -> bool {
    let (Some(owner_src), Some(test_src)) = (src_root(owner_file), src_root(test_file)) else {
        return false;
    };
    owner_src == test_src
}

fn src_root(file: &Path) -> Option<Vec<Component<'_>>> {
    let parts: Vec<Component<'_>> = file.components().collect();
    let src_at = parts
        .iter()
        .rposition(|part| matches!(part, Component::Normal(name) if *name == "src"))?;
    let below = &parts[src_at + 1..];
    let is_binary = matches!(below.first(), Some(Component::Normal(name)) if *name == "bin" || *name == "main.rs");
    (!is_binary && !below.is_empty()).then(|| parts[..=src_at].to_vec())
}

#[cfg(test)]
mod tests;
