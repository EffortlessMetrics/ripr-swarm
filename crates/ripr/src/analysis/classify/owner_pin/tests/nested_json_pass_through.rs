//! #14116: a `serde_json::json!` nested in an audited `serde_json::json!`
//! call is the same expression-only wrapper, so a trusted name the nested
//! call's arguments mention stays confined to the outer call's text.

use super::*;

/// The `format` sites one source reports with every drop-in root verified
/// or none.
fn format_sites(source: &str, verified: bool) -> Vec<MacroBindingSite> {
    trusted_macro_binding_sites(
        source,
        &BTreeSet::new(),
        &["format"],
        &|_, _| false,
        &|_| verified,
    )
    .into_iter()
    .map(|(_, site)| site)
    .collect()
}

fn outer(nested: &str) -> String {
    format!(
        "pub fn render(name: &str) -> serde_json::Value {{\n    serde_json::json!({{\n        \"tests\": [\n            {nested}!({{\n                \"name\": format!(\"{{}}::t\", name),\n            }}),\n        ],\n    }})\n}}\n"
    )
}

#[test]
fn a_nested_serde_json_call_keeps_the_outer_call_confined() {
    for nested in ["serde_json::json", "::serde_json::json"] {
        let source = outer(nested);
        // Fixture admission: the `format!` sits in the nested call's group.
        assert!(source.contains(&format!("{nested}!({{\n                \"name\": format!")));
        let sites = format_sites(&source, true);
        assert_eq!(sites.len(), 1, "{nested}: {sites:?}");
        let site = &sites[0];
        assert!(
            matches!(&site.kind, MacroBindingKind::MacroArgument(wrapper) if wrapper == "serde_json::json")
                && site.line == 2
                && site.scope == Some((2, 8))
                && site.crate_local,
            "{nested}: {site:?}"
        );
    }
}

#[test]
fn a_nested_json_call_under_another_root_or_unverified_stays_workspace_wide() {
    for (nested, verified) in [
        ("dep::json", true),
        ("crate::json", true),
        ("serde_json::other", true),
        ("serde_json::json", false),
    ] {
        let sites = format_sites(&outer(nested), verified);
        assert!(!sites.is_empty(), "{nested}");
        assert!(
            sites
                .iter()
                .all(|site| site.scope.is_none() && !site.crate_local),
            "{nested} verified={verified}: {sites:?}"
        );
    }
    // A plain import of `serde_json` takes over the relative path.
    let imported = format!("use dep as serde_json;\n{}", outer("serde_json::json"));
    let sites = format_sites(&imported, true);
    assert!(
        !sites.is_empty() && sites.iter().all(|site| site.scope.is_none()),
        "{sites:?}"
    );
}
