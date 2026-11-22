//! #14116: a `serde_json::json!` nested in an audited `serde_json::json!`
//! call is the same expression-only wrapper, so a trusted name the nested
//! call's arguments mention keeps the outer call confined to its own text.

use super::*;

/// The `format` sites one source reports, with drop-in roots verified as
/// `verified` says.
fn format_sites(source: &str, verified: &dyn Fn(&str) -> bool) -> Vec<MacroBindingSite> {
    trusted_macro_binding_sites(
        source,
        &BTreeSet::new(),
        &["assert_eq", "format", "file", "line"],
        &|_, _| false,
        verified,
    )
    .into_iter()
    .filter(|(name, _)| name == "format")
    .map(|(_, site)| site)
    .collect()
}

fn all_verified(_: &str) -> bool {
    true
}

/// The shape of ripr's LSP payload: a `format!` beside a nested call whose
/// arguments read fields named `file` and `line`.
fn outer(prefix: &str, outer_path: &str, nested: &str) -> String {
    format!(
        "{prefix}pub fn render(t: &Test) -> serde_json::Value {{\n    {outer_path}!({{\n        \"name\": format!(\"{{}}::t\", t.name),\n        \"location\": {nested}!({{ \"file\": t.file, \"line\": t.line }}),\n    }})\n}}\n"
    )
}

fn confined(sites: &[MacroBindingSite]) -> bool {
    !sites.is_empty()
        && sites.iter().all(|site| {
            matches!(&site.kind, MacroBindingKind::MacroArgument(_))
                && site.scope.is_some()
                && site.crate_local
        })
}

fn workspace_wide(sites: &[MacroBindingSite]) -> bool {
    !sites.is_empty()
        && sites
            .iter()
            .all(|site| site.scope.is_none() && !site.crate_local)
}

#[test]
fn a_nested_serde_json_call_keeps_the_outer_call_confined() {
    for nested in ["serde_json::json", "::serde_json::json"] {
        let source = outer("", "serde_json::json", nested);
        // Fixture admission: the nested call reads `file` and `line`, and
        // the `format!` sits beside it in the outer call.
        assert!(source.contains(&format!(
            "\"location\": {nested}!({{ \"file\": t.file, \"line\": t.line }})"
        )));
        let sites = format_sites(&source, &all_verified);
        // The site spans the outer call's lines, 2 through 5.
        assert!(
            confined(&sites) && sites.iter().all(|site| site.scope == Some((2, 5))),
            "{nested}: {sites:?}"
        );
    }
}

#[test]
fn a_nested_json_call_under_another_root_stays_workspace_wide() {
    for nested in ["dep::json", "crate::json", "serde_json::other"] {
        let sites = format_sites(&outer("", "serde_json::json", nested), &all_verified);
        assert!(workspace_wide(&sites), "{nested}: {sites:?}");
    }
}

#[test]
fn the_nested_root_is_proved_on_its_own() {
    // A plain import (not a rename) takes over the relative `serde_json`
    // path and leaves
    // the absolute outer path verified, so only the nested call's own root
    // decides.
    let alias = "use dep::serde_json;\n";
    let sites = format_sites(
        &outer(alias, "::serde_json::json", "::serde_json::json"),
        &all_verified,
    );
    assert!(confined(&sites), "control: {sites:?}");
    let sites = format_sites(
        &outer(alias, "::serde_json::json", "serde_json::json"),
        &all_verified,
    );
    assert!(workspace_wide(&sites), "{sites:?}");
    // An import of `serde_json` inside the arguments does the same.
    let inner_import = "pub fn render(t: &Test) -> serde_json::Value {\n    ::serde_json::json!({\n        \"name\": format!(\"{}::t\", t.name),\n        \"location\": { use crate::custom::serde_json; serde_json::json!({ \"file\": t.file }) },\n    })\n}\n";
    let sites = format_sites(inner_import, &all_verified);
    assert!(workspace_wide(&sites), "{sites:?}");
}

#[test]
fn a_nested_call_that_names_no_trusted_macro_needs_no_root_proof() {
    // Only `proptest` verifies: an unverified `serde_json` root matters only
    // where the nested call's own arguments name a trusted macro.
    let proptest_only = |krate: &str| krate == "proptest";
    let source = "proptest::proptest! {\n    #[test]\n    fn p(x in 0u8..3) {\n        let value = serde_json::json!(0);\n        assert_eq!(format!(\"{}\", x), value.to_string());\n    }\n}\n";
    let sites = format_sites(source, &proptest_only);
    assert!(
        !sites.is_empty() && sites.iter().all(|site| site.scope.is_some()),
        "{sites:?}"
    );
    let reads_line = source.replace("serde_json::json!(0)", "serde_json::json!(t.line)");
    let sites = format_sites(&reads_line, &proptest_only);
    assert!(workspace_wide(&sites), "{sites:?}");
}
