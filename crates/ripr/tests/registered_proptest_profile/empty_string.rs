//! #10843: the unchanged real property through the ordinary CLI and provider.
use super::*;

const ORIGINAL: &str =
    include_str!("../fixtures/proptest_empty_string/input/tests/prop_utf16_roundtrip.rs");
const CONVERTER: &str = include_str!("../fixtures/proptest_empty_string/input/src/convert.rs");
const LIBRARY: &str = include_str!("../fixtures/proptest_empty_string/input/src/lib.rs");
const GENERATOR: &str =
    include_str!("../fixtures/proptest_empty_string/input/perl-test-generators/src/lib.rs");
const REAL_MANIFEST: &str = include_str!("../fixtures/proptest_empty_string/input/Cargo.toml");
const GENERATOR_MANIFEST: &str =
    include_str!("../fixtures/proptest_empty_string/input/perl-test-generators/Cargo.toml");
const REAL_LOCK: &str = include_str!("../fixtures/proptest_empty_string/input/Cargo.lock");
const PROPERTY_PATH: &str = "tests/prop_utf16_roundtrip.rs";

fn fixture() -> Result<Fixture, String> {
    let fixture = Fixture::new()?;
    std::fs::remove_file(fixture.0.join("tests/boundary.rs")).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(fixture.0.join("perl-test-generators/src"))
        .map_err(|error| error.to_string())?;
    for (path, source) in [
        ("Cargo.toml", REAL_MANIFEST),
        ("Cargo.lock", REAL_LOCK),
        ("src/lib.rs", LIBRARY),
        ("src/convert.rs", CONVERTER),
        (PROPERTY_PATH, ORIGINAL),
        ("perl-test-generators/Cargo.toml", GENERATOR_MANIFEST),
        ("perl-test-generators/src/lib.rs", GENERATOR),
        (
            "tests/ordinary.rs",
            "#[test]\nfn ordinary_witness() { assert_eq!(perl_position_tracking::ordinary_total(2), 3); }\n",
        ),
    ] {
        fixture.write(path, source)?;
    }
    let line = CONVERTER
        .lines()
        .position(|line| line == "    text.len()")
        .ok_or("missing actual inverse tail")?
        + 1;
    fixture.write("diff.patch", &format!("--- a/src/convert.rs\n+++ b/src/convert.rs\n@@ -{line},1 +{line},1 @@\n-    text.len().saturating_add(1)\n+    text.len()\n"))?;
    Ok(fixture)
}

fn exact_native(fixture: &Fixture, label: &str) -> Result<Output, String> {
    run(
        fixture,
        OsStr::new("cargo"),
        &[
            OsStr::new("test"),
            OsStr::new("--locked"),
            OsStr::new("--offline"),
            OsStr::new("--test"),
            OsStr::new("prop_utf16_roundtrip"),
            OsStr::new("empty_string"),
            OsStr::new("--"),
            OsStr::new("--exact"),
            OsStr::new("--nocapture"),
        ],
        label,
        None,
        false,
        None,
    )
}

