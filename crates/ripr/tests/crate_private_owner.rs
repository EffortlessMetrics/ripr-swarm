//! End-to-end witness for #15932: an integration test under `tests/` cannot
//! name a function its crate does not export, so its call of that name
//! (`tokio::time::sleep`) is not a call of the crate-private function
//! (`runtime::jspi::sleep`). Every shape where the files do not settle that
//! keeps the name-only relation.

use ripr::{CheckInput, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The tokio shape: a crate-private `jspi::sleep` beside the public
/// `time::sleep`, which the crate re-exports from a private module so a
/// spelled `time::sleep` does not settle on either definition by path.
/// `{vis}` is the visibility and `{attr}` an attribute line of the changed
/// function; line 2 is the changed one.
fn lib(attr: &str, vis: &str) -> String {
    lib_with(attr, "", vis)
}

fn lib_with(attr: &str, fn_attr: &str, vis: &str) -> String {
    format!(
        "{attr}mod jspi {{\n{fn_attr}    {vis}fn sleep(ticks: u64) -> u64 {{ park(ticks + 2) }}\n\
         \x20   fn park(ticks: u64) -> u64 {{ ticks * 2 }}\n}}\n\n\
         pub mod time {{\n    mod sleep {{\n        pub fn sleep(ticks: u64) -> u64 {{ ticks + 9 }}\n    }}\n\
         \x20   pub use self::sleep::sleep;\n}}\n\n\
         pub fn run(ticks: u64) -> u64 {{\n    if ticks > 100 {{ jspi::sleep(ticks) }} else {{ ticks }}\n}}\n"
    )
}

const PUBLIC_SLEEP_TEST: &str = "use private::time::sleep;\n\n#[test]\nfn sleeps_imported() {\n    assert_eq!(sleep(1), 10);\n}\n\n\
     #[test]\nfn sleeps_qualified() {\n    assert_eq!(private::time::sleep(4), 13);\n}\n";

/// Names of the tests related to the `return_value` finding on the changed
/// line (`+ 1` became `+ 2` on the line holding `needle`).
fn related(lib: &str, needle: &str, test: &str) -> Result<Vec<String>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-crate-private-owner-{}-{sequence}",
            std::process::id()
        )),
    };
    let root = &repo.root;
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("src dir: {error}"))?;
    std::fs::create_dir_all(root.join("tests")).map_err(|error| format!("tests dir: {error}"))?;
    let write = |path: &str, text: &str| {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
    };
    write(
        "Cargo.toml",
        "[package]\nname = \"private\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write("src/lib.rs", lib)?;
    write("tests/api.rs", test)?;
    let lines = lib.lines().collect::<Vec<_>>();
    let changed = lines
        .iter()
        .position(|line| line.contains(needle))
        .ok_or_else(|| format!("fixture lost `{needle}`"))?;
    let mut body = String::new();
    for (at, line) in lines.iter().enumerate() {
        if at == changed {
            body.push_str(&format!("-{}\n+{line}\n", line.replace("+ 2", "+ 1")));
        } else {
            body.push_str(&format!(" {line}\n"));
        }
    }
    write(
        "diff.patch",
        &format!(
            "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{count} +1,{count} @@\n{body}",
            count = lines.len(),
        ),
    )?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
    .map_err(|error| format!("check failed: {error}"))?;
    let findings = output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == changed + 1)
        .collect::<Vec<Finding>>();
    let finding = findings
        .iter()
        .find(|finding| finding.probe.family.as_str() == "return_value")
        .ok_or_else(|| format!("no return_value finding on the changed line: {findings:?}"))?;
    let mut names = finding
        .related_tests
        .iter()
        .map(|test| test.name.clone())
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

#[test]
fn a_function_the_crate_does_not_export_relates_no_integration_test() -> Result<(), String> {
    for vis in ["", "pub(crate) ", "pub(super) ", "pub(in crate) "] {
        assert_eq!(
            related(&lib("", vis), "ticks + 2", PUBLIC_SLEEP_TEST)?,
            Vec::<String>::new(),
            "`{vis}fn sleep` is not nameable from tests/"
        );
    }
    Ok(())
}

/// Controls: the same tests keep the relation wherever the files do not
/// settle that the call cannot be the changed function.
#[test]
fn a_function_the_test_may_still_run_keeps_its_relation() -> Result<(), String> {
    let both = names(&["sleeps_imported", "sleeps_qualified"]);
    let exported = lib("", "pub ");
    assert_eq!(
        related(&exported, "ticks + 2", PUBLIC_SLEEP_TEST)?,
        both,
        "a `pub fn` may be re-exported"
    );
    let cases = [
        (
            "an attribute may export the function",
            lib_with("", "    #[unsafe(no_mangle)]\n", "pub(crate) "),
            PUBLIC_SLEEP_TEST.to_string(),
        ),
        (
            "a foreign ABI is called from outside Rust",
            lib("", "pub(crate) extern \"C\" "),
            PUBLIC_SLEEP_TEST.to_string(),
        ),
        (
            "an attribute macro receives the module",
            lib("#[bridge::module]\n", "pub(crate) "),
            PUBLIC_SLEEP_TEST.to_string(),
        ),
        (
            "the test crate compiles the source file itself",
            lib("", "pub(crate) "),
            format!("#[path = \"../src/lib.rs\"]\nmod copy;\n{PUBLIC_SLEEP_TEST}"),
        ),
        (
            "the test crate includes the source file",
            lib("", "pub(crate) "),
            format!("include!(\"../src/lib.rs\");\n{PUBLIC_SLEEP_TEST}"),
        ),
    ];
    let mut dropped = Vec::new();
    for (case, lib, test) in &cases {
        let related =
            related(lib, "ticks + 2", test).map_err(|error| format!("{case}: {error}"))?;
        if related != both {
            dropped.push((*case, related));
        }
    }
    assert_eq!(dropped, Vec::new());
    Ok(())
}

