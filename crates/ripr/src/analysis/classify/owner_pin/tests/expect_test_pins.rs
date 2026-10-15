//! #6578: an inline expect-test comparison,
//! `expect![["text"]].assert_eq(&owner(..))`, pins the owner's return as
//! `assert_eq!(owner(..), "text")` does, behind a manifest-verified
//! `use expect_test::expect;`; a golden-file
//! `expect_file!["path"].assert_eq(&owner(..))` pins behind a verified
//! `use expect_test::expect_file;`. Every other expect-test spelling stays a
//! `snapshot` / medium oracle (RIPR-SPEC-0231 step 6).

use super::*;
use crate::analysis::facts::drop_in::{DropInManifests, temp_workspace};
use crate::analysis::syntax::parser_oracles_for_function;
use crate::domain::{OracleKind, OracleStrength};

const RENDER_LIB: &str = "pub fn render(x: u32) -> String {\n    format!(\"w{}\", x * 3)\n}\n";

fn manifest(dependency: &str) -> String {
    format!("[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dev-dependencies]\n{dependency}\n")
}

/// The admitted assertions of the one test in `tests`, read under the
/// manifest at `root` (none: no manifest verifies the import).
fn render_admitted(tests: &str, root: Option<&Path>) -> Vec<String> {
    let mut index = index(&[(LIB, RENDER_LIB), (TESTS, tests)]);
    if let Some(root) = root {
        index.drop_in_manifests = DropInManifests::new(root);
    }
    let pin = establish(&index, "render", "format!(\"w{}\", x * 3)");
    assert!(pin.is_some(), "the format owner must establish a pin");
    let Some(pin) = pin else {
        return Vec::new();
    };
    admitted_texts(&index, &pin)
}

fn render_test(imports: &str, body: &str) -> String {
    format!("use demo::render;\n{imports}\n#[test]\nfn renders() {{\n    {body}\n}}\n")
}

#[test]
fn an_inline_expect_comparison_pins_behind_a_verified_import() -> Result<(), String> {
    let plain = temp_workspace(
        "expect-test-plain",
        &[("Cargo.toml", &manifest("expect-test = \"1\""))],
    )?;
    let aliased = temp_workspace(
        "expect-test-aliased",
        &[(
            "Cargo.toml",
            &manifest("expect-test = { package = \"fake-expect\", version = \"1\" }"),
        )],
    )?;
    let import = "use expect_test::expect;";
    let mut outcomes = Vec::new();
    let grouped = render_test(
        "use expect_test::{expect, expect_file};",
        "expect![[\"w12\"]].assert_eq(&render(4));",
    );
    let grouped = render_admitted(&grouped, Some(&plain)).len();
    for (import, body) in [
        (import, "expect![[\"w12\"]].assert_eq(&render(4));"),
        (import, "expect![\"w12\"].assert_eq(&render(4));"),
        (import, "expect![[\"w12\"]].assert_eq(render(4).as_ref());"),
        // #6578: the golden-file comparison pins the same way.
        (
            "use expect_test::expect_file;",
            "expect_file![\"w.txt\"].assert_eq(&render(4));",
        ),
    ] {
        let tests = render_test(import, body);
        outcomes.push((
            body,
            render_admitted(&tests, Some(&plain)).len(),
            render_admitted(&tests, Some(&aliased)).len(),
            render_admitted(&tests, None).len(),
        ));
    }
    let mut refused = Vec::new();
    for (imports, body) in [
        // The comparison's other spellings stay snapshots.
        (import, "expect![[\"w12\"]].assert_debug_eq(&render(4));"),
        (
            "use expect_test::expect_file;",
            "expect_file![\"w.txt\"].assert_debug_eq(&render(4));",
        ),
        ("", "expect_test::expect![[\"w12\"]].assert_eq(&render(4));"),
        ("", "expect_test::expect_file![\"w.txt\"].assert_eq(&render(4));"),
        // A computed or non-literal file path.
        (
            "use expect_test::expect_file;",
            "expect_file![concat!(\"w\", \".txt\")].assert_eq(&render(4));",
        ),
        (
            "use expect_test::expect_file;",
            "expect_file![[\"w.txt\"]].assert_eq(&render(4));",
        ),
        // `expect_file!` imported from somewhere else.
        (
            "use other::expect_file;",
            "expect_file![\"w.txt\"].assert_eq(&render(4));",
        ),
        // A computed expectation, or none of the owner's value.
        (
            import,
            "expect![[concat!(\"w\", \"12\")]].assert_eq(&render(4));",
        ),
        (import, "expect![[\"w12\"]].assert_eq(&other(4));"),
        // An `expect!` some other import or macro binds.
        (
            "use other::expect;",
            "expect![[\"w12\"]].assert_eq(&render(4));",
        ),
        (
            "use pretty_assertions::expect;",
            "expect![[\"w12\"]].assert_eq(&render(4));",
        ),
        // expect-test exports no drop-in `assert_eq!`.
        (
            "use expect_test::assert_eq;",
            "assert_eq!(render(4), \"w12\");",
        ),
        (
            "macro_rules! expect { ($($t:tt)*) => { Lenient }; }",
            "expect![[\"w12\"]].assert_eq(&render(4));",
        ),
    ] {
        let tests = render_test(imports, body);
        refused.push((tests.clone(), render_admitted(&tests, Some(&plain))));
    }
    let should_panic = render_test(import, "expect![[\"w12\"]].assert_eq(&render(4));")
        .replace("#[test]\n", "#[test]\n#[should_panic]\n");
    let should_panic = render_admitted(&should_panic, Some(&plain));
    let _ = std::fs::remove_dir_all(&plain);
    let _ = std::fs::remove_dir_all(&aliased);
    assert_eq!(grouped, 1);
    for (body, under_plain, under_alias, unverified) in outcomes {
        let expected_plain = usize::from(!body.contains("as_ref"));
        assert_eq!(under_plain, expected_plain, "{body}");
        assert_eq!((under_alias, unverified), (0, 0), "{body}");
    }
    for (tests, admitted) in refused {
        assert!(admitted.is_empty(), "{tests}: {admitted:?}");
    }
    assert!(should_panic.is_empty());
    Ok(())
}

