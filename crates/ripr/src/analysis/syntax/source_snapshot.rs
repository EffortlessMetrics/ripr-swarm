//! Run-local complete-source authority. Keep immutable parser storage (green
//! nodes) shared; each consumer creates its own thread-local red syntax view.
//! Unknown parses and ambiguous declarations are cached as unknown, never sync.

use super::{NameScopes, name_scope::name_scopes_in_tree, parse_clean_source_file, ra::LineIndex};
use crate::analysis::rust_index::{FunctionSummary, RustIndex, TestSummary};
use ra_ap_syntax::{
    AstNode, Parse, SourceFile,
    ast::{self, HasName},
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

type SnapshotSlot = (Arc<str>, Arc<OnceLock<Arc<SourceSnapshot>>>);
// The containing snapshot supplies exact file/source identity; retain symbol,
// name and both coordinates so equal names/lines never alias query answers.
type FunctionKey = (String, String, usize, usize);
type CallerKey = (String, usize, usize);

#[derive(Default)]
pub(crate) struct SourceSyntaxMemos {
    files: Mutex<BTreeMap<PathBuf, SnapshotSlot>>,
}

impl Clone for SourceSyntaxMemos {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl std::fmt::Debug for SourceSyntaxMemos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceSyntaxMemos").finish_non_exhaustive()
    }
}

impl SourceSyntaxMemos {
    pub(crate) fn get(&self, index: &RustIndex, file: &Path) -> Option<Arc<SourceSnapshot>> {
        let facts = index.files().get(file)?;
        let source = &facts.data().source;
        let slot = self.files.lock().ok().map(|mut files| {
            let entry = files
                .entry(file.to_path_buf())
                .or_insert_with(|| (Arc::clone(source), Arc::default()));
            // Retain the source Arc, so an allocation address cannot be reused
            // under this entry. Replacement (including test-only file edits)
            // receives a fresh slot even when path/coordinates are unchanged.
            if !Arc::ptr_eq(&entry.0, source) {
                *entry = (Arc::clone(source), Arc::default());
            }
            Arc::clone(&entry.1)
        });
        Some(match slot {
            Some(slot) => Arc::clone(slot.get_or_init(|| Arc::new(SourceSnapshot::new(source)))),
            None => Arc::new(SourceSnapshot::new(source)),
        })
    }
}

pub(crate) struct SourceSnapshot {
    parse: Option<Parse<SourceFile>>,
    lines: LineIndex,
    declarations: Mutex<BTreeMap<FunctionKey, Option<bool>>>,
    scopes: Mutex<BTreeMap<CallerKey, Option<Arc<NameScopes>>>>,
    nesting: Mutex<BTreeMap<(String, usize, bool), Option<ItemNesting>>>,
}

impl SourceSnapshot {
    pub(crate) fn tree(&self) -> Option<SourceFile> {
        self.parse.as_ref().map(Parse::tree)
    }

    fn new(source: &str) -> Self {
        Self {
            parse: parse_clean_source_file(source),
            lines: LineIndex::new(source),
            declarations: Mutex::default(),
            scopes: Mutex::default(),
            nesting: Mutex::default(),
        }
    }

    pub(crate) fn declaration_is_async(&self, function: &FunctionSummary) -> Option<bool> {
        let key = (
            function.id.0.clone(),
            function.name.clone(),
            function.start_line,
            function.end_line,
        );
        let compute = || {
            let tree = self.parse.as_ref()?.tree();
            let mut matches = tree
                .syntax()
                .descendants()
                .filter_map(ast::Fn::cast)
                .filter(|item| {
                    item.name()
                        .is_some_and(|name| name.text() == function.name.as_str())
                        && item.fn_token().is_some_and(|token| {
                            (self.lines.line(item.syntax().text_range().start())
                                ..=self.lines.line(token.text_range().start()))
                                .contains(&function.start_line)
                        })
                });
            match (matches.next(), matches.next()) {
                (Some(item), None) => Some(item.async_token().is_some()),
                _ => None,
            }
        };
        match self.declarations.lock() {
            Ok(mut memo) => *memo.entry(key).or_insert_with(compute),
            Err(_) => compute(),
        }
    }

    pub(crate) fn name_scopes(&self, caller: &TestSummary) -> Option<Arc<NameScopes>> {
        let key = (caller.name.clone(), caller.start_line, caller.end_line);
        let compute = || {
            name_scopes_in_tree(
                &self.parse.as_ref()?.tree(),
                &self.lines,
                caller.start_line,
                &caller.name,
            )
            .map(Arc::new)
        };
        match self.scopes.lock() {
            Ok(mut memo) => memo.entry(key).or_insert_with(compute).clone(),
            Err(_) => compute(),
        }
    }
}

