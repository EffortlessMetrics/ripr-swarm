//! Negative visibility evidence for ordinary direct private-owner relations.
//!
//! This does not resolve calls or grant accessibility. Only clean parser
//! placement in one file, or one resolved default child edge, can veto a
//! direct relation. Unknown contexts retain the existing relation behavior.

use super::CompactGripContext;
use crate::analysis::facts::{FunctionContainer, ModulePathTarget, SourceRoleProvenanceEdgeKind};
use crate::analysis::rust_index::{FunctionSummary, TestSummary};
use ra_ap_syntax::ast::{self, HasName, HasVisibility};
use ra_ap_syntax::{AstNode, SyntaxNode, TextRange};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

#[derive(Clone, PartialEq, Eq)]
struct ModuleScope {
    name: String,
    range: TextRange,
}

#[derive(Clone)]
struct FunctionScope {
    private: bool,
    modules: Vec<ModuleScope>,
}

/// Owned function-layout facts survive parse-window eviction. Keys include
/// the full parser symbol and line; no answer crosses an immutable index.
#[derive(Default)]
pub(super) struct PrivateVisibilityMemo {
    functions: Mutex<BTreeMap<(String, usize), Option<FunctionScope>>>,
    declarations: Mutex<BTreeMap<(PathBuf, usize, String), bool>>,
}

impl PrivateVisibilityMemo {
    pub(super) fn proven_inaccessible(
        &self,
        context: &CompactGripContext<'_>,
        owner: &FunctionSummary,
        test: &TestSummary,
    ) -> bool {
        let Some(owner_scope) = self
            .function_scope(context, owner)
            .filter(|scope| scope.private)
        else {
            return false;
        };
        let Some(observer) =
            context.unique_evidence_function(&test.file, &test.name, test.start_line)
        else {
            return false;
        };
        let Some(observer_scope) = self.function_scope(context, observer) else {
            return false;
        };
        let Some(owner_facts) = context.index.files().get(&owner.file) else {
            return false;
        };
        let Some(observer_facts) = context.index.files().get(&test.file) else {
            return false;
        };
        let owner_origin = &owner_facts.role_provenance;
        let observer_origin = &observer_facts.role_provenance;
        if owner_origin.earliest_unresolved_reason.is_some()
            || observer_origin.earliest_unresolved_reason.is_some()
            || owner_origin.outside_module_tree
            || observer_origin.outside_module_tree
        {
            return false;
        }
        if owner.file == test.file {
            // Keep path/include contexts outside this bounded rule. For a
            // root file, exact AST ranges distinguish even same-named modules.
            return owner_origin.edges.is_empty()
                && !observer_scope.modules.starts_with(&owner_scope.modules);
        }
        // A top-level private function in an ordinary child is inaccessible
        // to every observer in the declaring root. Do not infer that edge
        // from a filename or reconstruct a general module resolver.
        if !owner_scope.modules.is_empty()
            || !observer_origin.edges.is_empty()
            || !context.is_default_unit_test_file(&test.file)
        {
            return false;
        }
        let [edge] = owner_origin.edges.as_slice() else {
            return false;
        };
        edge.kind == SourceRoleProvenanceEdgeKind::Module
            && edge.parent == test.file
            && edge.child == owner.file
            && observer_facts
                .module_declarations
                .iter()
                .any(|declaration| {
                    declaration.line == edge.line
                        && declaration.path_target == ModulePathTarget::Default
                        && edge.declaration == format!("mod {};", declaration.name)
                        && self.ordinary_declaration(
                            context,
                            &edge.parent,
                            declaration.line,
                            &declaration.name,
                        )
                })
    }

    fn ordinary_declaration(
        &self,
        context: &CompactGripContext<'_>,
        file: &std::path::Path,
        line: usize,
        name: &str,
    ) -> bool {
        let key = (file.to_path_buf(), line, name.to_string());
        let mut cache = self
            .declarations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *cache.entry(key).or_insert_with(|| {
            context.parsed_test_file(file).is_some_and(|parsed| {
                parsed
                    .unique_module_declaration(name, line)
                    .is_some_and(|module| ordinary_attributes(module.syntax()))
            })
        })
    }