#[test]
fn inline_expect_operands_reads_one_literal_and_one_argument() {
    let operands = |text: &str| inline_expect_operands(text);
    let pair = |actual: &str, expected: &str| Some([actual.to_string(), expected.to_string()]);
    assert_eq!(
        operands("expect![[\"w12\"]].assert_eq(&render(4));"),
        pair("render(4)", "\"w12\"")
    );
    assert_eq!(
        operands("expect![\"w12\"].assert_eq(render(4))"),
        pair("render(4)", "\"w12\"")
    );
    assert_eq!(
        operands("expect![[r#\"w12\"#]].assert_eq(&render(4));"),
        pair("render(4)", "r#\"w12\"#")
    );
    // The empty expectation compares with "".
    assert_eq!(
        operands("expect![[]].assert_eq(&render(4));"),
        pair("render(4)", "\"\"")
    );
    // A golden file's operand is its path literal, standing in for the
    // committed text.
    assert_eq!(
        operands("expect_file![\"w.txt\"].assert_eq(&render(4));"),
        pair("render(4)", "\"w.txt\"")
    );
    for text in [
        "expect![[\"w12\"]].assert_debug_eq(&render(4));",
        "expect![[\"w12\"]].assert_eq::<str>(&render(4));",
        "expect_test::expect![[\"w12\"]].assert_eq(&render(4));",
        "expect_test::expect_file![\"w.txt\"].assert_eq(&render(4));",
        "expect_file![\"w.txt\"].assert_debug_eq(&render(4));",
        "expect_file![[\"w.txt\"]].assert_eq(&render(4));",
        "expect_file![w_txt].assert_eq(&render(4));",
        "expect_file![\"w\", \"txt\"].assert_eq(&render(4));",
        "expect![[w12]].assert_eq(&render(4));",
        "expect![[\"w\", \"12\"]].assert_eq(&render(4));",
        "expect![[\"w12\"]].assert_eq(&mut render(4));",
        // expect-test trims a multi-line literal's indentation first.
        "expect![[r#\"\n    w12\n\"#]].assert_eq(&render(4));",
        "expect![[\"w12\"]].assert_eq(&render(4), extra);",
        "expect![[\"w12\"]].assert_eq(&render(4)); other();",
        "lenient![[\"w12\"]].assert_eq(&render(4));",
    ] {
        assert_eq!(operands(text), None, "{text}");
    }
}

