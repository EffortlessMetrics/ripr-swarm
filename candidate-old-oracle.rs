fn workspace_macro_binding_site(
    name: &str,
    test_root: Option<&Path>,
    index: &RustIndex,
    roots: &TargetRoots,
    module_resolved: &(dyn Fn(&Path, usize, &str) -> bool + Sync),
) -> Option<(PathBuf, MacroBindingSite)> {
    if !assertion_context_macros().contains(&name) {
        return None;
    }
    // Every file may be parsed for this one name, which was a quarter of a
    // warm `ripr check` on ripr-swarm when run serially. Each file's answer
    // is independent, so files are scanned on the rayon pool in path-ordered
    // batches: the earliest file still wins, and a rebinding in an early
    // batch still stops the scan before later files are parsed.
    // Roots memoize in a `RefCell`, so they are resolved before the pool.
    let files: Vec<_> = index
        .files()
        .iter()
        .map(|(path, facts)| {
            let root = test_root.and_then(|_| roots.root(path, index));
            (path, facts, root)
        })
        .collect();
    let scan = |(path, facts, root): &(&PathBuf, FileFactsView<'_>, Option<PathBuf>)| {
        // The same rule as the decision: a crate-local site in another
        // recognized root does not reach this test.
        let reaches = |site: &MacroBindingSite| {
            !site.crate_local
                || test_root.is_none()
                || root
                    .as_ref()
                    .is_none_or(|site_root| Some(site_root.as_path()) == test_root)
        };
        let mut first = None;
        for (_, site) in macro_binding_sites(
            name,
            path,
            &facts.source,
            facts.macro_candidates.as_deref(),
            index,
            module_resolved,
        ) {
            if site.scope.is_some() || !reaches(&site) {
                continue;
            }
            if rebinds(&site) {
                return (Some(site), first);
            }
            first.get_or_insert(site);
        }
        (None, first)
    };
    // Wide enough that one large file does not stall the pool for long.
    let batch = rayon::current_num_threads().max(1) * 32;
    let mut first = None;
    for chunk in files.chunks(batch) {
        let per_file: Vec<_> = chunk.par_iter().map(scan).collect();
        for ((path, _, _), (rebinding, site)) in chunk.iter().zip(per_file) {
            if let Some(site) = rebinding {
                return Some(((*path).clone(), site));
            }
            if first.is_none() {
                first = site.map(|site| ((*path).clone(), site));
            }
        }
    }
    first
}

/// A definition or `use` of the name: a real rebinding, not one that only
/// may rebind it.