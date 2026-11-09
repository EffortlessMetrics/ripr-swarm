use super::*;
use crate::analysis::facts::FunctionSourceRole;
use crate::domain::SymbolId;
use std::path::PathBuf;

fn owner(body: &str) -> FunctionSummary {
    FunctionSummary {
        id: SymbolId("src/lib.rs::f".to_string()),
        name: "f".to_string(),
        file: PathBuf::from("src/lib.rs"),
        start_line: 1,
        end_line: body.lines().count(),
        body: body.into(),
        calls: crate::analysis::facts::SharedFactSlice::empty(),
        returns: Vec::new(),
        literals: crate::analysis::facts::SharedFactSlice::empty(),
        source_role: FunctionSourceRole::Production,
        attrs: crate::analysis::facts::SharedFactSlice::empty(),
        impl_attrs: Vec::new(),
        nested_fn_names: crate::analysis::facts::SharedFactSlice::empty(),
        let_bindings: crate::analysis::facts::SharedFactSlice::empty(),
        item: Default::default(),
        impl_context: Default::default(),
    }
}

fn params(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

const FLAGGED: &str = "if si { KIB } else { KB }";

#[test]
fn a_negated_flag_rebinding_flips_the_condition_the_local_reads() {
    let body = "pub fn tier_shadowed(bytes: u64, si: bool) -> &'static str {\n    let si = !si;\n    let unit = if si { KIB } else { KB };\n    if bytes >= unit { \"unit\" } else { \"byte\" }\n}\n";
    let found = owner_prelude(&owner(body), "unit", FLAGGED, &params(&["bytes", "si"]));
    let expected = OwnerPrelude {
        negated_flags: vec!["si".to_string()],
        constants: Vec::new(),
    };
    assert_eq!(found, Some(expected));
    let found = owner_prelude(&owner(body), "unit", FLAGGED, &params(&["bytes", "si"]));
    assert_eq!(
        found.and_then(|prelude| prelude.rewrite_initializer(FLAGGED)),
        Some("if !si { KIB } else { KB }".to_string())
    );
    // An already negated condition flips back to the plain flag.
    let negated = OwnerPrelude {
        negated_flags: vec!["si".to_string()],
        constants: Vec::new(),
    };
    assert_eq!(
        negated.rewrite_initializer("if !si { KB } else { KIB }"),
        Some("if si { KB } else { KIB }".to_string())
    );
    // The flag read anywhere else in the initializer declines.
    assert_eq!(
        negated.rewrite_initializer("if si { si } else { KB }"),
        None
    );
    assert_eq!(negated.rewrite_initializer("KB"), None);
    assert_eq!(
        negated.rewrite_initializer("if sin { KIB } else { KB }"),
        None
    );
}

#[test]
fn a_function_local_constant_replaces_the_file_constant() {
    let body = "pub fn tier_alias(bytes: u64) -> &'static str {\n    const KB: u64 = 2_000;\n    let unit = KB;\n    if bytes >= unit { \"unit\" } else { \"byte\" }\n}\n";
    let found = owner_prelude(&owner(body), "unit", "KB", &params(&["bytes"]));
    assert_eq!(
        found,
        Some(OwnerPrelude {
            negated_flags: Vec::new(),
            constants: vec![("KB".to_string(), "2_000".to_string())],
        })
    );
}

#[test]
fn the_prelude_declines_every_shape_it_does_not_read_exactly() {
    let parameters = params(&["bytes", "si"]);
    let tail =
        "    let unit = if si { KIB } else { KB };\n    if bytes >= unit { 1 } else { 0 }\n}\n";
    let declined = |prelude: &str| {
        let body = format!("fn f(bytes: u64, si: bool) -> u8 {{\n{prelude}{tail}");
        owner_prelude(&owner(&body), "unit", FLAGGED, &parameters)
    };
    // Nothing rebound: no prelude, the caller's own checks stand.
    assert_eq!(declined(""), None);
    // A loop, block, call, macro or closure in the prelude.
    assert_eq!(declined("    for si in [false] {}\n"), None);
    assert_eq!(
        declined("    let si = !si;\n    { let si = true; }\n"),
        None
    );
    assert_eq!(declined("    let si = flip(si);\n"), None);
    assert_eq!(declined("    let si = !si;\n    shadow!();\n"), None);
    // A rebinding that is not exactly `let NAME = !NAME;`.
    assert_eq!(declined("    let si = !other;\n"), None);
    assert_eq!(declined("    let mut si = !si;\n"), None);
    assert_eq!(declined("    let si = !si && true;\n"), None);
    // A name that is not a parameter, or one rebound twice.
    assert_eq!(declined("    let other = !other;\n"), None);
    assert_eq!(declined("    let si = !si;\n    let si = !si;\n"), None);
    // A constant item off the exact integer shape.
    assert_eq!(declined("    const KB: u64 = 2_000 + 1;\n"), None);
    assert_eq!(declined("    const KB: f64 = 2_000;\n"), None);
    assert_eq!(declined("    const KB: u64 = -2;\n"), None);
    assert_eq!(declined("    const kb: u64 = 2;\n"), None);
    assert_eq!(declined("    static KB: u64 = 2;\n"), None);
    assert_eq!(
        declined("    const KB: u64 = 2;\n    const KB: u64 = 3;\n"),
        None
    );
    // Any other statement before the local.
    assert_eq!(declined("    let si = !si;\n    let x = 1;\n"), None);
}

#[test]
fn a_rebound_name_mentioned_outside_the_prelude_and_the_initializer_declines() {
    let parameters = params(&["bytes", "si"]);
    // A later shadow of the flag after the local.
    let later = "fn f(bytes: u64, si: bool) -> u8 {\n    let si = !si;\n    let unit = if si { KIB } else { KB };\n    let si = true;\n    if bytes >= unit { 1 } else { 0 }\n}\n";
    assert_eq!(
        owner_prelude(&owner(later), "unit", FLAGGED, &parameters),
        None
    );
    // A nested block that declares the constant again.
    let nested = "fn f(bytes: u64) -> u8 {\n    const KB: u64 = 2_000;\n    let unit = KB;\n    { const KB: u64 = 5; }\n    if bytes >= unit { 1 } else { 0 }\n}\n";
    assert_eq!(
        owner_prelude(&owner(nested), "unit", "KB", &params(&["bytes"])),
        None
    );
    // A macro after the local could expand to a rebinding.
    let macro_after = "fn f(bytes: u64) -> u8 {\n    const KB: u64 = 2_000;\n    let unit = KB;\n    rebind!();\n    if bytes >= unit { 1 } else { 0 }\n}\n";
    assert_eq!(
        owner_prelude(&owner(macro_after), "unit", "KB", &params(&["bytes"])),
        None
    );
    // A second read of the constant outside the initializer.
    let twice = "fn f(bytes: u64) -> u8 {\n    const KB: u64 = 2_000;\n    let unit = KB;\n    let other = KB;\n    if bytes >= unit { 1 } else { 0 }\n}\n";
    assert_eq!(
        owner_prelude(&owner(twice), "unit", "KB", &params(&["bytes"])),
        None
    );
    // A `mut` local is not the `let NAME =` shape.
    let mutable = "fn f(bytes: u64) -> u8 {\n    const KB: u64 = 2_000;\n    let mut unit = KB;\n    if bytes >= unit { 1 } else { 0 }\n}\n";
    assert_eq!(
        owner_prelude(&owner(mutable), "unit", "KB", &params(&["bytes"])),
        None
    );
    // The local declared twice.
    let redeclared = "fn f(bytes: u64) -> u8 {\n    const KB: u64 = 2_000;\n    let unit = KB;\n    let unit = unit;\n    if bytes >= unit { 1 } else { 0 }\n}\n";
    assert_eq!(
        owner_prelude(&owner(redeclared), "unit", "KB", &params(&["bytes"])),
        None
    );
    // A shadow through a pattern after the local adds a mention, which only
    // the token count catches.
    for shadow in [
        "    if let Some(si) = Some(true) { let _ = si; }\n",
        "    match bytes { si => { let _ = si; } }\n",
        "    for si in [true] { let _ = si; }\n",
    ] {
        let body = format!(
            "fn f(bytes: u64, si: bool) -> u8 {{\n    let si = !si;\n    let unit = if si {{ KIB }} else {{ KB }};\n{shadow}    if bytes >= unit {{ 1 }} else {{ 0 }}\n}}\n"
        );
        assert_eq!(
            owner_prelude(&owner(&body), "unit", FLAGGED, &parameters),
            None,
            "{shadow}"
        );
    }
    // A macro call spelled with whitespace before `!`.
    let spaced_macro = "fn f(bytes: u64) -> u8 {\n    const KB: u64 = 2_000;\n    let unit = KB;\n    rebind !();\n    if bytes >= unit { 1 } else { 0 }\n}\n";
    assert_eq!(
        owner_prelude(&owner(spaced_macro), "unit", "KB", &params(&["bytes"])),
        None
    );
}

#[test]
fn a_local_constant_named_like_a_parameter_declines() {
    // Rust reads the block's `const KB`, but the row's `KB` would win in the
    // evaluator's inputs, so the prelude must not resolve it (#12771 review).
    let body = "pub fn tier(bytes: u64, KB: u64) -> bool {\n    const KB: u64 = 2_000;\n    let unit = KB;\n    bytes >= unit\n}\n";
    assert_eq!(
        owner_prelude(&owner(body), "unit", "KB", &params(&["bytes", "KB"])),
        None
    );
    assert!(owner_prelude(&owner(body), "unit", "KB", &params(&["bytes"])).is_some());
}
