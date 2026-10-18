from pathlib import Path
R=Path('/workspace/macro-site-parity');P=Path('/workspace/scratch/8265-reconstruction')
p=R/'crates/ripr/src/analysis/syntax/owner_pin.rs';s=p.read_text();a=s.index('fn macro_binding_ambiguities(\n');b=s.index('\n/// `pretty_assertions` exports',a);old=s[a:b];(P/'base-syntax-query.rs').write_text(old);prefix,body=old.split('    // Built only when a binding site is found, which most files lack.\n',1);signature=prefix[:prefix.index('    let mut ambiguous')];prefilter='''fn macro_binding_scan_needed(source: &str, trusted: &[&str]) -> bool {
    trusted.contains(&"json") && source.contains("serde_json")
        || source.contains("macro") || source.contains("use")
        || source.contains("no_implicit_prelude") || source.contains('!')
}

type NamedMacroSites = Vec<(String, MacroBindingSite)>;

''';wrapper=signature+'''    macro_binding_ambiguity_sets(source, packages, trusted, allowed_empty, module_resolved, drop_in_verified, None).0
}

/// Returns the admission union and per-name disclosure queries. Their trust
/// contexts differ; only the parse and binding-free wrapper index are shared.
pub(crate) fn trusted_macro_binding_site_sets(
    source: &str, packages: &BTreeSet<String>, trusted: &[&str], singleton_names: &[&str],
    module_resolved: &dyn Fn(usize, &str) -> bool, drop_in_verified: &dyn Fn(&str) -> bool,
) -> (NamedMacroSites, NamedMacroSites) {
    macro_binding_ambiguity_sets(source, packages, trusted, &BTreeSet::new(), module_resolved, drop_in_verified, Some(singleton_names))
}

fn macro_binding_ambiguity_sets(
    source: &str, packages: &BTreeSet<String>, trusted: &[&str], allowed_empty: &BTreeSet<String>,
    module_resolved: &dyn Fn(usize, &str) -> bool, drop_in_verified: &dyn Fn(&str) -> bool,
    singleton_names: Option<&[&str]>,
) -> (NamedMacroSites, NamedMacroSites) {
    if !macro_binding_scan_needed(source, trusted) { return (Vec::new(), Vec::new()); }
    let Some(parse) = parse_clean_source_file(source) else {
        let site = MacroBindingSite { line: 0, kind: MacroBindingKind::Unparsed, scope: None, crate_local: false };
        let union: NamedMacroSites = trusted.iter().map(|name| ((*name).to_string(), site.clone())).collect();
        let singletons = singleton_names.into_iter().flatten()
            .filter(|name| macro_binding_scan_needed(source, &[name]))
            .map(|name| ((*name).to_string(), site.clone())).collect();
        return (union, singletons);
    };
    let tree = parse.tree();
    let binding_free = BindingFreeFileIndex::collect(tree.syntax());
    let scan = |names: &[&str]| macro_binding_sites_in_tree(source, packages, names, allowed_empty, module_resolved, drop_in_verified, (&tree, &binding_free));
    let union = scan(trusted);
    let singletons = singleton_names.into_iter().flatten()
        .filter(|name| macro_binding_scan_needed(source, &[name]))
        .flat_map(|name| scan(&[name])).collect();
    (union, singletons)
}

fn macro_binding_sites_in_tree(
    source: &str, packages: &BTreeSet<String>, trusted: &[&str], allowed_empty: &BTreeSet<String>,
    module_resolved: &dyn Fn(usize, &str) -> bool, drop_in_verified: &dyn Fn(&str) -> bool,
    (tree, binding_free): (&ast::SourceFile, &BindingFreeFileIndex),
) -> NamedMacroSites {
    let mut ambiguous = Vec::new();
    let all = |line: usize, kind: MacroBindingKind, crate_local: bool| {
        trusted.iter().map(|name| ((*name).to_string(), MacroBindingSite {line, kind: kind.clone(), scope: None, crate_local})).collect::<Vec<_>>()
    };
    // Built only when a binding site is found, which most files lack.
''';body=body.replace('parse.tree().syntax()', 'tree.syntax()').replace('binding_free_local_wrapper(&binding_free,','binding_free_local_wrapper(binding_free,');p.write_text(s[:a]+prefilter+wrapper+body+s[b:])
p=R/'crates/ripr/src/analysis/syntax/mod.rs';s=p.read_text();s=s.replace('trusted_macro_binding_sites,','trusted_macro_binding_site_sets, trusted_macro_binding_sites,',1);p.write_text(s)
p=R/'crates/ripr/src/analysis/classify/owner_pin.rs';s=p.read_text();s=s.replace('trusted_macro_binding_sites,','trusted_macro_binding_site_sets, trusted_macro_binding_sites,',1);s=s.replace('Option<&(PathBuf, MacroBindingSite)>','Option<&OrderedMacroSite>');s=s.replace('rebinding.cloned().or_else(|| may_rebind.cloned())','rebinding.or(may_rebind).map(|site| (site.path.clone(), site.site.clone()))');a=s.index('        for (path, name, site) in sites {',s.index('fn complete_workspace_site_scan'));b=s.index('        retained.complete = true;',a);s=s[:a]+'        retain_macro_sites(sites.retained, retained, roots, index);\n'+s[b:];s=s.replace('rebinding: Option<(PathBuf, MacroBindingSite)>,','rebinding: Option<OrderedMacroSite>,').replace('may_rebind: Option<(PathBuf, MacroBindingSite)>,','may_rebind: Option<OrderedMacroSite>,');a=s.index('impl MacroSiteRank {');b=s.index('/// The crate root whose module tree',a);s=s[:a]+'''/// Producer position, rather than line number, preserves same-line order
/// when global and crate-local buckets are merged.
#[derive(Clone, Debug)]
struct OrderedMacroSite { path: PathBuf, position: usize, site: MacroBindingSite }

impl OrderedMacroSite {
    fn precedes(&self, other: &Self) -> bool { (&self.path, self.position) < (&other.path, other.position) }
}
impl MacroSiteRank {
    fn absorb(&mut self, path: &Path, position: usize, site: &MacroBindingSite) {
        let slot = if rebinds(site) { &mut self.rebinding } else { &mut self.may_rebind };
        if slot.as_ref().is_none_or(|kept| (path, position) < (kept.path.as_path(), kept.position)) {
            *slot = Some(OrderedMacroSite { path: path.to_path_buf(), position, site: site.clone() });
        }
    }
}
fn keep_earliest_site<'a>(slot: &mut Option<&'a OrderedMacroSite>, candidate: Option<&'a OrderedMacroSite>) {
    if let Some(candidate) = candidate && slot.as_ref().is_none_or(|kept| candidate.precedes(kept)) { *slot = Some(candidate); }
}
fn merge_macro_site_rank<'a>(rebinding: &mut Option<&'a OrderedMacroSite>, may_rebind: &mut Option<&'a OrderedMacroSite>, rank: &'a MacroSiteRank) {
    keep_earliest_site(rebinding, rank.rebinding.as_ref());
    keep_earliest_site(may_rebind, rank.may_rebind.as_ref());
}

'''+s[b:];a=s.index('/// Every trusted name\'s sites in `files`');b=s.index('/// Route scanned sites',a);s=s[:a]+'''#[derive(Default)]
struct ScannedMacroSites {
    ambiguities: Vec<(PathBuf, String, MacroBindingSite)>,
    retained: Vec<(PathBuf, String, usize, MacroBindingSite)>,
}

/// One parse per candidate file in the bounded pass. Singleton queries share
/// that tree/index, and stored candidates prune impossible names.
fn scan_macro_sites(files: &[MacroScanFile<'_>], index: &RustIndex,
    module_resolved: &(dyn Fn(&Path, usize, &str) -> bool + Sync)) -> ScannedMacroSites {
    let names = assertion_context_macros();
    let per_file: Vec<_> = files.par_iter()
        .filter(|(_, _, candidates)| candidates.is_none_or(MacroBindingCandidates::may_bind_any_trusted))
        .map(|(path, source, candidates)| {
            let singleton_names: Vec<_> = names.iter().copied().filter(|name| candidates.is_none_or(|c| c.may_bind(name))).collect();
            let (union, singleton) = trusted_macro_binding_site_sets(source, index.macro_scope_crates(), &names, &singleton_names,
                &|line, declaration| module_resolved(path, line, declaration),
                &|krate| if krate == "serde_json" { index.drop_in_manifests.verified_literal_json(path) } else { index.drop_in_manifests.verified(path, krate) });
            (*path, union, singleton)
        }).collect();
    let mut scanned = ScannedMacroSites::default();
    for (path, union, singleton) in per_file {
        scanned.ambiguities.extend(union.into_iter().map(|(name, site)| (path.clone(), name, site)));
        scanned.retained.extend(singleton.into_iter().enumerate().map(|(position, (name, site))| (path.clone(), name, position, site)));
    }
    scanned
}

/// Both initial absorption and deferred completion exclude every scoped site.
fn retain_macro_sites(sites: Vec<(PathBuf, String, usize, MacroBindingSite)>, retained: &mut WorkspaceMacroSiteIndex, roots: &TargetRoots, index: &RustIndex) {
    for (path, name, position, site) in sites {
        if site.scope.is_some() { continue; }
        let names = match site.crate_local.then(|| roots.root(&path, index)).flatten() {
            Some(root) => retained.by_root.entry(root).or_default(), None => &mut retained.global,
        };
        names.entry(name).or_default().absorb(&path, position, &site);
    }
}

'''+s[b:];s=s.replace('    sites: Vec<(PathBuf, String, MacroBindingSite)>,\n    global:', '    sites: ScannedMacroSites,\n    global:');s=s.replace('    for (path, name, site) in sites {\n        if site.scope.is_some() {','    retain_macro_sites(sites.retained, retained, roots, index);\n    for (path, name, site) in sites.ambiguities {\n        if site.scope.is_some() {');s=s.replace('''                retained
                    .by_root
                    .entry(root.clone())
                    .or_default()
                    .entry(name.clone())
                    .or_default()
                    .absorb(&path, &site);
''','').replace('''                retained
                    .global
                    .entry(name.clone())
                    .or_default()
                    .absorb(&path, &site);
''','');p.write_text(s)