/// The comparison is a method call on the macro's value, so the macro
/// walk alone never extracts it; a multi-line expectation is still one
/// oracle, not a second line-scanned `.assert_debug_eq(..)`.
#[test]
fn expect_test_comparisons_extract_as_one_snapshot_oracle() {
    let function = "fn renders() {\n    expect![[\"w12\"]].assert_eq(&render(4));\n    expect![[r#\"\n        Row { w: 12 }\n    \"#]]\n    .assert_debug_eq(&row(4));\n    expect_file![\"w.txt\"].assert_eq(&render(4));\n    expect![\"w12\"].assert_eq(&render(4));\n    expect! [[\"w12\"]].assert_eq (&render(4));\n}\n";
    let oracles = parser_oracles_for_function(function, 1).unwrap_or_default();
    let read: Vec<(usize, OracleKind, OracleStrength)> = oracles
        .iter()
        .map(|oracle| (oracle.line, oracle.kind.clone(), oracle.strength.clone()))
        .collect();
    assert_eq!(
        read,
        vec![
            (2, OracleKind::Snapshot, OracleStrength::Medium),
            (3, OracleKind::Snapshot, OracleStrength::Medium),
            (7, OracleKind::Snapshot, OracleStrength::Medium),
            // The single-bracket and spaced spellings grade the same
            // (#7856 review).
            (8, OracleKind::Snapshot, OracleStrength::Medium),
            (9, OracleKind::Snapshot, OracleStrength::Medium),
        ],
        "{oracles:?}"
    );
    assert!(
        oracles[0].text.starts_with("expect![[\"w12\"]].assert_eq("),
        "{oracles:?}"
    );
}

/// #6578 review: `expect!` is trusted only where a test's verified
/// `use expect_test::expect;` binds it. In an owner body it is a crate's
/// own macro, which may hide a `return` before the changed tail.
#[test]
fn an_owner_body_expect_macro_keeps_its_return_paths_open() {
    let try_style = "macro_rules! {name} {\n    ($e:expr) => {\n        match $e {\n            Some(v) => v,\n            None => return 0,\n        }\n    };\n}\n";
    let admitted = |lib: &str| {
        let tests =
            "use demo::score;\n#[test]\nfn scores() {\n    assert_eq!(score(None), 0);\n}\n";
        let index = index(&[(LIB, lib), (TESTS, tests)]);
        let owner = owner(&index, "score");
        // The changed tail sits on the owner's third line.
        let mut probe = return_probe(owner, "v + 1");
        probe.location = SourceLocation::new(owner.file.clone(), owner.start_line + 2, 1);
        OwnerReturnPin::establish(&probe, owner, &index)
            .map(|pin| admitted_texts(&index, &pin))
            .unwrap_or_default()
    };
    let score = |call: &str| {
        format!(
            "{}pub fn score(x: Option<u32>) -> u32 {{\n    let v = {call};\n    v + 1\n}}\n",
            try_style.replace("{name}", "expect")
        )
    };
    // `score(None)` leaves through the macro's hidden `return 0`.
    assert!(admitted(&score("expect!(x)")).is_empty());
    // The control: a standard non-returning macro keeps the pin.
    assert_eq!(admitted(&score("dbg!(x).unwrap_or(0)")).len(), 1);
}

/// #7856 review: the parsed `snapshot` / medium grade is a ceiling. A
/// comparison whose compared value is a tautology or a broad `is_err()`
/// keeps the weak reading the line scan gives it.
#[test]
fn a_weak_expect_comparison_keeps_its_weak_grade() {
    let function = "fn renders() {\n    expect![[\"true\"]].assert_eq(&(r.is_ok() || r.is_err()).to_string());\n    expect![[\"true\"]].assert_eq(&render(1).is_err().to_string());\n}\n";
    let oracles = parser_oracles_for_function(function, 1).unwrap_or_default();
    let expect_oracles: Vec<_> = oracles
        .iter()
        .filter(|oracle| oracle.text.starts_with("expect!"))
        .collect();
    assert_eq!(expect_oracles.len(), 2, "{oracles:?}");
    for oracle in expect_oracles {
        assert_eq!(oracle.strength, OracleStrength::Weak, "{oracle:?}");
        assert_ne!(oracle.kind, OracleKind::Snapshot, "{oracle:?}");
    }
}

/// A comparison nested under `||` may never run, so only a statement or
/// tail of the function's own body is extracted as a parser oracle; the
/// line scan keeps reading the nested line as it did before.
#[test]
fn a_nested_expect_comparison_is_not_its_own_snapshot() {
    let function = "fn renders() {\n    let _ok = ready() || { expect![[\"1\"]].assert_eq(&render(4)); true };\n}\n";
    let oracles = parser_oracles_for_function(function, 1).unwrap_or_default();
    assert!(
        oracles
            .iter()
            .all(|oracle| !oracle.text.starts_with("expect![[")),
        "{oracles:?}"
    );
}
