//! Generator spellings, providers and modules for RIPR-SPEC-0153 (#11545).
//!
//! Each fixture changes one `wrapping_add(2)` line through the public check
//! pipeline. A verified generator reads `static_unknown` with the stimulus
//! explanation; every unsupported spelling keeps its product reading.
#![cfg(feature = "lang-rust")]

#[path = "../src/testing/compiler_environment.rs"]
mod compiler_environment;

use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, check_workspace, process_owner};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const QUICKCHECK: &str = "quickcheck = '1'";
const PROPTEST: &str = "proptest = '1'";
const ARBITRARY: &str = "arbitrary = { version = '1', features = ['derive'] }";
const STIMULUS: &str = "test-input generator changes test stimulus";

const PRODUCT: &str = "#[derive(Clone, Debug)]\npub struct Pair(pub u8, pub u8);\n\npub fn gap(pair: &Pair) -> u8 {\n    pair.1.wrapping_sub(pair.0)\n}\n";

fn quickcheck_impl(trait_path: &str, self_ty: &str) -> String {
    format!(
        "impl {trait_path} for {self_ty} {{\n    fn arbitrary(g: &mut quickcheck::Gen) -> Self {{\n        let a = <u8 as quickcheck::Arbitrary>::arbitrary(g);\n        {self_ty}(a, a.wrapping_add(2))\n    }}\n}}\n"
    )
}

/// Root-relative fixture files and their contents.
type Files = Vec<(&'static str, String)>;

struct Fixture(PathBuf);

impl Fixture {
    fn new(dependency: &str, files: &[(&str, String)]) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-arbitrary-shapes-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let fixture = Self(root);
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            format!(
                "[workspace]\n[package]\nname = 'shapes'\nversion = '0.1.0'\nedition = '2021'\n[dependencies]\n{dependency}\n"
            ),
        )
        .map_err(|e| e.to_string())?;
        let mut patch = String::new();
        for (path, source) in files {
            let full = fixture.0.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&full, source).map_err(|e| e.to_string())?;
            if let Some((index, after)) = source
                .lines()
                .enumerate()
                .find(|(_, line)| line.contains("wrapping_add(2)"))
            {
                let line = index + 1;
                let before = after.replace("wrapping_add(2)", "wrapping_add(1)");
                patch.push_str(&format!(
                    "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -{line},1 +{line},1 @@\n-{before}\n+{after}\n"
                ));
            }
        }
        if patch.is_empty() {
            return Err("fixture has no changed line".to_string());
        }
        std::fs::write(fixture.0.join("diff.patch"), patch).map_err(|e| e.to_string())?;
        Ok(fixture)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reading {
    /// Static unknown with the stimulus explanation and no product credit.
    Generator,
    /// The ordinary product reading: never the generator explanation.
    Product,
    /// No finding: test-only source.
    Silent,
    /// Whatever ordinary analysis reports (an undeclared file has no
    /// finding), but never the generator explanation.
    NotGenerator,
}

