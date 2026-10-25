//! Closed dispatch for the fixture's iterator and literal String construction.
//! This gate deliberately withholds unread imports and macro-supplied methods;
//! it does not attempt general Rust method resolution.

use crate::analysis::classify::owner_pin::item_nesting_with_impl;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::rust_index::{FunctionSummary, RustIndex};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasName},
};
use std::path::PathBuf;

pub(super) struct FixtureDispatch {
    file: PathBuf,
    owner_modules: Vec<String>,
}

impl FixtureDispatch {
    pub(super) fn establish(owner: &FunctionSummary, index: &RustIndex) -> Option<Self> {
        if !owner.attrs.is_empty()
            || !owner.impl_attrs.is_empty()
            || index.functions().iter().any(|function| {
                function.item.has_self_param
                    && matches!(
                        normal_name(&function.name),
                        "iter" | "map" | "sum" | "to_string"
                    )
            })
            || index.files().values().any(|facts| {
                mask_comments_and_strings(&facts.source).contains("no_implicit_prelude")
            })
        {
            return None;
        }
        let facts = index.files().get(&owner.file)?;
        let owner_modules = closed_scope(owner, &facts.source, None)?;
        Some(Self {
            file: owner.file.clone(),
            owner_modules,
        })
    }

    pub(super) fn admits_helper(&self, helper: &FunctionSummary, index: &RustIndex) -> bool {
        if helper.file != self.file {
            return false;
        }
        let Some(facts) = index.files().get(&helper.file) else {
            return false;
        };
        closed_scope(helper, &facts.source, Some(&self.owner_modules)).is_some()
    }
}

/// Module and method facts preserve raw spelling; Rust identity does not.
fn normal_name(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}

fn closed_scope(
    function: &FunctionSummary,
    source: &str,
    safe_super: Option<&[String]>,
) -> Option<Vec<String>> {
    let scope = item_nesting_with_impl(source, &function.name, function.start_line, true)?;
    let parse = parse_clean_source_file(source)?;
    let unread = parse.tree().syntax().descendants().any(|node| {
        let is_use = ast::Use::can_cast(node.kind());
        let is_macro = ast::MacroCall::can_cast(node.kind());
        if !(is_use || ast::ExternCrate::can_cast(node.kind()) || is_macro)
            || !node.parent().is_some_and(|parent| {
                ast::ItemList::can_cast(parent.kind())
                    || ast::SourceFile::can_cast(parent.kind())
                    || is_macro && ast::AssocItemList::can_cast(parent.kind())
            })
        {
            return false;
        }
        let mut modules: Vec<_> = node
            .ancestors()
            .filter_map(ast::Module::cast)
            .filter_map(|module| {
                module
                    .name()
                    .map(|name| normal_name(name.text()).to_string())
            })
            .collect();
        modules.reverse();
        if !scope.modules.starts_with(&modules) {
            return false;
        }
        // The canonical test helper imports the already-closed owner's
        // module directly. Another parent or imported trait remains unread.
        let parent_import = is_use
            && node.text().to_string().trim() == "use super::*;"
            && modules == scope.modules
            && safe_super.is_some_and(|parent| {
                modules.len() == parent.len() + 1 && modules.starts_with(parent)
            });
        !parent_import
    });
    (!unread).then_some(scope.modules)
}
