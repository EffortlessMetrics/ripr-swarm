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
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
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

/// The record-struct fields one file declares, parsed once: a struct name
/// declared more than once, or a field declared twice, has no entry.
pub(in crate::analysis) struct FieldTable {
    struct_counts: BTreeMap<String, usize>,
    fields: BTreeMap<(String, String), Option<FieldReach>>,
}

impl FieldTable {
    /// `None` when the source does not parse cleanly.
    pub(in crate::analysis) fn parse(source: &str) -> Option<Self> {
        let parse = crate::analysis::syntax::parse_clean_source_file(source)?;
        let mut table = Self {
            struct_counts: BTreeMap::new(),
            fields: BTreeMap::new(),
        };
        for item in parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Struct::cast)
        {
            let Some(name) = item.name() else { continue };
            let name = name.text().to_string();
            *table.struct_counts.entry(name.clone()).or_insert(0) += 1;
            let Some(ast::FieldList::RecordFieldList(list)) = item.field_list() else {
                continue;
            };
            for field in list.fields() {
                let Some(field_name) = field.name() else {
                    continue;
                };
                let field_name = field_name.text().trim_start_matches("r#").to_string();
                let reach = reach_of(field.visibility());
                match table.fields.entry((name.clone(), field_name)) {
                    Entry::Vacant(slot) => {
                        slot.insert(Some(reach));
                    }
                    Entry::Occupied(mut slot) => {
                        slot.insert(None);
                    }
                }
            }
        }
        Some(table)
    }

    /// The declared reach of `field` on the impl type of an owner in this
    /// file, when that type is declared exactly once with that field.
    pub(in crate::analysis) fn reach(
        &self,
        impl_context: &FunctionImplContext,
        field: &str,
    ) -> Option<FieldReach> {
        let FunctionImplContext::Impl { self_type } = impl_context else {
            return None;
        };
        let type_name = self_type.split('<').next().unwrap_or(self_type).trim();
        if self.struct_counts.get(type_name) != Some(&1) {
            return None;
        }
        self.fields
            .get(&(type_name.to_string(), field.to_string()))
            .copied()
            .flatten()
    }
}

fn reach_of(visibility: Option<ast::Visibility>) -> FieldReach {
    let Some(visibility) = visibility else {
        return FieldReach::Module;
    };
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

/// The declared reach of `field` on the owner's impl type, when the owner's
/// file declares that type exactly once as a record struct with that field.
pub(in crate::analysis) fn declared_field_reach(
    owner: &FunctionSummary,
    field: &str,
    index: &RustIndex,
) -> Option<FieldReach> {
    let facts = index.files().get(owner.file.as_path())?;
    FieldTable::parse(&facts.source)?.reach(&owner.impl_context, field)
}

/// Whether `masked` (comments and strings masked) calls `name` as a bare,
/// method or path call: `name(` not continuing a longer identifier.
fn mentions_call(masked: &str, name: &str) -> bool {
    let call = format!("{name}(");
    masked.match_indices(&call).any(|(at, _)| {
        !masked[..at]
            .chars()
            .next_back()
            .is_some_and(|before| before.is_alphanumeric() || before == '_')
    })
}

/// Every test, in any file, that may run `owner`: it calls the owner's name on
/// the owner's type (the test sits in the owner's file or names the type), or
/// calls a function that does, up to three hops. This is a structural
/// over-approximation of the relations the finding is graded over: a test the
/// relation machinery misses (an aliased, iterated or closure receiver, a
/// helper) is still found, and the start withholds unless it too is a placed
/// weak read.
pub(in crate::analysis) fn tests_that_may_run_owner<'a>(
    owner: &FunctionSummary,
    index: &'a RustIndex,
) -> Vec<&'a crate::analysis::rust_index::TestSummary> {
    let FunctionImplContext::Impl { self_type } = &owner.impl_context else {
        return Vec::new();
    };
    let type_name = self_type.split('<').next().unwrap_or(self_type).trim();
    let mask = |text: &str| crate::analysis::extract::mask_comments_and_strings(text);
    let mut names: Vec<String> = Vec::new();
    // Functions that call the owner, then functions that call those.
    let mut callers: Vec<(String, String)> = Vec::new();
    for function in index.functions().iter() {
        if function.id == owner.id {
            continue;
        }
        callers.push((function.name.clone(), mask(function.body.as_ref())));
    }
    let mut called = vec![owner.name.clone()];
    for _ in 0..3 {
        let mut next: Vec<String> = Vec::new();
        for (name, body) in &callers {
            if !names.contains(name) && called.iter().any(|callee| mentions_call(body, callee)) {
                names.push(name.clone());
                next.push(name.clone());
            }
        }
        if next.is_empty() {
            break;
        }
        called = next;
    }
    index
        .tests()
        .iter()
        .filter(|test| {
            let body = mask(test.body.as_ref());
            (mentions_call(&body, &owner.name)
                && (test.file == owner.file || body.contains(type_name)))
                || names.iter().any(|helper| mentions_call(&body, helper))
        })
        .collect()
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