/// Where a `fn` sits among its file's inline modules (#6974).
#[derive(Clone, Debug)]
pub(crate) struct ItemNesting {
    /// The inline modules enclosing the `fn`, outermost first.
    pub(crate) modules: Vec<String>,
    /// Whether the `fn` or an enclosing inline module carries a `cfg` or
    /// `cfg_attr` attribute. A complementary cfg may then compile a
    /// same-named `static`, `const`, `use` or module in its place, so a path
    /// to the module no longer has to reach the `fn` (#7061 review).
    pub(crate) cfg_gated: bool,
}

impl SourceSnapshot {
    pub(crate) fn item_nesting(
        &self,
        name: &str,
        line: usize,
        allow_impl: bool,
    ) -> Option<ItemNesting> {
        let key = (name.to_string(), line, allow_impl);
        match self.nesting.lock() {
            Ok(mut memo) => memo
                .entry(key)
                .or_insert_with(|| self.compute_item_nesting(name, line, allow_impl))
                .clone(),
            Err(_) => self.compute_item_nesting(name, line, allow_impl),
        }
    }

    fn compute_item_nesting(
        &self,
        name: &str,
        line: usize,
        allow_impl: bool,
    ) -> Option<ItemNesting> {
        let base = |text: String| text.strip_prefix("r#").map_or(text.clone(), str::to_string);
        let wanted = name.strip_prefix("r#").unwrap_or(name);
        let mut found = self
            .parse
            .as_ref()?
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .filter(|function| {
                function
                    .name()
                    .is_some_and(|ident| base(ident.text().to_string()) == wanted)
                    && {
                        let range = function.syntax().text_range();
                        let start = self.lines.line(range.start());
                        let end = self.lines.line(range.end());
                        start <= line && line <= end
                    }
            });
        let function = found.next()?;
        if found.next().is_some() {
            return None;
        }
        let mut modules = Vec::new();
        // An inner `#![cfg]` in the body gates the whole fn too.
        let mut cfg_gated = has_cfg_attr(&function)
            || function
                .body()
                .and_then(|body| body.stmt_list())
                .is_some_and(|statements| has_cfg_attr(&statements));
        for ancestor in function.syntax().ancestors().skip(1) {
            if let Some(module) = ast::Module::cast(ancestor.clone()) {
                cfg_gated |= has_cfg_attr(&module)
                    || module.item_list().is_some_and(|items| has_cfg_attr(&items));
                modules.push(base(module.name()?.text().to_string()));
            } else if allow_impl && let Some(implementation) = ast::Impl::cast(ancestor.clone()) {
                cfg_gated |= has_cfg_attr(&implementation);
            } else if !(ast::ItemList::can_cast(ancestor.kind())
                || ast::SourceFile::can_cast(ancestor.kind())
                || allow_impl && ast::AssocItemList::can_cast(ancestor.kind()))
            {
                return None;
            }
        }
        modules.reverse();
        Some(ItemNesting { modules, cfg_gated })
    }
}

