//! `#[macro_use] mod NAME;` inside a registry `cfg_if::cfg_if!` (#5993).
//!
//! The item is admitted like the same item written directly when the module
//! resolves, so the standard `assert_eq!` stays credited. Every other
//! `macro_use`, `no_implicit_prelude` or unverified `cfg_if` keeps the
//! assertion refused. Each row compiles and runs the same source against the
//! correct and a deliberately wrong library.
//!
//! The registry `cfg-if` crate is not fetched: the static check reads the
//! workspace manifest (`cfg-if = "1"`), while the runtime control resolves the
//! same name to a path copy of the crate's macro, so the control needs no
//! network.

use super::{Scratch, run};
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use std::path::{Path, PathBuf};

/// The `cfg-if` 1.0 macro, as shipped.
const CFG_IF_COPY: &str = r#"
#[macro_export]
macro_rules! cfg_if {
    ($(if #[cfg($meta:meta)] { $($tokens:tt)* }) else * else { $($tokens2:tt)* }) => {
        $crate::cfg_if! { @__items () ; $( ( ($meta) ($($tokens)*) ), )* ( () ($($tokens2)*) ), }
    };
    (if #[cfg($i_met:meta)] { $($i_tokens:tt)* } $(else if #[cfg($e_met:meta)] { $($e_tokens:tt)* })*) => {
        $crate::cfg_if! { @__items () ; ( ($i_met) ($($i_tokens)*) ), $( ( ($e_met) ($($e_tokens)*) ), )* ( () () ), }
    };
    (@__items ($($not:meta,)*) ; ) => {};
    (@__items ($($not:meta,)*) ; ( ($($m:meta),*) ($($tokens:tt)*) ), $($rest:tt)*) => {
        #[cfg(all($($m,)* not(any($($not),*))))]
        $crate::cfg_if! { @__identity $($tokens)* }
        $crate::cfg_if! { @__items ($($not,)* $($m,)*) ; $($rest)* }
    };
    (@__identity $($tokens:tt)*) => { $($tokens)* };
}
"#;

/// A `cfg_if!` that defines a shadow `assert_eq!` beside the items it emits.
const SHADOWING_CFG_IF: &str = r#"
#[macro_export]
macro_rules! cfg_if {
    (if #[cfg($m:meta)] { $($t:tt)* }) => {
        macro_rules! assert_eq { ($a:expr, $b:expr) => {{ let _ = (&$a, &$b); }}; }
        $($t)*
    };
}
"#;

const UNRELATED: &str = "macro_rules! twice { ($value:expr) => { $value * 2 }; }\n";
const SHADOW: &str =
    "macro_rules! assert_eq { ($a:expr, $b:expr) => {{ let _ = (&$a, &$b); }}; }\n";
const TEST_FILE: &str =
    "use super::*;\n#[test]\nfn checks() {\n    assert_eq!(weight(4), 12);\n}\n";
const INLINE_TESTS: &str = "#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn checks() {\n        assert_eq!(weight(4), 12);\n    }\n}\n";

#[derive(Clone, Copy)]
enum CfgIf {
    /// The registry `cfg-if = "1"` the static check verifies.
    Registry,
    /// `cfg-if` renamed onto another package, which shadows `assert_eq!`.
    RenamedPackage,
}

/// What rustc does with the row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Runtime {
    /// Compiles; the wrong library fails the test.
    Catches,
    /// Compiles; the wrong library passes.
    Misses,
    /// Does not compile: E0659, the shadow is ambiguous with the prelude.
    Ambiguous,
}

struct Row {
    case: &'static str,
    /// Items placed after the production function.
    items: &'static str,
    inline_tests: bool,
    files: &'static [(&'static str, &'static str)],
    cfg_if: CfgIf,
    exposed: bool,
    runtime: Runtime,
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, text).map_err(|error| error.to_string())
}

/// Writes one workspace: the manifest, the subject and its module files.
fn workspace(
    root: &Path,
    dependency: &str,
    source: &str,
    files: &[(&str, &str)],
) -> Result<(), String> {
    write(
        &root.join("Cargo.toml"),
        &format!(
            "[package]\nname = \"owner_pin_control\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n{dependency}\n\n[workspace]\n"
        ),
    )?;
    write(&root.join("src/lib.rs"), source)?;
    for (relative, text) in files {
        write(&root.join("src").join(relative), text)?;
    }
    Ok(())
}