fn assert_reading(
    label: &str,
    dependency: &str,
    files: &[(&str, String)],
    expected: Reading,
) -> Result<(), String> {
    let fixture = Fixture::new(dependency, files)?;
    let output = check_workspace(CheckInput {
        root: fixture.0.clone(),
        diff_file: Some(fixture.0.join("diff.patch")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        ..CheckInput::default()
    })?;
    let generator = |finding: &ripr::Finding| {
        finding.class == ExposureClass::StaticUnknown
            && finding.evidence.iter().any(|text| text.contains(STIMULUS))
            && finding.canonical_gap.is_none()
            && finding.related_tests.is_empty()
    };
    let summary = output
        .findings
        .iter()
        .map(|finding| format!("{:?} {:?}", finding.class, finding.evidence))
        .collect::<Vec<_>>();
    match expected {
        Reading::Generator => {
            if output.findings.is_empty() || !output.findings.iter().all(generator) {
                return Err(format!(
                    "{label}: expected generator reading, got {summary:?}"
                ));
            }
        }
        Reading::Product => {
            if output.findings.is_empty()
                || output.findings.iter().any(|finding| {
                    finding.evidence.iter().any(|text| text.contains(STIMULUS))
                        || finding.class == ExposureClass::StaticUnknown
                })
            {
                return Err(format!(
                    "{label}: expected product reading, got {summary:?}"
                ));
            }
        }
        Reading::NotGenerator => {
            if output
                .findings
                .iter()
                .any(|finding| finding.evidence.iter().any(|text| text.contains(STIMULUS)))
            {
                return Err(format!(
                    "{label}: expected no generator reading, got {summary:?}"
                ));
            }
        }
        Reading::Silent => {
            if !output.findings.is_empty() {
                return Err(format!("{label}: expected no finding, got {summary:?}"));
            }
        }
    }
    Ok(())
}

#[test]
fn imported_aliased_and_provider_spellings_read_as_stimulus() -> Result<(), String> {
    compiler_environment::plain(
        module_path!(),
        "imported_aliased_and_provider_spellings_read_as_stimulus",
        || {
            let lib = |head: &str, body: String| format!("{head}{PRODUCT}\n{body}");
            assert_reading(
                "imported",
                QUICKCHECK,
                &[(
                    "src/lib.rs",
                    lib(
                        "use quickcheck::Arbitrary;\n",
                        quickcheck_impl("Arbitrary", "Pair"),
                    ),
                )],
                Reading::Generator,
            )?;
            assert_reading(
                "aliased",
                QUICKCHECK,
                &[(
                    "src/lib.rs",
                    lib(
                        "use quickcheck::{Arbitrary as Arb, Gen};\n",
                        quickcheck_impl("Arb", "Pair"),
                    ),
                )],
                Reading::Generator,
            )?;
            assert_reading(
                "proptest",
                PROPTEST,
                &[(
                    "src/lib.rs",
                    lib(
                        "",
                        "impl proptest::arbitrary::Arbitrary for Pair {\n    type Parameters = ();\n    type Strategy = proptest::strategy::Just<Pair>;\n\n    fn arbitrary_with(_: ()) -> Self::Strategy {\n        let a = 7u8;\n        proptest::strategy::Just(Pair(a, a.wrapping_add(2)))\n    }\n}\n".to_string(),
                    ),
                )],
                Reading::Generator,
            )?;
            assert_reading(
                "arbitrary",
                ARBITRARY,
                &[(
                    "src/lib.rs",
                    lib(
                        "",
                        "impl<'a> arbitrary::Arbitrary<'a> for Pair {\n    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {\n        let a: u8 = u.arbitrary()?;\n        Ok(Pair(a, a.wrapping_add(2)))\n    }\n}\n".to_string(),
                    ),
                )],
                Reading::Generator,
            )
        },
    )
}

#[test]
fn generator_modules_read_as_stimulus_and_test_only_modules_stay_silent() -> Result<(), String> {
    compiler_environment::plain(
        module_path!(),
        "generator_modules_read_as_stimulus_and_test_only_modules_stay_silent",
        || {
            let generator = quickcheck_impl("quickcheck::Arbitrary", "crate::Pair");
            assert_reading(
                "sibling module",
                QUICKCHECK,
                &[
                    ("src/lib.rs", format!("{PRODUCT}\nmod generators;\n")),
                    ("src/generators.rs", generator.clone()),
                ],
                Reading::Generator,
            )?;
            assert_reading(
                "nested module",
                QUICKCHECK,
                &[
                    ("src/lib.rs", format!("{PRODUCT}\npub mod testing;\n")),
                    ("src/testing/mod.rs", "mod generators;\n".to_string()),
                    (
                        "src/testing/generators.rs",
                        format!(
                            "use quickcheck::Arbitrary;\nuse crate::Pair;\n\n{}",
                            quickcheck_impl("Arbitrary", "Pair")
                        ),
                    ),
                ],
                Reading::Generator,
            )?;
            assert_reading(
                "module shared by both crate roots",
                QUICKCHECK,
                &[
                    ("src/lib.rs", format!("{PRODUCT}\nmod generators;\n")),
                    (
                        "src/main.rs",
                        "mod generators;\n\nfn main() {}\n".to_string(),
                    ),
                    ("src/generators.rs", generator.clone()),
                ],
                Reading::Generator,
            )?;
            assert_reading(
                "cfg(test) module file",
                QUICKCHECK,
                &[
                    (
                        "src/lib.rs",
                        format!("{PRODUCT}\n#[cfg(test)]\nmod testing;\n"),
                    ),
                    ("src/testing.rs", generator.clone()),
                ],
                Reading::Silent,
            )?;
            assert_reading(
                "cfg(test) inline module",
                QUICKCHECK,
                &[(
                    "src/lib.rs",
                    format!(
                        "{PRODUCT}\n#[cfg(test)]\nmod tests {{\n{}}}\n",
                        quickcheck_impl("quickcheck::Arbitrary", "super::Pair")
                    ),
                )],
                Reading::Silent,
            )
        },
    )
}

#[test]
fn unsupported_spellings_and_layouts_keep_the_product_reading() -> Result<(), String> {
    compiler_environment::plain(
        module_path!(),
        "unsupported_spellings_and_layouts_keep_the_product_reading",
        || {
            let generator = quickcheck_impl("quickcheck::Arbitrary", "crate::Pair");
            let cases: Vec<(&str, &str, Files)> = vec![
                (
                    "glob import",
                    QUICKCHECK,
                    vec![(
                        "src/lib.rs",
                        format!(
                            "use quickcheck::*;\n{PRODUCT}\n{}",
                            quickcheck_impl("Arbitrary", "Pair")
                        ),
                    )],
                ),
                (
                    "import in another module scope",
                    QUICKCHECK,
                    vec![(
                        "src/lib.rs",
                        format!(
                            "use quickcheck::Arbitrary;\n{PRODUCT}\nmod inner {{\n    pub trait Arbitrary {{\n        fn arbitrary(g: &mut quickcheck::Gen) -> Self;\n    }}\n{}}}\n",
                            quickcheck_impl("Arbitrary", "super::Pair")
                        ),
                    )],
                ),
                (
                    "local trait import",
                    QUICKCHECK,
                    vec![(
                        "src/lib.rs",
                        format!(
                            "use crate::local::Arbitrary;\n{PRODUCT}\nmod local {{\n    pub trait Arbitrary {{\n        fn arbitrary(g: &mut quickcheck::Gen) -> Self;\n    }}\n}}\n{}",
                            quickcheck_impl("Arbitrary", "Pair")
                        ),
                    )],
                ),
                (
                    "other provider's trait",
                    PROPTEST,
                    vec![(
                        "src/lib.rs",
                        format!(
                            "{PRODUCT}\n{}",
                            quickcheck_impl("quickcheck::Arbitrary", "Pair")
                        ),
                    )],
                ),
                (
                    "opaque root expansion",
                    QUICKCHECK,
                    vec![
                        (
                            "src/lib.rs",
                            format!("make_provider!();\n{PRODUCT}\nmod generators;\n"),
                        ),
                        ("src/generators.rs", generator.clone()),
                    ],
                ),
                (
                    "derive in the generator's file",
                    ARBITRARY,
                    vec![(
                        "src/lib.rs",
                        format!(
                            "#[derive(arbitrary::Arbitrary)]\npub struct Other(pub u8);\n{PRODUCT}\nimpl<'a> arbitrary::Arbitrary<'a> for Pair {{\n    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {{\n        let a: u8 = u.arbitrary()?;\n        Ok(Pair(a, a.wrapping_add(2)))\n    }}\n}}\n"
                        ),
                    )],
                ),
                (
                    "derive beside a non-generator trait impl",
                    ARBITRARY,
                    vec![(
                        "src/lib.rs",
                        format!(
                            "#[derive(arbitrary::Arbitrary)]\npub struct Other(pub u8);\n{PRODUCT}\nimpl Default for Pair {{\n    fn default() -> Self {{\n        let a = 1u8;\n        Pair(a, a.wrapping_add(2))\n    }}\n}}\n"
                        ),
                    )],
                ),
                (
                    "child import rebinding the provider",
                    QUICKCHECK,
                    vec![
                        (
                            "src/lib.rs",
                            format!(
                                "{PRODUCT}\npub mod shim {{\n    pub trait Arbitrary {{}}\n}}\nmod generators;\n"
                            ),
                        ),
                        (
                            "src/generators.rs",
                            format!("use crate::shim as quickcheck;\n\n{generator}"),
                        ),
                    ],
                ),
                (
                    "module shared with a refused sibling root",
                    QUICKCHECK,
                    vec![
                        (
                            "src/lib.rs",
                            format!(
                                "extern crate self as quickcheck;\n{PRODUCT}\npub trait Arbitrary {{\n    fn arbitrary(g: &mut u8) -> Self;\n}}\nmod generators;\n"
                            ),
                        ),
                        (
                            "src/main.rs",
                            "mod generators;\n\nfn main() {}\n".to_string(),
                        ),
                        ("src/generators.rs", generator.clone()),
                    ],
                ),
            ];
            for (label, dependency, files) in &cases {
                assert_reading(label, dependency, files, Reading::Product)?;
            }
            // `#[path]` loads `other.rs`, not the conventional `generators.rs`
            // the declaration's name would select; neither borrows the role.
            assert_reading(
                "path attribute module",
                QUICKCHECK,
                &[
                    (
                        "src/lib.rs",
                        format!("{PRODUCT}\n#[path = \"other.rs\"]\nmod generators;\n"),
                    ),
                    ("src/other.rs", generator.clone()),
                    ("src/generators.rs", generator.clone()),
                ],
                Reading::NotGenerator,
            )?;
            // An undeclared file is not compiled; it must not borrow the role.
            assert_reading(
                "undeclared module file",
                QUICKCHECK,
                &[
                    (
                        "src/lib.rs",
                        format!(
                            "{PRODUCT}
pub fn other() {{}}
"
                        ),
                    ),
                    ("src/generators.rs", generator),
                ],
                Reading::NotGenerator,
            )
        },
    )
}

#[test]
fn derived_arbitrary_type_keeps_its_product_reading() -> Result<(), String> {
    compiler_environment::plain(
        module_path!(),
        "derived_arbitrary_type_keeps_its_product_reading",
        || {
            assert_reading(
                "derive(Arbitrary) product",
                ARBITRARY,
                &[(
                    "src/lib.rs",
                    "#[derive(Clone, Debug, arbitrary::Arbitrary)]\npub struct Pair(pub u8, pub u8);\n\npub fn gap(pair: &Pair) -> u8 {\n    pair.1.wrapping_sub(pair.0).wrapping_add(2)\n}\n".to_string(),
                )],
                Reading::Product,
            )
        },
    )
}

#[test]
fn compiler_wrapper_refuses_module_generator_role() -> Result<(), String> {
    compiler_environment::untrusted_wrapper(
        module_path!(),
        "compiler_wrapper_refuses_module_generator_role",
        "generator_modules_read_as_stimulus_and_test_only_modules_stay_silent",
        || {
            assert_reading(
                "wrapped sibling module",
                QUICKCHECK,
                &[
                    ("src/lib.rs", format!("{PRODUCT}\nmod generators;\n")),
                    (
                        "src/generators.rs",
                        quickcheck_impl("quickcheck::Arbitrary", "crate::Pair"),
                    ),
                ],
                Reading::Product,
            )
        },
    )
}