/// Whether `node` carries a `cfg` or `cfg_attr` attribute, outer or inner.
fn has_cfg_attr(node: &impl ast::HasAttrs) -> bool {
    // `cfg` and `cfg_attr` parse to their own metas, which have no `path`.
    node.attrs().any(|attr| {
        matches!(
            attr.meta(),
            Some(ast::Meta::CfgMeta(_) | ast::Meta::CfgAttrMeta(_))
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::rust_index::summarize_file;

    const FILE: &str = "src/lib.rs";

    fn index(source: &str) -> RustIndex {
        let mut index = RustIndex::default();
        index.insert_file(
            PathBuf::from(FILE),
            summarize_file(PathBuf::from(FILE), source.into()),
            true,
        );
        index
    }

    fn snapshot(index: &RustIndex) -> Result<Arc<SourceSnapshot>, String> {
        index
            .source_syntax
            .get(index, Path::new(FILE))
            .ok_or_else(|| "indexed source missing".to_string())
    }

    #[test]
    fn source_snapshot_reuses_parse_and_scopes_across_threads_and_owners() -> Result<(), String> {
        let index =
            index("fn first() {} fn second() {}\n#[test] fn subject() { first(); second(); }");
        let snapshot = snapshot(&index)?;
        let caller = index.tests().iter().next().ok_or("test")?;
        let scopes = snapshot.name_scopes(caller).ok_or("scopes")?;
        std::thread::scope(|threads| -> Result<(), String> {
            let mut handles = Vec::new();
            for _ in 0..8 {
                let index = &index;
                let snapshot = &snapshot;
                let scopes = &scopes;
                handles.push(threads.spawn(move || -> Result<(), String> {
                    let shared = index
                        .source_syntax
                        .get(index, Path::new(FILE))
                        .ok_or("snapshot")?;
                    assert!(Arc::ptr_eq(snapshot, &shared));
                    assert!(Arc::ptr_eq(
                        scopes,
                        &shared.name_scopes(caller).ok_or("scopes")?
                    ));
                    for owner in index.functions().iter() {
                        assert_eq!(shared.declaration_is_async(owner), Some(false));
                    }
                    Ok(())
                }));
            }
            for handle in handles {
                handle
                    .join()
                    .map_err(|payload| format!("snapshot worker failed: {payload:?}"))??;
            }
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn source_snapshot_preserves_qualifiers_ambiguity_and_complete_query_identity()
    -> Result<(), String> {
        let index = index(
            "pub(crate)\nasync\nfn first() {} fn second() {}\nmod a { fn same() {} } mod b { async fn same() {} }",
        );
        let snapshot = snapshot(&index)?;
        let first = index
            .functions()
            .iter()
            .find(|f| f.name == "first")
            .ok_or("first")?;
        assert_eq!(snapshot.declaration_is_async(first), Some(true));
        let second = index
            .functions()
            .iter()
            .find(|f| f.name == "second")
            .ok_or("second")?;
        assert_eq!(snapshot.declaration_is_async(second), Some(false));
        // Even an artificial symbol-id collision must not borrow another
        // name/coordinate's answer. Same-line same-name ambiguity stays unknown.
        let mut collision = second.clone();
        collision.id = first.id.clone();
        assert_eq!(snapshot.declaration_is_async(&collision), Some(false));
        for function in index.functions().iter().filter(|f| f.name == "same") {
            assert_eq!(snapshot.declaration_is_async(function), None);
        }
        let mut missing = first.clone();
        missing.start_line = 100;
        assert_eq!(snapshot.declaration_is_async(&missing), None);
        Ok(())
    }

    #[test]
    fn source_snapshot_keeps_same_line_lexical_ambiguity_and_scope_refusals() -> Result<(), String>
    {
        let index = index(
            "mod a { #[test] fn same() {} } mod b { #[test] fn same() {} }\n#[cfg(test)] mod gated { fn helper() {} }\nfn outer() { fn nested() {} }\n",
        );
        let snapshot = snapshot(&index)?;
        assert_eq!(index.tests().len(), 2);
        for caller in index.tests().iter() {
            assert!(snapshot.name_scopes(caller).is_none());
        }
        let nested = snapshot
            .item_nesting("helper", 2, false)
            .ok_or("module helper")?;
        assert_eq!(nested.modules, ["gated"]);
        assert!(nested.cfg_gated);
        assert!(snapshot.item_nesting("nested", 3, false).is_none());
        assert!(snapshot.item_nesting("same", 1, false).is_none());
        Ok(())
    }

    #[test]
    fn source_snapshot_replacement_clone_and_removal_cannot_reuse_stale_facts() -> Result<(), String>
    {
        let mut original = index("fn helper() {}\n#[test] fn subject() {}\n");
        let old = snapshot(&original)?;
        let helper = original
            .functions()
            .iter()
            .find(|f| f.name == "helper")
            .ok_or("helper")?
            .clone();
        assert_eq!(old.declaration_is_async(&helper), Some(false));
        let mut cloned = original.clone();
        assert!(!Arc::ptr_eq(&old, &snapshot(&cloned)?));
        cloned.insert_file(
            PathBuf::from(FILE),
            summarize_file(
                PathBuf::from(FILE),
                "async fn helper() {}\n#[test] fn subject() {}\n".into(),
            ),
            true,
        );
        assert_eq!(snapshot(&cloned)?.declaration_is_async(&helper), Some(true));
        assert_eq!(
            snapshot(&original)?.declaration_is_async(&helper),
            Some(false)
        );
        // This test-only edit bypasses insert_file and keeps every coordinate.
        original
            .file_data_mut(Path::new(FILE))
            .ok_or("file")?
            .source = "fn broken( {".into();
        let unknown = snapshot(&original)?;
        assert_eq!(unknown.declaration_is_async(&helper), None);
        assert!(unknown.parse.is_none());
        assert!(Arc::ptr_eq(&unknown, &snapshot(&original)?));
        original.remove_file(Path::new(FILE));
        assert!(
            original
                .source_syntax
                .get(&original, Path::new(FILE))
                .is_none()
        );
        Ok(())
    }
}