fn empty_related(report: &Value) -> Vec<&Value> {
    report["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|finding| finding["related_tests"].as_array().into_iter().flatten())
        .filter(|test| test["name"] == "empty_string")
        .collect()
}

fn has_empty_input(report: &Value) -> bool {
    report["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|finding| {
            finding["activation"]["observed_values"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .any(|fact| {
            fact["line"] == 157
                && fact["value"] == "text = \"\""
                && fact["context"] == "function_argument"
        })
}

fn has_independent_ordinary_witness(report: &Value) -> bool {
    report["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|finding| finding["related_tests"].as_array().into_iter().flatten())
        .any(|test| {
            test["name"] == "crlf_excludes_only_the_cr_paired_with_lf"
                && test["file"] == "src/convert.rs"
        })
}

#[test]
fn original_empty_string_has_nonzero_native_and_cli_owner_evidence() -> Result<(), String> {
    let fixture = fixture()?;
    let correct = exact_native(&fixture, "empty-native-correct")?;
    assert!(
        correct.status.success(),
        "native setup/correct failure: {}\n{}",
        String::from_utf8_lossy(&correct.stdout),
        String::from_utf8_lossy(&correct.stderr)
    );
    assert!(String::from_utf8_lossy(&correct.stdout).contains("1 passed; 0 failed"));
    assert!(String::from_utf8_lossy(&correct.stdout).contains("5 filtered out"));
    let wrong_source =
        CONVERTER.replace("    text.len()\n}", "    text.len().saturating_add(1)\n}");
    assert_ne!(wrong_source, CONVERTER);
    fixture.write("src/convert.rs", &wrong_source)?;
    let wrong = exact_native(&fixture, "empty-native-wrong")?;
    let wrong_text = format!(
        "{}{}",
        String::from_utf8_lossy(&wrong.stdout),
        String::from_utf8_lossy(&wrong.stderr)
    );
    assert_eq!(wrong.status.code(), Some(101), "{wrong_text}");
    assert!(
        wrong_text.contains("0 passed; 1 failed"),
        "setup/compiler failure is not discrimination: {wrong_text}"
    );
    let compact = wrong_text.split_whitespace().collect::<String>();
    assert!(
        compact.contains("left:`1`")
            && compact.contains("right:`0`")
            && compact.contains("minimalfailinginput:s=\"\""),
        "wrong result must fail the unchanged property: {wrong_text}"
    );
    fixture.write("src/convert.rs", CONVERTER)?;
    let restored = exact_native(&fixture, "empty-native-restored")?;
    assert!(restored.status.success());
    assert_eq!(
        std::fs::read_to_string(fixture.0.join(PROPERTY_PATH)).map_err(|error| error.to_string())?,
        ORIGINAL
    );

    let (cold, _) = report(&fixture, "empty-cli-cold", None, None)?;
    assert!(
        !cold["findings"]
            .as_array()
            .ok_or("missing changed converter findings")?
            .is_empty(),
        "{cold}"
    );
    assert!(
        !empty_related(&cold).is_empty(),
        "the original empty_string must reach the actual inverse converter: {cold}"
    );
    assert!(
        has_empty_input(&cold),
        "the original borrowed String must retain its exact empty input: {cold}"
    );
    assert!(has_independent_ordinary_witness(&cold), "{cold}");
    let (warm, _) = report(&fixture, "empty-cli-warm", None, None)?;
    assert_eq!(warm["findings"], cold["findings"]);
    let (zero, _) = report(&fixture, "empty-cli-zero", Some("0"), None)?;
    assert!(
        empty_related(&zero).is_empty(),
        "zero cases must revoke the property: {zero}"
    );
    assert!(
        !has_empty_input(&zero),
        "zero cases revoke the input: {zero}"
    );
    assert!(has_independent_ordinary_witness(&zero), "{zero}");
    let (restored, _) = report(&fixture, "empty-cli-restored", None, None)?;
    assert_eq!(restored["findings"], cold["findings"]);
    std::fs::create_dir(fixture.0.join(".cargo")).map_err(|error| error.to_string())?;
    fixture.write(".cargo/config.toml", "[env]\nPROPTEST_CASES='0'\n")?;
    let (configured, _) = report(&fixture, "empty-cli-configured", None, None)?;
    assert!(empty_related(&configured).is_empty(), "{configured}");
    std::fs::remove_file(fixture.0.join(".cargo/config.toml"))
        .map_err(|error| error.to_string())?;
    fixture.write("Cargo.toml", &format!("{REAL_MANIFEST}\n[[test]]\nname='prop_utf16_roundtrip'\npath='{PROPERTY_PATH}'\nharness=false\n"))?;
    let (inactive, _) = report(&fixture, "empty-cli-target-disabled", None, None)?;
    assert!(empty_related(&inactive).is_empty(), "{inactive}");
    fixture.write("Cargo.toml", REAL_MANIFEST)?;
    let (restored, _) = report(&fixture, "empty-cli-target-restored", None, None)?;
    assert_eq!(restored["findings"], cold["findings"]);
    Ok(())
}

#[test]
fn original_empty_string_rejects_shadowed_discarded_and_rewritten_evidence() -> Result<(), String> {
    let fixture = fixture()?;
    let (positive, _) = report(&fixture, "empty-negative-control", None, None)?;
    assert!(
        !empty_related(&positive).is_empty(),
        "positive setup: {positive}"
    );
    assert!(
        has_empty_input(&positive),
        "positive input setup: {positive}"
    );
    assert!(has_independent_ordinary_witness(&positive), "{positive}");
    for (label, from, to) in [
        (
            "foreign-glob",
            "use proptest::prelude::*;",
            "use proptest::prelude::*; use other::*;",
        ),
        (
            "string-shadow",
            "use proptest::prelude::*;",
            "use proptest::prelude::*; type String = std::string::String;",
        ),
        (
            "raw-string-import",
            "use proptest::prelude::*;",
            "use proptest::prelude::*; use other::r#String;",
        ),
        (
            "transitive-concat-import",
            "use proptest::prelude::*;",
            "use proptest::prelude::*; use other::concat;",
        ),
        (
            "transitive-file-import",
            "use proptest::prelude::*;",
            "use proptest::prelude::*; use other::file;",
        ),
        (
            "just-shadow",
            "use proptest::prelude::*;",
            "use proptest::prelude::*; #[allow(non_snake_case)] fn Just(_: String) -> proptest::strategy::Just<String> { proptest::strategy::Just(String::from(\"x\")) }",
        ),
        (
            "assertion-shadow",
            "use proptest::prelude::*;",
            "use proptest::prelude::*; macro_rules! prop_assert_eq { ($($tokens:tt)*) => {}; }",
        ),
        (
            "owner-shadow",
            "use perl_position_tracking::{offset_to_utf16_line_col, utf16_line_col_to_offset};",
            "use perl_position_tracking::offset_to_utf16_line_col; fn utf16_line_col_to_offset(_: &str, _: u32, _: u32) -> usize { 0 }",
        ),
        (
            "discarded-result",
            "let back = utf16_line_col_to_offset(&s, line, col);",
            "let back = 0;",
        ),
        (
            "rewritten-result",
            "prop_assert_eq!(back, 0);",
            "let back = 0; prop_assert_eq!(back, 0);",
        ),
        (
            "wrong-tuple-flow",
            "prop_assert_eq!((line, col), (0, 0));",
            "prop_assert_eq!((col, line), (0, 0));",
        ),
        (
            "ignored-property",
            "fn empty_string(s in",
            "#[ignore]\n    fn empty_string(s in",
        ),
        (
            "sibling-attribute",
            "fn roundtrip_offset_to_utf16_and_back(",
            "#[unknown_attribute]\n    fn roundtrip_offset_to_utf16_and_back(",
        ),
        (
            "sibling-owner-shadow",
            "fn roundtrip_offset_to_utf16_and_back(",
            "fn utf16_line_col_to_offset(",
        ),
        (
            "sibling-macro-export",
            "fn char_boundaries_roundtrip_with_separator_normalization(s in source_text()) {",
            "fn char_boundaries_roundtrip_with_separator_normalization(s in source_text()) { #[macro_export] macro_rules! prop_assert_eq { ($($tokens:tt)*) => { () }; }",
        ),
        (
            "strategy-macro-export",
            "fn char_boundaries_roundtrip_with_separator_normalization(s in source_text()) {",
            "fn char_boundaries_roundtrip_with_separator_normalization(s in { #[macro_export] macro_rules! prop_assert_eq { ($($tokens:tt)*) => { () }; } source_text() }) {",
        ),
        (
            "indirect-emission",
            "fn char_boundaries_roundtrip_with_separator_normalization(s in source_text()) {",
            "fn char_boundaries_roundtrip_with_separator_normalization(s in source_text()) { perl_test_generators::emit_shadow!($);",
        ),
        (
            "local-macro-import",
            "fn char_boundaries_roundtrip_with_separator_normalization(s in source_text()) {",
            "fn char_boundaries_roundtrip_with_separator_normalization(s in source_text()) { use perl_test_generators::emit_shadow as prop_assert_eq; prop_assert_eq!($);",
        ),
        (
            "helper-nested-emission",
            "fn source_text() -> impl Strategy<Value = String> {",
            "fn source_text() -> impl Strategy<Value = String> { let _ = vec![perl_test_generators::emit_shadow!($)];",
        ),
        (
            "helper-nested-proptest",
            "fn source_text() -> impl Strategy<Value = String> {",
            "fn source_text() -> impl Strategy<Value = String> { proptest::proptest! { #[test] fn emit(n in proptest::strategy::Just(0u8)) { #[macro_export] macro_rules! prop_assert_eq { ($($t:tt)*) => { () }; } } }",
        ),
        (
            "module-nested-proptest",
            "fn source_text() -> impl Strategy<Value = String> {",
            "fn source_text() -> impl Strategy<Value = String> { mod hidden { proptest::proptest! { #[test] fn emit(n in proptest::strategy::Just(0u8)) { #[macro_export] macro_rules! prop_assert_eq { ($($t:tt)*) => { () }; } } } }",
        ),
    ] {
        if matches!(
            label,
            "indirect-emission" | "local-macro-import" | "helper-nested-emission"
        ) {
            fixture.write(
                "perl-test-generators/src/lib.rs",
                &format!("{GENERATOR}\n#[macro_export] macro_rules! emit_shadow {{ ($d:tt) => {{ #[macro_export] macro_rules! prop_assert_eq {{ ($d($d tokens:tt)*) => {{ () }}; }} }}; }}\n"),
            )?;
        }
        let changed = ORIGINAL.replace(from, to);
        assert_ne!(changed, ORIGINAL, "{label}");
        fixture.write(PROPERTY_PATH, &changed)?;
        let (result, _) = report(&fixture, &format!("empty-{label}"), None, None)?;
        assert!(empty_related(&result).is_empty(), "{label}: {result}");
        assert!(!has_empty_input(&result), "{label} input: {result}");
        assert!(
            has_independent_ordinary_witness(&result),
            "{label} must preserve the independent ordinary witness: {result}"
        );
        fixture.write("perl-test-generators/src/lib.rs", GENERATOR)?;
    }
    fixture.write(PROPERTY_PATH, ORIGINAL)?;
    fixture.write("src/lib.rs", &LIBRARY.replace("pub use convert::{offset_to_utf16_line_col, utf16_line_col_to_offset};", "pub use convert::offset_to_utf16_line_col; pub use substitute::utf16_line_col_to_offset; mod substitute { pub fn utf16_line_col_to_offset(_: &str, _: u32, _: u32) -> usize { 0 } }"))?;
    let (substitute, _) = report(&fixture, "empty-reexport-substitute", None, None)?;
    assert!(
        empty_related(&substitute).is_empty(),
        "a different re-export cannot borrow the converter: {substitute}"
    );
    assert!(!has_empty_input(&substitute), "{substitute}");
    for (label, from, to) in [
        ("absolute-child", "pub use convert::", "pub use ::convert::"),
        (
            "renamed-child",
            "pub use convert::",
            "use convert as inner; pub use inner::",
        ),
        (
            "ambiguous-child",
            "mod convert;",
            "mod convert; #[path=\"alternate.rs\"] mod convert;",
        ),
    ] {
        if label == "ambiguous-child" {
            fixture.write("src/alternate.rs", CONVERTER)?;
        }
        let changed = LIBRARY.replace(from, to);
        assert_ne!(changed, LIBRARY, "{label}");
        fixture.write("src/lib.rs", &changed)?;
        let (result, _) = report(&fixture, &format!("empty-{label}"), None, None)?;
        assert!(empty_related(&result).is_empty(), "{label}: {result}");
        assert!(!has_empty_input(&result), "{label} input: {result}");
        if label == "ambiguous-child" {
            std::fs::remove_file(fixture.0.join("src/alternate.rs"))
                .map_err(|error| error.to_string())?;
        }
    }
    fixture.write("src/lib.rs", LIBRARY)?;
    let (restored, _) = report(&fixture, "empty-negatives-restored", None, None)?;
    assert_eq!(restored["findings"], positive["findings"]);
    Ok(())
}

#[test]
fn original_empty_string_refuses_consumer_standard_crate_substitution() -> Result<(), String> {
    let fixture = fixture()?;
    let (positive, _) = report(&fixture, "empty-std-before", None, None)?;
    assert!(!empty_related(&positive).is_empty(), "{positive}");
    assert!(has_empty_input(&positive), "{positive}");
    const SUBSTITUTE: &str =
        "[package]\nname='profile-stdlib-substitute'\nversion='0.1.0'\nedition='2021'\n";
    std::fs::create_dir_all(fixture.0.join("stdlib-substitute/src"))
        .map_err(|error| error.to_string())?;
    fixture.write("stdlib-substitute/src/lib.rs", "pub fn marker() {}\n")?;
    let substituted_lock = format!(
        "{}\n[[package]]\nname = \"profile-stdlib-substitute\"\nversion = \"0.1.0\"\n",
        REAL_LOCK.replace(
            " \"perl-test-generators\",\n \"proptest\",",
            " \"perl-test-generators\",\n \"profile-stdlib-substitute\",\n \"proptest\","
        )
    );
    for name in ["std", "core", "alloc"] {
        for form in ["renamed-dependency", "dependency-library", "own-library"] {
            let label = format!("empty-{name}-{form}");
            let (manifest, substitute_manifest) = match form {
                "renamed-dependency" => (
                    format!(
                        "{REAL_MANIFEST}{name} = {{ package = 'profile-stdlib-substitute', path = 'stdlib-substitute' }}\n"
                    ),
                    SUBSTITUTE.to_string(),
                ),
                "dependency-library" => (
                    format!(
                        "{REAL_MANIFEST}profile-stdlib-substitute = {{ path = 'stdlib-substitute' }}\n"
                    ),
                    format!("{SUBSTITUTE}\n[lib]\nname='{name}'\n"),
                ),
                _ => (
                    format!("{REAL_MANIFEST}\n[lib]\nname='{name}'\n"),
                    SUBSTITUTE.to_string(),
                ),
            };
            fixture.write("Cargo.toml", &manifest)?;
            fixture.write("stdlib-substitute/Cargo.toml", &substitute_manifest)?;
            fixture.write(
                "Cargo.lock",
                if form == "own-library" {
                    REAL_LOCK
                } else {
                    &substituted_lock
                },
            )?;
            // A renamed own library remains the actual converter owner. The
            // refusal must follow its extern name, not a stale import spelling.
            fixture.write(
                PROPERTY_PATH,
                &if form == "own-library" {
                    ORIGINAL.replace("perl_position_tracking::{", &format!("{name}::{{"))
                } else {
                    ORIGINAL.to_string()
                },
            )?;
            let resolved = metadata(&fixture, &format!("{label}-metadata"))?;
            let provider = package(&resolved, "proptest")?;
            assert_eq!(provider["version"], "1.11.0", "{resolved}");
            assert_eq!(
                provider["source"], "registry+https://github.com/rust-lang/crates.io-index",
                "{resolved}"
            );
            let owner = package(&resolved, "perl-position-tracking")?;
            if form == "own-library" {
                assert!(
                    owner["targets"]
                        .as_array()
                        .ok_or("missing targets")?
                        .iter()
                        .any(|target| {
                            target["name"] == name && target["kind"] == serde_json::json!(["lib"])
                        }),
                    "{resolved}"
                );
            } else {
                let node = resolved["resolve"]["nodes"]
                    .as_array()
                    .ok_or("missing nodes")?
                    .iter()
                    .find(|node| node["id"] == owner["id"])
                    .ok_or("missing consumer node")?;
                assert!(
                    node["deps"]
                        .as_array()
                        .ok_or("missing edges")?
                        .iter()
                        .any(|edge| edge["name"] == name),
                    "{resolved}"
                );
                let declaration = owner["dependencies"]
                    .as_array()
                    .ok_or("missing declarations")?
                    .iter()
                    .find(|dependency| dependency["name"] == "profile-stdlib-substitute")
                    .ok_or("missing substitute declaration")?;
                if form == "renamed-dependency" {
                    assert_eq!(declaration["rename"], name, "{resolved}");
                } else {
                    assert!(declaration["rename"].is_null(), "{resolved}");
                }
            }
            let (refused, _) = report(&fixture, &label, None, None)?;
            assert!(empty_related(&refused).is_empty(), "{label}: {refused}");
            assert!(!has_empty_input(&refused), "{label}: {refused}");
            assert!(
                has_independent_ordinary_witness(&refused),
                "{label}: {refused}"
            );
        }
    }
    fixture.write("Cargo.toml", REAL_MANIFEST)?;
    fixture.write("Cargo.lock", REAL_LOCK)?;
    fixture.write(PROPERTY_PATH, ORIGINAL)?;
    let (restored, _) = report(&fixture, "empty-std-restored", None, None)?;
    assert_eq!(restored["findings"], positive["findings"]);
    Ok(())
}
