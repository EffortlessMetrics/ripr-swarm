//! #14115: a refused assertion names a site the admission scan also reports.
//! A call to another standard assertion-context macro is a wrapper only when
//! that macro has a binding site of its own in the file.
use super::*;

const GATE: &str = "pub fn gate(value: u32) -> bool {\n    10 <= value\n}\n";
const PINS: &str = "use demo::gate;\n\n#[test]\nfn pins() {\n    assert_eq!(gate(10), true);\n}\n";
const NESTED: &str = "src/a_nested.rs";
const GLOB: &str = "src/z_glob.rs";
/// The standard `assert!` over a closure that asserts with `assert_eq!`.
const NESTED_STANDARD: &str = "pub fn check(f: impl FnOnce() -> bool) -> bool {\n    f()\n}\n\n#[test]\nfn nested() {\n    assert!(check(|| {\n        assert_eq!(1, 1);\n        true\n    }));\n}\n";

/// The workspace site named for `name`, with the production retention
/// checked against the per-name reference scan.
fn named_site(files: &[(&str, &str)], name: &str) -> Option<(PathBuf, MacroBindingSite)> {
    let index = index(files);
    let syntax = OwnerPinSyntax::default();
    let roots = syntax.target_roots(&index);
    let modules = syntax.resolved_module_declarations(&index);
    let resolved = |path: &Path, line: usize, declaration: &str| {
        modules.contains(&(path.to_path_buf(), line, declaration.to_string()))
    };
    let reference = workspace_macro_binding_site(name, None, &index, roots, &resolved);
    let retained = syntax.workspace_macro_site(name, None, &index);
    assert_eq!(retained, reference, "retention parity for {name}");
    retained
}

#[test]
fn a_standard_assert_nesting_assert_eq_is_not_a_named_wrapper() {
    // Fixture admission: the nested file holds the shape, and the union scan
    // reports no site for it.
    assert!(NESTED_STANDARD.contains("assert!(check(|| {\n        assert_eq!(1, 1);"));
    let names = assertion_context_macros();
    assert!(
        trusted_macro_binding_sites(
            NESTED_STANDARD,
            &BTreeSet::new(),
            &names,
            &|_, _| false,
            &|_| false
        )
        .is_empty(),
        "the admission scan reports no site for a standard nested assertion"
    );
    let files = [(NESTED, NESTED_STANDARD), (LIB, GATE), (TESTS, PINS)];
    assert_eq!(named_site(&files, "assert_eq"), None);
    assert_eq!(named_site(&files, "assert"), None);
}

#[test]
fn the_named_site_is_the_glob_that_blocks_not_an_earlier_standard_call() {
    let files = [
        (NESTED, NESTED_STANDARD),
        (GLOB, "pub use foreign::fmt::*;\n"),
        (LIB, GATE),
        (TESTS, PINS),
    ];
    let named = named_site(&files, "assert_eq");
    assert!(
        matches!(&named, Some((path, site)) if path == Path::new(GLOB)
            && site.line == 1
            && matches!(&site.kind, MacroBindingKind::ForeignGlob(import) if import == "pub use foreign::fmt::*;")),
        "{named:?}"
    );
}

#[test]
fn a_variable_named_macro_use_in_a_standard_assert_is_not_an_attribute() {
    let source = "pub fn admitted(source: &str) -> Vec<u8> {\n    source.as_bytes().to_vec()\n}\n\n#[test]\nfn reads() {\n    let macro_use = \"x\";\n    assert!(admitted(macro_use).is_empty(), \"{macro_use}\");\n}\n";
    let files = [(NESTED, source), (LIB, GATE), (TESTS, PINS)];
    for name in ["assert_eq", "assert", "matches"] {
        assert_eq!(named_site(&files, name), None, "{name}");
    }
}

#[test]
fn a_rebound_standard_wrapper_is_still_named() {
    // `assert` has a definition in the file, so its arguments are only
    // tokens: the expansion may define `assert_eq` (#5941).
    let source = "macro_rules! assert {\n    ($($tokens:tt)*) => {};\n}\n\npub fn noise() {\n    assert!(assert_eq!(1, 1));\n}\n";
    let files = [(NESTED, source), (LIB, GATE), (TESTS, PINS)];
    let named = named_site(&files, "assert_eq");
    assert!(
        matches!(&named, Some((path, site)) if path == Path::new(NESTED)
            && site.line == 6
            && site.kind == MacroBindingKind::MacroArgument("assert".to_string())),
        "{named:?}"
    );
}

#[test]
fn an_unknown_wrapper_is_still_named() {
    let source = "pub fn noise() {\n    wrap!(assert_eq!(1, 1));\n}\n";
    let files = [(NESTED, source), (LIB, GATE), (TESTS, PINS)];
    let named = named_site(&files, "assert_eq");
    assert!(
        matches!(&named, Some((path, site)) if path == Path::new(NESTED)
            && site.line == 2
            && site.kind == MacroBindingKind::MacroArgument("wrap".to_string())),
        "{named:?}"
    );
    // `stringify!` is itself a trusted name, so the per-name query reads it
    // as the union does: a standard macro, not a wrapper.
    assert!(assertion_context_macros().contains(&"stringify"));
    let source = "pub fn noise() {\n    let _ = stringify!(assert_eq);\n}\n";
    let files = [(NESTED, source), (LIB, GATE), (TESTS, PINS)];
    assert_eq!(named_site(&files, "assert_eq"), None);
}