/// A unit test of the same crate names the crate-private function and keeps
/// its relation.
#[test]
fn the_crate_s_own_test_still_relates() -> Result<(), String> {
    let with_unit_test = format!(
        "{}\n#[cfg(test)]\nmod tests {{\n    #[test]\n    fn sleeps_inside() {{\n        assert_eq!(crate::jspi::sleep(1), 6);\n    }}\n}}\n",
        lib("", "pub(crate) ")
    );
    assert_eq!(
        related(&with_unit_test, "ticks + 2", PUBLIC_SLEEP_TEST)?,
        names(&["sleeps_inside"])
    );
    // A test that reaches the function through the public entry in front of
    // it keeps that relation whether or not it also calls a namesake: the
    // namesake call neither lends a relation nor takes one away.
    let behind_entry = "mod jspi {\n    pub(crate) fn sleep(ticks: u64) -> u64 { ticks + 2 }\n}\n\n\
         pub fn run(ticks: u64) -> u64 {\n    if ticks > 100 { jspi::sleep(ticks) } else { ticks }\n}\n";
    for namesake in ["", "    std::thread::sleep(std::time::Duration::ZERO);\n"] {
        let entry = format!(
            "#[test]\nfn runs_the_entry() {{\n{namesake}    assert_eq!(private::run(200), 202);\n}}\n"
        );
        assert_eq!(
            related(behind_entry, "ticks + 2", &entry)?,
            names(&["runs_the_entry"]),
            "`private::run` reaches the function (namesake call: {})",
            !namesake.is_empty()
        );
    }
    Ok(())
}

/// A test named after the function is no relation either when its crate
/// cannot name the function: `sleep_is_quick` is named for the public
/// `time::sleep`.
#[test]
fn a_test_named_after_an_unexported_function_is_not_related() -> Result<(), String> {
    let named = "#[test]\nfn sleep_is_quick() {\n    assert_eq!(private::run(1), 1);\n}\n";
    assert_eq!(
        related(&lib("", "pub "), "ticks + 2", named)?,
        names(&["sleep_is_quick"])
    );
    assert_eq!(
        related(&lib("", "pub(crate) "), "ticks + 2", named)?,
        Vec::<String>::new()
    );
    Ok(())
}

/// The same rule for an inherent method: a `.sleep(..)` call in `tests/`
/// cannot be the crate-private `Timer::sleep`.
#[test]
fn an_unexported_inherent_method_relates_no_integration_test() -> Result<(), String> {
    let lib = |vis: &str| {
        format!(
            "pub struct Timer;\n\nimpl Timer {{\n    {vis}fn sleep(&self, ticks: u64) -> u64 {{ ticks + 2 }}\n\
             \x20   pub fn run(&self, ticks: u64) -> u64 {{ if ticks > 100 {{ self.sleep(ticks) }} else {{ ticks }} }}\n}}\n\n\
             pub struct Clock;\n\nimpl Clock {{\n    pub fn sleep(&self, ticks: u64) -> u64 {{ ticks + 9 }}\n}}\n"
        )
    };
    let test = "#[test]\nfn clock_sleeps() {\n    assert_eq!(private::Clock.sleep(1), 10);\n}\n";
    assert_eq!(
        related(&lib("pub "), "ticks + 2", test)?,
        names(&["clock_sleeps"]),
        "a `pub` method of the name keeps the name-only relation"
    );
    for vis in ["", "pub(crate) "] {
        assert_eq!(
            related(&lib(vis), "ticks + 2", test)?,
            Vec::<String>::new(),
            "`{vis}fn sleep(&self)` is not nameable from tests/"
        );
    }
    // A namesake call takes nothing away either. A method entry
    // (`timer.run(..)`) is not yet a relation from `tests/` on its own, for a
    // `pub` method as well, so the reading is the same with and without it.
    let through_entry = |namesake: &str| {
        format!(
            "#[test]\nfn timer_runs() {{\n    let timer = private::Timer;\n{namesake}    assert_eq!(timer.run(200), 202);\n}}\n"
        )
    };
    assert_eq!(
        related(
            &lib("pub(crate) "),
            "ticks + 2",
            &through_entry("    std::thread::sleep(std::time::Duration::ZERO);\n")
        )?,
        related(&lib("pub(crate) "), "ticks + 2", &through_entry(""))?
    );
    Ok(())
}
