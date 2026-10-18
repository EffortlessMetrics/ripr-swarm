from pathlib import Path
p=Path('/workspace/5500-assertion-source');s=p/'crates/ripr/src/analysis/syntax/owner_pin.rs';t=s.read_text()
start=t.index('pub(crate) fn trusted_macro_binding_sites(');end=t.index('\n/// Apply the same binding',start);old=t[start:end]
new=old.replace('    macro_binding_ambiguities(\n','    trusted_macro_binding_sites_with_local_modules(\n',1).replace('        &BTreeSet::new(),\n','',1).replace('        drop_in_verified,\n','        drop_in_verified,\n        false,\n',1)
new+='''
/// Locality removes only a foreign-glob wrapper. The caller must first prove
/// the complete indexed export namespace source-visible; every visible macro
/// definition, import and opaque call still goes through the ordinary scan.
pub(crate) fn trusted_macro_binding_sites_with_local_modules(
    source: &str,
    packages: &BTreeSet<String>,
    trusted: &[&str],
    module_resolved: &dyn Fn(usize, &str) -> bool,
    drop_in_verified: &dyn Fn(&str) -> bool,
    local_modules_visible: bool,
) -> Vec<(String, MacroBindingSite)> {
    macro_binding_ambiguities(source, packages, trusted, &BTreeSet::new(),
        module_resolved, drop_in_verified, local_modules_visible)
}

/// Existing composition and attribute authorities bound the namespace, rather
/// than guessing what an opaque expansion exports. A body macro may export a
/// macro to crate root, so the full module-tree scan must be complete.
pub(in crate::analysis) fn module_export_source_is_visible(
    source: &str,
    module_resolved: &dyn Fn(usize, &str) -> bool,
) -> bool {
    if !super::rust_module_tree_scan(source).complete { return false; }
    let Some(parse) = parse_clean_source_file(source) else { return false; };
    parse.tree().syntax().descendants().all(|node| {
        if let Some(attr) = ast::Attr::cast(node.clone()) {
            return crate::analysis::facts::cfg_predicates::attribute_preserves_source_items(
                &attr.syntax().text().to_string());
        }
        if let Some(module) = ast::Module::cast(node) {
            if module.item_list().is_some() { return true; }
            return module.mod_token().zip(module.name()).is_some_and(|(token, name)| {
                let offset: usize = token.text_range().start().into();
                let line = source[..offset].bytes().filter(|byte| *byte == b'\\n').count() + 1;
                module_resolved(line, &format!("mod {};", name.text()))
            });
        }
        true
    })
}

fn resolved_direct_local_glob(
    import: &ast::Use,
    root: &str,
    source: &str,
    module_resolved: &dyn Fn(usize, &str) -> bool,
) -> bool {
    // The initial slice covers only an unconditional direct file-root module
    // and a single-component, relative import in that same exact scope.
    let Some(file) = import.syntax().parent().and_then(ast::SourceFile::cast) else { return false; };
    if import.attrs().next().is_some() || file.syntax().children().any(|node| ast::Attr::can_cast(node.kind()))
        || import.use_tree().and_then(|tree| tree.path())
            .is_none_or(|path| path.syntax().text().to_string() != root)
        || import.use_tree().is_none_or(|tree| tree.star_token().is_none() || tree.use_tree_list().is_some())
        || module_use_leaves(file.syntax()).iter()
            .any(|leaf| matches!(&leaf.binding, UseBinding::Name(name) if unraw(name) == unraw(root)))
    { return false; }
    let modules = file.syntax().children().filter_map(ast::Module::cast)
        .filter(|module| module.name().is_some_and(|name| unraw(name.text()) == unraw(root)))
        .collect::<Vec<_>>();
    let [module] = modules.as_slice() else { return false; };
    if module.attrs().next().is_some() { return false; }
    module.item_list().is_some() || module.mod_token().zip(module.name()).is_some_and(|(token, name)| {
        let offset: usize = token.text_range().start().into();
        let line = source[..offset].bytes().filter(|byte| *byte == b'\\n').count() + 1;
        module_resolved(line, &format!("mod {};", name.text()))
    })
}
'''
t=t[:start]+new+t[end:]
# All existing standalone and withheld routes retain default refusal.
t=t.replace('        &|_| false,\n    )\n    .into_iter()', '        &|_| false,\n        false,\n    )\n    .into_iter()')
t=t.replace('        drop_in_verified,\n    )\n    .into_iter()', '        drop_in_verified,\n        false,\n    )\n    .into_iter()')
start=t.index('fn macro_binding_ambiguities('); end=t.index('\n) ->',start)
t=t[:end]+'\n    local_modules_visible: bool,'+t[end:]
needle="                    .any(|package| package.replace('-', \"_\") == root);";assert needle in t
t=t.replace(needle,"                    .any(|package| package.replace('-', \"_\") == root)\n                || (!external && local_modules_visible\n                    && resolved_direct_local_glob(&import, root, source, module_resolved));",1);s.write_text(t)
s=p/'crates/ripr/src/analysis/syntax/mod.rs';t=s.read_text().replace('pub(in crate::analysis) use owner_pin::primitive_namespace_is_closed;', 'pub(in crate::analysis) use owner_pin::{primitive_namespace_is_closed, module_export_source_is_visible};').replace('supported_targeted_item_context, trusted_macro_binding_sites,','supported_targeted_item_context, trusted_macro_binding_sites, trusted_macro_binding_sites_with_local_modules,');s.write_text(t)
s=p/'crates/ripr/src/analysis/classify/owner_pin.rs';t=s.read_text().replace('source_declares_value_name, trusted_macro_binding_sites,','source_declares_value_name, trusted_macro_binding_sites, trusted_macro_binding_sites_with_local_modules,')
t=t.replace('    ambiguous_macro_bindings: RefCell<Option<BTreeSet<String>>>,','    ambiguous_macro_bindings: RefCell<Option<BTreeSet<String>>>,\n    local_glob_namespace_closed: OnceCell<bool>,')
t=t.replace('pub(in crate::analysis) struct WithheldMacroBindings {','pub(in crate::analysis) struct WithheldMacroBindings {\n    present: bool,')
t=t.replace('        if self.any_name {\n            return true;','        self.present = true;\n        if self.any_name {\n            return true;',1)
marker='    fn trusted_macro_ambiguities(&self, index: &RustIndex) -> BTreeSet<String> {'
method='''    fn local_glob_namespace_is_closed(&self, index: &RustIndex) -> bool {
        *self.local_glob_namespace_closed.get_or_init(|| {
            if self.withheld.present || !index.non_utf8_sources.is_empty()
                || !index.include_limitations.is_empty() || !index.include_targets.is_empty()
            { return false; }
            let resolved = self.resolved_module_declarations(index);
            index.files().iter().all(|(path, facts)| {
                !facts.used_lexical_fallback
                    && facts.role_provenance.earliest_unresolved_reason.is_none()
                    && crate::analysis::syntax::module_export_source_is_visible(&facts.source,
                        &|line, declaration| resolved.contains(&(path.clone(), line, declaration.to_string())))
            })
        })
    }

''';assert marker in t;t=t.replace(marker,method+marker,1)
t=t.replace('trusted_macro_sites_in(index, self.target_roots(index), &module_resolved)', 'trusted_macro_sites_in(index, self.target_roots(index), &module_resolved, self.local_glob_namespace_is_closed(index))')
t=t.replace('scan_macro_sites(&rest, index, &module_resolved)', 'scan_macro_sites(&rest, index, &module_resolved, self.local_glob_namespace_is_closed(index))')
t=t.replace('                    &resolved,\n                );','                    &resolved,\n                    self.local_glob_namespace_is_closed(index),\n                );',1)
# Add capability to the shared scan/disclosure chain.
start=t.index('fn test_macro_binding_site('); end=t.index('\n) ->',start)
t=t[:end]+'\n    local_modules_visible: bool,'+t[end:]
t=t.replace('                module_resolved,\n            )','                module_resolved,\n                local_modules_visible,\n            )',1)
t=t.replace('    module_resolved: &dyn Fn(&Path, usize, &str) -> bool,\n) -> Vec<(String, MacroBindingSite)> {','    module_resolved: &dyn Fn(&Path, usize, &str) -> bool,\n    local_modules_visible: bool,\n) -> Vec<(String, MacroBindingSite)> {',1)
start=t.index('fn macro_binding_sites(');end=t.index('\n/// Trusted macro names',start);frag=t[start:end].replace('    trusted_macro_binding_sites(','    trusted_macro_binding_sites_with_local_modules(').replace('        },\n    )','        },\n        local_modules_visible,\n    )');t=t[:start]+frag+t[end:]
for name in ['trusted_macro_sites_in','scan_macro_sites']:
 start=t.index('fn '+name+'(');sigend=t.index(') ->',start);t=t[:sigend]+ '    local_modules_visible: bool,\n'+t[sigend:]
t=t.replace('scan_macro_sites(&likely, index, module_resolved)','scan_macro_sites(&likely, index, module_resolved, local_modules_visible)').replace('scan_macro_sites(&rest, index, module_resolved)','scan_macro_sites(&rest, index, module_resolved, local_modules_visible)')
start=t.index('fn scan_macro_sites(');end=t.index('\n/// Route scanned sites',start);frag=t[start:end].replace('            trusted_macro_binding_sites(','            trusted_macro_binding_sites_with_local_modules(').replace('                },\n            )','                },\n                local_modules_visible,\n            )');t=t[:start]+frag+t[end:]
# Keep test-only singleton reference under the same exact capability.
start=t.index('fn workspace_macro_binding_site(');end=t.index('\n/// A definition or',start);frag=t[start:end].replace('    let scan = |','    let local_modules_visible = OwnerPinSyntax::default().local_glob_namespace_is_closed(index);\n    let scan = |').replace('            module_resolved,\n        )','            module_resolved,\n            local_modules_visible,\n        )');t=t[:start]+frag+t[end:]
s.write_text(t)
