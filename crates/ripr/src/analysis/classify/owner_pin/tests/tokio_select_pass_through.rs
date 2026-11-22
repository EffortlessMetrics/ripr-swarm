//! #14116: tokio's `select!` places its branches in pattern, expression and
//! block fragments, so a `format!` in a branch handler keeps its mention
//! inside the call where the `tokio` root is the audited locked version.

use super::*;

fn format_sites(source: &str, verified: &dyn Fn(&str) -> bool) -> Vec<MacroBindingSite> {
    trusted_macro_binding_sites(
        source,
        &BTreeSet::new(),
        &["assert_eq", "format"],
        &|_, _| false,
        verified,
    )
    .into_iter()
    .filter(|(name, _)| name == "format")
    .map(|(_, site)| site)
    .collect()
}

fn tokio_verified(krate: &str) -> bool {
    krate == "tokio"
}

/// The shape of ripr's LSP progress test: a `select!` whose first handler
/// maps an error through `format!`.
fn select(prelude: &str, call: &str) -> String {
    format!(
        "{prelude}async fn drain() -> Result<(), String> {{\n    {call}! {{\n        reached = &mut witness, if waiting => {{\n            let reached = reached.map_err(|error| format!(\"dropped: {{error}}\"))?;\n        }}\n        message = &mut frame => {{}}\n    }}\n    Ok(())\n}}\n"
    )
}

#[test]
fn an_audited_tokio_select_keeps_its_branches_confined() {
    let qualified = select("", "tokio::select");
    // Fixture admission: the `format!` sits in the first branch's handler.
    assert!(
        qualified
            .contains("if waiting => {\n            let reached = reached.map_err(|error| format!")
    );
    for (label, source) in [
        ("qualified", qualified.clone()),
        ("external", select("", "::tokio::select")),
    ] {
        let sites = format_sites(&source, &tokio_verified);
        assert!(
            !sites.is_empty()
                && sites
                    .iter()
                    .all(|site| site.scope == Some((2, 7)) && site.crate_local),
            "{label}: {sites:?}"
        );
    }
}

#[test]
fn a_select_without_the_audited_tokio_root_stays_workspace_wide() {
    let workspace_wide = |sites: &[MacroBindingSite]| {
        !sites.is_empty()
            && sites
                .iter()
                .all(|site| site.scope.is_none() && !site.crate_local)
    };
    // The lock or manifest does not prove the audited `tokio`.
    let sites = format_sites(&select("", "tokio::select"), &|_| false);
    assert!(workspace_wide(&sites), "unverified: {sites:?}");
    for (label, source) in [
        // A bare `select!` names whatever the file imports or defines.
        ("bare", select("use tokio::select;\n", "select")),
        ("other-root", select("", "dep::select")),
        // Another tokio macro is not audited.
        ("join", select("", "tokio::join")),
        // A rename or module takes the root's name.
        (
            "renamed-root",
            select("use dep as tokio;\n", "tokio::select"),
        ),
        ("module-root", select("mod tokio;\n", "tokio::select")),
        // A plain import of the root name takes over the relative path.
        (
            "imported-root",
            select("use dep::tokio;\n", "tokio::select"),
        ),
    ] {
        let sites = format_sites(&source, &tokio_verified);
        assert!(workspace_wide(&sites), "{label}: {sites:?}");
    }
}
