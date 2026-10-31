//! Declared ordinary Kani source inventory (#10686). This phase owns source
//! roles, not verifier results, Cargo test registration or oracle credit.

use super::{FunctionSourceRole, RustIndex, cfg_predicates, source_digest};
use crate::analysis::syntax::ra::LineIndex;
use crate::analysis::syntax::{name_scopes_for_fn, parse_clean_source_file};
use crate::config::KaniAnalysisContext;
use crate::domain::SymbolId;
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasAttrs, HasGenericParams, HasName},
};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KaniApplicability {
    Enabled,
    Disabled,
    Unknown,
}

/// One source declaration. Its name is not a validated native selector.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct KaniHarnessSource {
    pub symbol: SymbolId,
    pub file: PathBuf,
    pub function_name: String,
    pub module_path: Vec<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub applicability: KaniApplicability,
    /// Source-preserving attributes from the file, enclosing inline modules
    /// and function. No cfg expansion or host environment is inferred.
    pub attributes: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct KaniSourceLimitation {
    pub file: PathBuf,
    pub line: Option<usize>,
    pub code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct KaniSourceInventory {
    pub context: KaniAnalysisContext,
    /// SHA-256 of the indexed UTF-8 source (the ordinary parser strips BOM).
    pub indexed_source_digest: Option<String>,
    pub harnesses: Vec<KaniHarnessSource>,
    pub limitations: Vec<KaniSourceLimitation>,
    pub native_selection: &'static str,
    pub native_execution: &'static str,
    pub functional_protection: &'static str,
}

impl KaniSourceInventory {
    fn new(context: &KaniAnalysisContext) -> Self {
        Self {
            context: context.clone(),
            indexed_source_digest: None,
            harnesses: Vec::new(),
            limitations: Vec::new(),
            native_selection: "unknown",
            native_execution: "not_observed",
            functional_protection: "not_established",
        }
    }

    fn limit(&mut self, line: Option<usize>, code: &str) {
        self.limitations.push(KaniSourceLimitation {
            file: self.context.source.clone(),
            line,
            code: code.into(),
        });
    }
}

/// Runs after ordinary role authorities and before helper credit. Every
/// generation is reconstructed from raw facts, including context-only edits.
pub(super) fn apply_context(
    index: &mut RustIndex,
    context: Option<&KaniAnalysisContext>,
) -> Result<(), String> {
    let Some(context) = context else {
        index.kani_source_inventory = None;
        return Ok(());
    };
    let mut inventory = KaniSourceInventory::new(context);
    let normalized = context.normalized_source();
    if normalized.as_ref().ok() != Some(&context.source) {
        inventory.limit(None, "invalid_native_context");
        index.kani_source_inventory = Some(inventory);
        return Ok(());
    }
    let roles = collect(index, &mut inventory)?;
    // All role updates bind the parser's full identity and line span, not a
    // short function name. Both flat and containing-file memberships agree.
    let updates: Vec<_> = index
        .live_function_ids()
        .into_iter()
        .filter_map(|id| {
            let function = &index.function_facts[id];
            roles
                .iter()
                .find(|(key, _)| key.matches(function))
                .map(|(_, role)| (id, *role))
        })
        .collect();
    index.apply_function_roles(updates.clone(), updates);
    let keep = |test: &super::TestFact| {
        !roles.iter().any(|(key, _)| {
            key.file == test.file
                && key.name == test.name
                && key.start == test.start_line
                && key.end == test.end_line
        })
    };
    index.test_order.retain(|id| keep(&index.test_facts[*id]));
    for file in index.files.values_mut() {
        file.tests.retain(|id| keep(&index.test_facts[*id]));
    }
    index.harness_subjects.retain(|subject| {
        !roles.iter().any(|(key, _)| {
            key.file == subject.file
                && key.name == subject.name
                && key.start == subject.start_line
                && key.end == subject.end_line
        })
    });
    index.kani_source_inventory = Some(inventory);
    index.refresh_memberships()
}

struct FunctionKey {
    symbol: SymbolId,
    file: PathBuf,
    name: String,
    start: usize,
    end: usize,
}

impl FunctionKey {
    fn matches(&self, function: &super::FunctionFact) -> bool {
        self.symbol == function.id
            && self.file == function.file
            && self.name == function.name
            && self.start == function.start_line
            && self.end == function.end_line
    }
}

fn function_key(
    function: &ast::Fn,
    lines: &LineIndex,
    index: &RustIndex,
    file: &std::path::Path,
) -> Option<FunctionKey> {
    let name = function.name()?.text().to_string();
    let start = lines.line(function.fn_token()?.text_range().start());
    let end = lines.line_for_range_end(function.syntax().text_range().end());
    let mut candidates = index.functions().iter().filter(|fact| {
        fact.file == file && fact.name == name && fact.start_line == start && fact.end_line == end
    });
    let fact = candidates.next()?;
    if candidates.next().is_some() {
        return None;
    }
    Some(FunctionKey {
        symbol: fact.id.clone(),
        file: file.to_path_buf(),
        name,
        start,
        end,
    })
}

fn collect(
    index: &RustIndex,
    inventory: &mut KaniSourceInventory,
) -> Result<Vec<(FunctionKey, FunctionSourceRole)>, String> {
    let path = inventory.context.source.clone();
    let Some(file) = index.files().get(&path) else {
        inventory.limit(None, "configured_source_not_indexed");
        return Ok(Vec::new());
    };
    if file.used_lexical_fallback || index.non_utf8_sources.contains(&path) {
        inventory.limit(None, "clean_utf8_syntax_required");
        return Ok(Vec::new());
    }
    let source = file.source.as_ref();
    inventory.indexed_source_digest = Some(source_digest(source.as_bytes()));
    let Some(parse) = parse_clean_source_file(source) else {
        inventory.limit(None, "clean_utf8_syntax_required");
        return Ok(Vec::new());
    };
    // Included fragments have another compilation-unit identity and can be
    // selected through more than one parent. This first profile never guesses.
    if index.include_parents.contains_key(&path) || index.include_targets.contains(&path) {
        inventory.limit(None, "included_source_unit_unsupported");
        return Ok(Vec::new());
    }
    let tree = parse.tree();
    let lines = LineIndex::new(source);
    for module in tree.syntax().descendants().filter_map(ast::Module::cast) {
        if module.item_list().is_none() {
            inventory.limit(
                Some(lines.line(module.syntax().text_range().start())),
                "out_of_line_module_not_expanded",
            );
        }
    }
    let mut roles = Vec::new();
    for function in tree.syntax().descendants().filter_map(ast::Fn::cast) {
        crate::analysis::cancellation::checkpoint()?;
        let own_attrs: Vec<String> = function
            .attrs()
            .map(|attr| attr.syntax().text().to_string())
            .collect();
        let line = function
            .fn_token()
            .map(|token| lines.line(token.text_range().start()));
        if !own_attrs
            .iter()
            .any(|attr| compact(attr) == "#[kani::proof]")
        {
            if own_attrs.iter().any(|attr| {
                attr.contains("kani") && (attr.contains("proof") || attr.contains("cfg_attr"))
            }) {
                inventory.limit(line, "noncanonical_or_conditional_proof_unsupported");
            }
            continue;
        }
        if function.syntax().ancestors().skip(1).any(|node| {
            !matches!(
                node.kind(),
                ra_ap_syntax::SyntaxKind::SOURCE_FILE
                    | ra_ap_syntax::SyntaxKind::ITEM_LIST
                    | ra_ap_syntax::SyntaxKind::MODULE
            )
        }) || function.generic_param_list().is_some()
            || function.async_token().is_some()
            || function.param_list().is_none_or(|parameters| {
                parameters.params().next().is_some() || parameters.self_param().is_some()
            })
            || function.ret_type().is_some()
            || function.body().is_none()
        {
            inventory.limit(line, "ordinary_free_unit_proof_required");
            continue;
        }
        if function.body().is_some_and(|body| {
            body.syntax()
                .descendants()
                .filter_map(ast::Attr::cast)
                .any(|attr| compact(&attr.syntax().text().to_string()).starts_with("#!["))
        }) {
            inventory.limit(line, "function_inner_attributes_unsupported");
            continue;
        }
        let Some(key) = function_key(&function, &lines, index, &path) else {
            inventory.limit(line, "proof_source_identity_ambiguous");
            continue;
        };
        let Some(scopes) = name_scopes_for_fn(source, key.start, &key.name) else {
            inventory.limit(line, "provider_binding_unknown");
            continue;
        };
        // The function's own body does not bind its outer attribute. Module
        // globs, local kani and opaque item expansions can, so refuse them.
        let rebound = scopes
            .scopes
            .iter()
            .filter(|scope| !scope.fn_body)
            .any(|scope| {
                !scope.globs.is_empty()
                    || scope
                        .imports
                        .iter()
                        .any(|import| import.bound.trim_start_matches("r#") == "kani")
                    || scope
                        .defines
                        .iter()
                        .chain(&scope.child_modules)
                        .any(|name| name.trim_start_matches("r#") == "kani")
            });
        let ancestors: Vec<_> = function.syntax().ancestors().skip(1).collect();
        if rebound {
            inventory.limit(line, "provider_binding_unknown");
            continue;
        }
        let mut attributes: Vec<String> = tree
            .attrs()
            .map(|attr| attr.syntax().text().to_string())
            .collect();
        let mut modules: Vec<_> = ancestors
            .iter()
            .filter_map(|node| ast::Module::cast(node.clone()))
            .collect();
        modules.reverse();
        let module_path = modules
            .iter()
            .filter_map(|module| module.name().map(|name| name.text().to_string()))
            .collect();
        for module in &modules {
            attributes.extend(module.attrs().map(|attr| attr.syntax().text().to_string()));
            if let Some(items) = module.item_list() {
                attributes.extend(
                    items
                        .syntax()
                        .children()
                        .filter_map(ast::Attr::cast)
                        .map(|attr| attr.syntax().text().to_string()),
                );
            }
        }
        attributes.extend(own_attrs);
        if attributes
            .iter()
            .any(|attr| !source_preserving_attribute(attr))
        {
            inventory.limit(line, "opaque_attribute_context");
            continue;
        }
        let mut limitations = Vec::new();
        if attributes.iter().any(|attr| {
            let text = compact(attr);
            text.starts_with("#[cfg_attr(") || text.starts_with("#![cfg_attr(")
        }) {
            // Conditional attributes can introduce proof/transforming macros.
            // Keep this form outside the closed source-role admission.
            inventory.limit(line, "conditional_attribute_context_unsupported");
            continue;
        }
        if ancestors.iter().any(opaque_scope) {
            inventory.limit(line, "provider_binding_unknown");
            continue;
        }
        if attributes.iter().any(|attr| {
            let text = compact(attr);
            text.starts_with("#[kani::") && text != "#[kani::proof]"
                || text.starts_with("#[should_panic")
        }) {
            limitations.push("native_modifier_semantics_unmodeled".into());
        }
        if index.tests().iter().any(|test| {
            test.file == key.file && test.start_line == key.start && test.name == key.name
        }) {
            limitations.push("ordinary_test_role_withheld".into());
        }
        let values: Vec<_> = attributes
            .iter()
            .map(|attr| cfg_predicates::attribute_kani_availability(attr, inventory.context.tests))
            .collect();
        let applicability = if values.contains(&Some(false)) {
            KaniApplicability::Disabled
        } else if values.iter().all(|value| *value == Some(true)) {
            KaniApplicability::Enabled
        } else {
            KaniApplicability::Unknown
        };
        if applicability == KaniApplicability::Unknown {
            limitations.push("native_cfg_applicability_unknown".into());
        }
        let nested = function
            .syntax()
            .descendants()
            .skip(1)
            .filter_map(ast::Fn::cast)
            .map(|function| function_key(&function, &lines, index, &path))
            .collect::<Option<Vec<_>>>();
        let Some(nested) = nested else {
            inventory.limit(line, "proof_nested_source_identity_ambiguous");
            continue;
        };
        inventory.harnesses.push(KaniHarnessSource {
            symbol: key.symbol.clone(),
            file: key.file.clone(),
            function_name: key.name.clone(),
            module_path,
            start_line: key.start,
            end_line: key.end,
            applicability,
            attributes,
            limitations,
        });
        roles.push((key, FunctionSourceRole::KaniProofHarness));
        for key in nested {
            roles.push((key, FunctionSourceRole::KaniProofLocal));
        }
    }
    Ok(roles)
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn source_preserving_attribute(text: &str) -> bool {
    let text = compact(text);
    let body = text.strip_prefix("#[").or_else(|| text.strip_prefix("#!["));
    body.is_some_and(|body| {
        matches!(
            body,
            "test]" | "should_panic]" | "kani::proof]" | "kani::should_panic]"
        ) || [
            "cfg(",
            "cfg_attr(",
            "allow(",
            "warn(",
            "deny(",
            "forbid(",
            "doc=",
            "doc(",
            "should_panic(",
            "kani::unwind(",
            "kani::stub(",
            "kani::stub_verified(",
            "kani::proof_for_contract(",
            "kani::requires(",
            "kani::ensures(",
        ]
        .iter()
        .any(|prefix| body.starts_with(prefix))
    })
}

fn opaque_scope(node: &SyntaxNode) -> bool {
    use ra_ap_syntax::SyntaxKind as K;
    matches!(node.kind(), K::SOURCE_FILE | K::ITEM_LIST)
        && node.children().any(|child| {
            ast::MacroCall::can_cast(child.kind())
                || ast::MacroRules::can_cast(child.kind())
                || ast::MacroDef::can_cast(child.kind())
                || child.descendants().filter_map(ast::Attr::cast).any(|attr| {
                    let text = attr.syntax().text().to_string();
                    let compact = compact(&text);
                    let scope_attribute = attr.syntax().parent().as_ref() == Some(&child)
                        || compact.starts_with("#![");
                    scope_attribute
                        && (!source_preserving_attribute(&text)
                            || compact.starts_with("#[cfg_attr(")
                            || compact.starts_with("#![cfg_attr("))
                })
        })
}

#[cfg(test)]
mod tests;