    fn function_scope(
        &self,
        context: &CompactGripContext<'_>,
        function: &FunctionSummary,
    ) -> Option<FunctionScope> {
        let key = (function.id.0.clone(), function.start_line);
        let mut cache = self
            .functions
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        cache
            .entry(key)
            .or_insert_with(|| parsed_function_scope(context, function))
            .clone()
    }
}

fn parsed_function_scope(
    context: &CompactGripContext<'_>,
    function: &FunctionSummary,
) -> Option<FunctionScope> {
    if function.item.container != FunctionContainer::Free {
        return None;
    }
    let parsed = context.parsed_test_file(&function.file)?;
    let node = parsed.unique_test_fn(&function.name, function.start_line)?;
    let mut modules = Vec::new();
    for ancestor in node.syntax().ancestors() {
        // A proc-macro attribute on any enclosing item can rewrite privacy
        // or placement. Only the ordinary cfg/test attributes are admitted.
        if !ordinary_attributes(&ancestor) {
            return None;
        }
        if let Some(module) = ast::Module::cast(ancestor) {
            module.item_list()?;
            modules.push(ModuleScope {
                name: module.name()?.text().to_string(),
                range: module.syntax().text_range(),
            });
        }
    }
    modules.reverse();
    let mut identity = crate::analysis::stable_path_text(&function.file);
    for module in &modules {
        identity.push_str("::");
        identity.push_str(&module.name);
    }
    identity.push_str("::");
    identity.push_str(&function.name);
    if function.id.0 != identity {
        return None;
    }
    Some(FunctionScope {
        private: node.visibility().is_none(),
        modules,
    })
}

fn ordinary_attributes(node: &SyntaxNode) -> bool {
    node.children()
        .filter_map(ast::Attr::cast)
        .all(|attribute| {
            attribute
                .meta()
                .and_then(|meta| meta.simple_name())
                .is_some_and(|name| matches!(name.as_str(), "cfg" | "test"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::build_index;
    use crate::analysis::test_grip_evidence::tests::claim_index_fixture_root;
    use std::fs;

    #[test]
    fn module_extraction_visibility_requires_ordinary_module_placement() -> Result<(), String> {
        for (declaration, owner_attribute, expected) in [
            ("mod child;", "", true),
            ("#[path = \"child.rs\"] mod child;", "", false),
            ("#[rewrite] mod child;", "", false),
            ("mod child;", "#[rewrite]\n", false),
            ("#![rewrite]\nmod child;", "", false),
        ] {
            let root = claim_index_fixture_root(4680)?;
            fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
            fs::write(
                root.join("Cargo.toml"),
                "[package]\nname = \"private-placement-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            )
            .map_err(|error| error.to_string())?;
            fs::write(
                root.join("src/lib.rs"),
                format!("{declaration}\n#[cfg(test)] mod tests {{\n#[test]\nfn exact_value() {{ assert_eq!(super::child::compute(), 9); }}\n}}\n"),
            )
            .map_err(|error| error.to_string())?;
            fs::write(
                root.join("src/child.rs"),
                format!("{owner_attribute}fn compute() -> i32 {{ 9 }}\n"),
            )
            .map_err(|error| error.to_string())?;
            let index = build_index(
                &root,
                &[PathBuf::from("src/lib.rs"), PathBuf::from("src/child.rs")],
            )?;
            assert_eq!(index.tests().len(), 1);
            let owner = index
                .functions()
                .iter()
                .find(|function| function.id.0 == "src/child.rs::compute")
                .ok_or("indexed owner")?;
            let context = CompactGripContext::new(&index);
            // This checks the new negative authority itself. Refusing a
            // placement claim does not assert that the call is accessible or
            // endorse the pre-existing relation fallback in unknown contexts.
            assert_eq!(
                PrivateVisibilityMemo::default().proven_inaccessible(
                    &context,
                    owner,
                    &index.tests()[0],
                ),
                expected,
                "{declaration} / {owner_attribute}"
            );
        }
        Ok(())
    }
}
