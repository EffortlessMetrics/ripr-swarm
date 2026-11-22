//! #14116: the standard `pin!` places its argument in an expression, so a
//! `format!` in the future it pins keeps its mention inside the call.

use super::*;

/// The `format` sites one source reports, with the standard roots proved
/// unbound by any manifest. The standard `pin!` needs no lock audit.
fn format_sites(source: &str) -> Vec<MacroBindingSite> {
    format_sites_verified(source, &|krate| matches!(krate, "std" | "core"))
}

fn format_sites_verified(source: &str, verified: &dyn Fn(&str) -> bool) -> Vec<MacroBindingSite> {
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

/// The shape of ripr's LSP debounce test: a pinned future whose error is a
/// `format!`, with a `tokio::select!` statement later in the same block.
fn pinned(prelude: &str, call: &str, block_extra: &str) -> String {
    format!(
        "{prelude}fn run() {{\n    let fut = {call}!(async {{\n        Err::<(), String>(format!(\"never {{}}\", 1))\n    }});\n    {block_extra}tokio::select! {{ _ = fut => {{}} }}\n}}\n"
    )
}

const STD: &str = "use std::pin::pin;\n";

fn confined(sites: &[MacroBindingSite]) -> bool {
    !sites.is_empty()
        && sites
            .iter()
            .all(|site| site.scope.is_some() && site.crate_local)
}

/// Every argument site is unconfined. A local definition's own
/// redefinition site is crate-local by design and is not an argument site.
fn workspace_wide(sites: &[MacroBindingSite]) -> bool {
    let arguments: Vec<_> = sites
        .iter()
        .filter(|site| matches!(site.kind, MacroBindingKind::MacroArgument(_)))
        .collect();
    !arguments.is_empty()
        && arguments
            .iter()
            .all(|site| site.scope.is_none() && !site.crate_local)
}

#[test]
fn a_standard_pin_keeps_its_argument_confined() {
    let bare = pinned(STD, "pin", "");
    // Fixture admission: the `format!` sits inside the pinned future.
    assert!(bare.contains("let fut = pin!(async {\n        Err::<(), String>(format!"));
    for (label, source) in [
        ("bare", bare.clone()),
        ("bare-core", pinned("use core::pin::pin;\n", "pin", "")),
        ("bare-external", pinned("use ::std::pin::pin;\n", "pin", "")),
        ("qualified", pinned("", "std::pin::pin", "")),
        ("qualified-core", pinned("", "core::pin::pin", "")),
        ("qualified-external", pinned("", "::std::pin::pin", "")),
        // A statement-position call in the block: an import or `pin`
        // definition it expands to cannot shadow the module's import
        // (rustc E0659, macro-expanded name vs outer name), as the
        // `tokio::select!` in every row shows too.
        ("statement-call", pinned(STD, "pin", "define! {}\n    ")),
    ] {
        // The site spans the `let fut = ..;` statement's three lines, after
        // the import line where there is one.
        let first = if source.starts_with("use ") { 3 } else { 2 };
        let sites = format_sites(&source);
        assert!(
            confined(&sites)
                && sites
                    .iter()
                    .all(|site| site.scope == Some((first, first + 2))),
            "{label}: {sites:?}"
        );
    }
}

#[test]
fn a_pin_that_may_name_another_macro_stays_workspace_wide() {
    for (label, source) in [
        // Nothing imports the bare name.
        ("unimported", pinned("", "pin", "")),
        ("other-root", pinned("use dep::pin;\n", "pin", "")),
        ("crate-root", pinned("use crate::pin::pin;\n", "pin", "")),
        ("renamed", pinned("use std::pin::pin as pin;\n", "pin", "")),
        // A second import of the name anywhere in the file.
        (
            "second-import",
            pinned(STD, "pin", "{ use dep::pin; }\n    "),
        ),
        // A local definition outranks the import by textual scope.
        (
            "local-definition",
            pinned(
                "use std::pin::pin;\nmacro_rules! pin { ($e:expr) => { $e } }\n",
                "pin",
                "",
            ),
        ),
        // A glob or an item-position call in the block can shadow the import.
        ("inner-glob", pinned(STD, "pin", "use dep::*;\n    ")),
        // An item-position call in the call's module is refused outright.
        (
            "item-call",
            pinned("use std::pin::pin;\ndefine! {}\n", "pin", ""),
        ),
        // The import must be an item of the call's own module.
        (
            "import-in-other-module",
            format!("mod other {{ {STD} }}\n{}", pinned("", "pin", "")),
        ),
        // A module or alias taking the standard root's name.
        ("module-std", pinned("mod std;\n", "std::pin::pin", "")),
        (
            "alias-core",
            pinned("use dep as core;\n", "core::pin::pin", ""),
        ),
        ("other-path", pinned("", "dep::pin::pin", "")),
    ] {
        let sites = format_sites(&source);
        assert!(workspace_wide(&sites), "{label}: {sites:?}");
    }
}

#[test]
fn a_pin_whose_standard_root_a_manifest_may_rebind_stays_workspace_wide() {
    // Only `core` is proved unbound: a dependency renamed to `std` would
    // take over every `std::` path.
    let core_only = |krate: &str| krate == "core";
    for (label, source) in [
        ("bare", pinned(STD, "pin", "")),
        ("qualified", pinned("", "std::pin::pin", "")),
        ("external", pinned("", "::std::pin::pin", "")),
    ] {
        assert!(
            workspace_wide(&format_sites_verified(&source, &core_only)),
            "{label}"
        );
    }
    // Control: the `core` spelling stays confined under the same verifier.
    let sites = format_sites_verified(&pinned("", "core::pin::pin", ""), &core_only);
    assert!(confined(&sites), "{sites:?}");
}

#[test]
fn a_local_pin_that_pastes_its_tokens_reports_a_redefinition() {
    let source = "macro_rules! pin { ($($t:tt)*) => { $($t)* } }\n";
    let sites = trusted_macro_binding_sites(
        source,
        &BTreeSet::new(),
        &["assert_eq"],
        &|_, _| false,
        &|_| false,
    );
    assert!(
        sites.iter().any(|(_, site)| matches!(
            &site.kind,
            MacroBindingKind::WrapperRedefinition(name) if name == "pin"
        )),
        "{sites:?}"
    );
    // A definition that pastes nothing adds nothing.
    let inert = "macro_rules! pin { () => { 1 } }\n";
    let sites = trusted_macro_binding_sites(
        inert,
        &BTreeSet::new(),
        &["assert_eq"],
        &|_, _| false,
        &|_| false,
    );
    assert!(
        !sites
            .iter()
            .any(|(_, site)| matches!(&site.kind, MacroBindingKind::WrapperRedefinition(_))),
        "{sites:?}"
    );
}

#[test]
fn a_local_pin_that_pastes_only_node_fragments_reports_no_redefinition() {
    let redefines = |source: &str| {
        trusted_macro_binding_sites(
            source,
            &BTreeSet::new(),
            &["assert_eq"],
            &|_, _| false,
            &|_| false,
        )
        .iter()
        .any(|(_, site)| {
            matches!(
                &site.kind,
                MacroBindingKind::WrapperRedefinition(name) if name == "pin"
            )
        })
    };
    // An expression, type, pattern, literal, lifetime or block fragment is
    // pasted as one node: a definition inside it stays inside it. The first
    // row is the polling-helper fixture's counterfeit `pin!`.
    for source in [
        "macro_rules! pin { ($f:expr) => {{ drop($f); Box::pin(async { 110u32 }) }} }\n",
        "macro_rules! pin { ($($f:expr),*) => { ($($f,)*) } }\n",
        "macro_rules! pin { ($t:ty, $b:block) => { let _: $t = $b; } }\n",
        "macro_rules! pin { ($f:expr) => { $crate::wrap($f) } }\n",
    ] {
        assert!(!redefines(source), "{source}");
    }
    // Fragments that can carry an item or statement, or a name the scan
    // cannot tie to its matcher, still report the redefinition.
    for source in [
        "macro_rules! pin { ($i:item) => { $i } }\n",
        "macro_rules! pin { ($s:stmt) => { $s } }\n",
        "macro_rules! pin { ($n:ident) => { macro_rules! $n { () => {} } } }\n",
        "macro_rules! pin { ($f:expr) => { $f }; ($t:tt) => { $t } }\n",
        "macro_rules! pin { ($f:expr) => { $g } }\n",
    ] {
        assert!(redefines(source), "{source}");
    }
}

#[test]
fn an_unfinished_pin_arm_with_a_non_ascii_token_reads_as_opaque() {
    // A balanced body whose transcriber is not yet a group: the scan keeps
    // the last two characters of `=>値` and must not split `値`.
    for source in [
        "macro_rules! pin { ($x:expr) => 値 }\n",
        "macro_rules! pin { ($x:expr) => 値値 ($($y:tt)*) => { $($y)* } }\n",
    ] {
        let sites = trusted_macro_binding_sites(
            source,
            &BTreeSet::new(),
            &["assert_eq"],
            &|_, _| false,
            &|_| false,
        );
        assert!(
            sites.iter().any(|(_, site)| matches!(
                &site.kind,
                MacroBindingKind::WrapperRedefinition(name) if name == "pin"
            )),
            "{source}: {sites:?}"
        );
    }
}