fn runtime_output(root: &Path, target: &Path) -> Result<(bool, String), String> {
    let cargo = PathBuf::from(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    let manifest = root.join("Cargo.toml");
    let result = run(
        &cargo,
        &[
            "test".as_ref(),
            "--offline".as_ref(),
            "--quiet".as_ref(),
            "--manifest-path".as_ref(),
            manifest.as_os_str(),
            "--target-dir".as_ref(),
            target.as_os_str(),
        ],
    )?;
    Ok((
        result.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        ),
    ))
}

#[test]
fn cfg_if_macro_use_module_admission_matches_runtime() -> Result<(), String> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/owner_return_pin_direct");
    let production = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
        .map_err(|error| error.to_string())?;
    let direct = "cfg_if::cfg_if! { if #[cfg(all())] { #[macro_use] mod helpers; } }";
    let direct_with_tests = "cfg_if::cfg_if! { if #[cfg(all())] { #[macro_use] mod helpers; #[cfg(test)] mod tests; } }";
    let rows = [
        Row {
            case: "unrelated_module_is_admitted",
            items: direct,
            inline_tests: true,
            files: &[("helpers.rs", UNRELATED)],
            cfg_if: CfgIf::Registry,
            exposed: true,
            runtime: Runtime::Catches,
        },
        Row {
            case: "else_branch_module_is_admitted",
            items: "cfg_if::cfg_if! { if #[cfg(any())] { mod absent; } else { #[macro_use] mod helpers; } }",
            inline_tests: true,
            files: &[("helpers.rs", UNRELATED)],
            cfg_if: CfgIf::Registry,
            exposed: true,
            runtime: Runtime::Catches,
        },
        Row {
            case: "directory_module_is_admitted",
            items: direct,
            inline_tests: true,
            files: &[("helpers/mod.rs", UNRELATED)],
            cfg_if: CfgIf::Registry,
            exposed: true,
            runtime: Runtime::Catches,
        },
        // The definition inside the module stays a binding site. Written
        // beside inline tests rustc rejects it (E0659), so the shadow cannot
        // pass a wrong library there; both modules from one expansion do
        // compile, and the wrong library passes.
        Row {
            case: "shadow_beside_inline_tests_does_not_compile",
            items: direct,
            inline_tests: true,
            files: &[("helpers.rs", SHADOW)],
            cfg_if: CfgIf::Registry,
            exposed: false,
            runtime: Runtime::Ambiguous,
        },
        Row {
            case: "shadow_and_tests_from_one_expansion",
            items: direct_with_tests,
            inline_tests: false,
            files: &[("helpers.rs", SHADOW), ("tests.rs", TEST_FILE)],
            cfg_if: CfgIf::Registry,
            exposed: false,
            runtime: Runtime::Misses,
        },
        Row {
            case: "extern_crate_stays_refused",
            items: "cfg_if::cfg_if! { if #[cfg(all())] { #[macro_use] extern crate alloc; } }",
            inline_tests: true,
            files: &[],
            cfg_if: CfgIf::Registry,
            exposed: false,
            runtime: Runtime::Catches,
        },
        Row {
            case: "inline_module_stays_refused",
            items: "cfg_if::cfg_if! { if #[cfg(all())] { #[macro_use] mod helpers { macro_rules! twice { ($value:expr) => { $value * 2 }; } } } }",
            inline_tests: true,
            files: &[],
            cfg_if: CfgIf::Registry,
            exposed: false,
            runtime: Runtime::Catches,
        },
        Row {
            case: "path_attribute_stays_refused",
            items: "cfg_if::cfg_if! { if #[cfg(all())] { #[macro_use] #[path = \"other/helpers.rs\"] mod helpers; } }",
            inline_tests: true,
            files: &[("other/helpers.rs", UNRELATED)],
            cfg_if: CfgIf::Registry,
            exposed: false,
            runtime: Runtime::Catches,
        },
        Row {
            case: "no_implicit_prelude_stays_refused",
            items: "cfg_if::cfg_if! { if #[cfg(all())] { #[macro_use] mod helpers; #[no_implicit_prelude] mod quiet {} } }",
            inline_tests: true,
            files: &[("helpers.rs", UNRELATED)],
            cfg_if: CfgIf::Registry,
            exposed: false,
            runtime: Runtime::Catches,
        },
        // The same text, but `cfg-if` is another package whose macro
        // defines a shadow `assert_eq!`. rustc rejects that beside the
        // inline tests (E0659), so no wrong library passes; the pin stays
        // conservative because only the manifest tells the packages apart.
        Row {
            case: "renamed_package_is_refused",
            items: direct,
            inline_tests: true,
            files: &[("helpers.rs", UNRELATED)],
            cfg_if: CfgIf::RenamedPackage,
            exposed: false,
            runtime: Runtime::Ambiguous,
        },
        // A module of the crate named `cfg_if` takes the path's first
        // segment from the crate. (The shadow's definition is also a binding
        // site here; the unit test isolates the module-name rule.)
        Row {
            case: "module_named_cfg_if_is_refused",
            items: "mod cfg_if {\n    macro_rules! cfg_if { (if #[cfg($m:meta)] { $($t:tt)* }) => { macro_rules! assert_eq { ($a:expr, $b:expr) => {{ let _ = (&$a, &$b); }}; } $($t)* }; }\n    pub(crate) use cfg_if;\n}\ncfg_if::cfg_if! { if #[cfg(all())] { #[macro_use] mod helpers; } }",
            inline_tests: true,
            files: &[("helpers.rs", UNRELATED)],
            cfg_if: CfgIf::Registry,
            exposed: false,
            runtime: Runtime::Ambiguous,
        },
    ];
    for row in rows {
        let source = format!(
            "{production}\n{}\n{}",
            row.items,
            if row.inline_tests { INLINE_TESTS } else { "" }
        );
        let scratch = Scratch::create()?;
        let analyzed = scratch.0.join("analyzed");
        let registry = "cfg-if = \"1\"";
        let renamed = "cfg-if = { package = \"fake-cfg-if\", path = \"../fake\" }";
        let runtime_dependency = "cfg-if = { path = \"../cfgif\" }";
        let (dependency, runtime_dependency, library) = match row.cfg_if {
            CfgIf::Registry => (registry, runtime_dependency, CFG_IF_COPY),
            CfgIf::RenamedPackage => (renamed, renamed, SHADOWING_CFG_IF),
        };
        workspace(&analyzed, dependency, &source, row.files)?;
        let report = check_workspace(CheckInput {
            root: analyzed.clone(),
            diff_file: Some(fixture.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let json: serde_json::Value =
            serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                .map_err(|error| error.to_string())?;
        let findings = json["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(findings.len(), 1, "{}", row.case);
        assert_eq!(
            findings[0]["classification"] == "exposed",
            row.exposed,
            "{}: {}",
            row.case,
            findings[0]["classification"]
        );
        // Runtime controls: the dependency name resolves offline to a path.
        let library_root = scratch.0.join(match row.cfg_if {
            CfgIf::Registry => "cfgif",
            CfgIf::RenamedPackage => "fake",
        });
        write(
            &library_root.join("Cargo.toml"),
            &format!(
                "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
                match row.cfg_if {
                    CfgIf::Registry => "cfg-if",
                    CfgIf::RenamedPackage => "fake-cfg-if",
                }
            ),
        )?;
        write(&library_root.join("src/lib.rs"), library)?;
        for wrong in [false, true] {
            let runtime = scratch.0.join(if wrong { "wrong" } else { "right" });
            let runtime_source = if wrong {
                source.replace("input * 3", "input * 2")
            } else {
                source.clone()
            };
            workspace(&runtime, runtime_dependency, &runtime_source, row.files)?;
            let (success, output) = runtime_output(&runtime, &scratch.0.join("target"))?;
            match (row.runtime, wrong) {
                (Runtime::Ambiguous, _) => assert!(
                    !success && output.contains("E0659"),
                    "{}: wrong={wrong}: {output}",
                    row.case
                ),
                (Runtime::Catches, true) => assert!(
                    !success && output.contains("0 passed; 1 failed;"),
                    "{}: the wrong library must fail the test: {output}",
                    row.case
                ),
                _ => assert!(
                    success && output.contains("1 passed; 0 failed;"),
                    "{}: wrong={wrong}: {output}",
                    row.case
                ),
            }
        }
    }
    Ok(())
}
