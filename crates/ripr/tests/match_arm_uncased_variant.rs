//! End-to-end witness for uncased enum variants in RIPR-SPEC-0229 arm
//! selection (#7543).
//!
//! `Kind::A` may name a variant or an associated constant, which a pattern
//! compares by value. Arm selection reads it as a variant only when the
//! owner's file declares `enum Kind` with that variant, or imports `Kind`
//! from the one module file that does (#7675). So examples 4 and 6 name
//! their missing discriminators with single-letter variants, and a `Kind`
//! that may be a struct with constants names none.

use ripr::{
    CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const ENUM: &str = "#[derive(Debug, Clone, Copy, PartialEq)]\npub enum Kind {\n    A,\n    B,\n}\n";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    /// `lib` is the new `src/lib.rs` and `files` the other sources by path;
    /// `diff` changes one line of one of them.
    fn create(lib: &str, files: &[(&str, &str)], test: &str, diff: &str) -> Result<Self, String> {
        // A process-wide sequence, not the clock: parallel tests share one
        // `SystemTime` on some platforms.
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-match-arm-uncased-variant-{}-{sequence}",
            std::process::id()
        ));
        // Built before any write so `Drop` removes a partly written root.
        let repo = Self { root };
        let root = &repo.root;
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        let write = |path: &str, text: &str| {
            if let Some(parent) = root.join(path).parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {path} directory failed: {error}"))?;
            }
            std::fs::write(root.join(path), text)
                .map_err(|error| format!("write {path} failed: {error}"))
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"uncased-variant\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )?;
        write("src/lib.rs", lib)?;
        for (path, text) in files {
            write(path, text)?;
        }
        write("tests/it.rs", test)?;
        write("diff.patch", diff)?;
        Ok(repo)
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn body(arms: &str, returns: &str) -> String {
    format!("pub fn f(k: Kind) -> {returns} {{\n    match k {{\n{arms}    }}\n}}\n")
}

fn diff(path: &str, line: usize, context: &[&str], old: &str, new: &str) -> String {
    // `context` holds the three lines before the changed one; `}` closers
    // follow it.
    let start = line - 3;
    let before = context
        .iter()
        .map(|line| format!(" {line}\n"))
        .collect::<String>();
    format!(
        "diff --git a/{path} b/{path}\nindex 0000000..1111111 100644\n--- a/{path}\n+++ b/{path}\n@@ -{start},6 +{start},6 @@\n{before}-{old}\n+{new}\n     }}\n }}\n"
    )
}

/// The missing-discriminator values of the one changed match-arm finding.
fn named_discriminators(output: &CheckOutput, line: usize) -> Result<Vec<String>, String> {
    let mut arms = output.findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::MatchArm && finding.probe.location.line == line
    });
    let finding = arms.next().ok_or_else(|| {
        let observed = output
            .findings
            .iter()
            .map(|finding| {
                format!(
                    "{}:{}:{}",
                    finding.probe.family.as_str(),
                    finding.probe.location.line,
                    finding.probe.expression
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        format!("missing the changed match arm at line {line}; observed {observed}")
    })?;
    if format!("{output:?}").contains("MalformedDiff") {
        return Err("the fixture diff must parse cleanly".to_string());
    }
    Ok(finding
        .activation
        .missing_discriminators
        .iter()
        .map(|missing| missing.value.clone())
        .collect())
}

const WILDCARD_ARMS: &str = "        Kind::A => 1,\n        _ => 2,\n";
const WILDCARD_TEST: &str = "use uncased_variant::{Kind, f};\n\n#[test]\nfn kind_a_is_one() {\n    assert_eq!(f(Kind::A), 1);\n}\n";

#[test]
fn a_declared_single_letter_enum_names_the_unselected_wildcard() -> Result<(), String> {
    // RIPR-SPEC-0229 example 4: `f(Kind::A)` takes the earlier arm.
    let lib = format!("{ENUM}\n{}", body(WILDCARD_ARMS, "i32"));
    let repo = TempRepo::create(
        &lib,
        &[],
        WILDCARD_TEST,
        &diff(
            "src/lib.rs",
            10,
            &[
                "pub fn f(k: Kind) -> i32 {",
                "    match k {",
                "        Kind::A => 1,",
            ],
            "        _ => 0,",
            "        _ => 2,",
        ),
    )?;
    assert_eq!(named_discriminators(&repo.check()?, 10)?, vec!["_"]);
    Ok(())
}

#[test]
fn a_declared_single_letter_enum_names_the_unselected_variant_arm() -> Result<(), String> {
    // RIPR-SPEC-0229 example 6: only the expected side names `Kind::B`.
    let arms = "        Kind::A => Kind::B,\n        Kind::B => Kind::A,\n";
    let lib = format!("{ENUM}\n{}", body(arms, "Kind"));
    let test = "use uncased_variant::{Kind, f};\n\n#[test]\nfn a_maps_to_b() {\n    assert_eq!(f(Kind::A), Kind::B);\n}\n";
    let repo = TempRepo::create(
        &lib,
        &[],
        test,
        &diff(
            "src/lib.rs",
            10,
            &[
                "pub fn f(k: Kind) -> Kind {",
                "    match k {",
                "        Kind::A => Kind::B,",
            ],
            "        Kind::B => Kind::B,",
            "        Kind::B => Kind::A,",
        ),
    )?;
    assert_eq!(named_discriminators(&repo.check()?, 10)?, vec!["Kind::B"]);
    Ok(())
}

/// The wildcard example with `Kind` reached through `import` from
/// `src/kinds.rs` (which holds `kinds`) and the changed arm at line 7.
fn imported_wildcard(import: &str, kinds: &str) -> Result<Vec<String>, String> {
    let lib = format!("mod kinds;\n{import}\n\n{}", body(WILDCARD_ARMS, "i32"));
    let repo = TempRepo::create(
        &lib,
        &[("src/kinds.rs", kinds)],
        WILDCARD_TEST,
        &diff(
            "src/lib.rs",
            7,
            &[
                "pub fn f(k: Kind) -> i32 {",
                "    match k {",
                "        Kind::A => 1,",
            ],
            "        _ => 0,",
            "        _ => 2,",
        ),
    )?;
    named_discriminators(&repo.check()?, 7)
}

#[test]
fn an_enum_imported_from_its_module_file_names_the_unselected_wildcard() -> Result<(), String> {
    // #7675: the owner's one import of `Kind` names `src/kinds.rs`, which
    // declares the enum, so `A` is a variant there too.
    assert_eq!(imported_wildcard("pub use kinds::Kind;", ENUM)?, vec!["_"]);
    assert_eq!(
        imported_wildcard("use crate::kinds::Kind;", ENUM)?,
        vec!["_"]
    );
    Ok(())
}

#[test]
fn an_imported_type_that_may_hold_constants_names_no_arm() -> Result<(), String> {
    // `Kind` declared in another file as a struct with constants: `Kind::A`
    // is compared by value, so the wildcard is not shown unselected.
    let constants = "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub struct Kind(u8);\n\nimpl Kind {\n    pub const A: Kind = Kind(0);\n    pub const B: Kind = Kind(1);\n}\n";
    assert_eq!(
        imported_wildcard("pub use kinds::Kind;", constants)?,
        Vec::<String>::new()
    );
    // A renamed import is not followed: `Kind` there is `Other`, a struct
    // with constants, though the same file also declares `enum Kind`.
    let other = format!("{ENUM}\n{}", CONSTANTS.replace("Kind", "Other"));
    assert_eq!(
        imported_wildcard("pub use kinds::Other as Kind;", &other)?,
        Vec::<String>::new()
    );
    // A module file that only re-exports `Kind` from elsewhere is not read
    // further.
    let reexport = format!("mod inner {{\n{ENUM}}}\npub use inner::Kind;\n");
    assert_eq!(
        imported_wildcard("pub use kinds::Kind;", &reexport)?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_test_side_constant_named_like_a_variant_names_no_arm() -> Result<(), String> {
    // The test's own `Kind::A` is a constant holding the owner's `Kind::B`,
    // so `f(Kind::A)` takes the changed wildcard arm. The test file declares
    // its own `Kind`, so its uncased inputs are not read (#7620 review).
    let lib = format!("{ENUM}\n{}", body(WILDCARD_ARMS, "i32"));
    let test = "use uncased_variant::f;\n\nstruct Kind;\n\nimpl Kind {\n    const A: uncased_variant::Kind = uncased_variant::Kind::B;\n}\n\n#[test]\nfn shadowed_a_is_two() {\n    assert_eq!(f(Kind::A), 2);\n}\n";
    let repo = TempRepo::create(
        &lib,
        &[],
        test,
        &diff(
            "src/lib.rs",
            10,
            &[
                "pub fn f(k: Kind) -> i32 {",
                "    match k {",
                "        Kind::A => 1,",
            ],
            "        _ => 0,",
            "        _ => 2,",
        ),
    )?;
    assert_eq!(
        named_discriminators(&repo.check()?, 10)?,
        Vec::<String>::new()
    );
    Ok(())
}

/// A one-line diff of `text` at `path` whose new line is the one equal to
/// `new` (formerly `old`), with the changed line's number.
fn diff_at(path: &str, text: &str, new: &str, old: &str) -> Result<(String, usize), String> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut matches = lines.iter().enumerate().filter(|(_, line)| **line == new);
    let (index, _) = matches
        .next()
        .ok_or_else(|| format!("{path} has no line {new:?}"))?;
    if matches.next().is_some() || index < 3 || index + 3 > lines.len() {
        return Err(format!(
            "{path} needs one {new:?} with three lines before and two after"
        ));
    }
    let context = |range: std::ops::Range<usize>| {
        lines[range]
            .iter()
            .map(|line| format!(" {line}\n"))
            .collect::<String>()
    };
    let start = index - 2;
    Ok((
        format!(
            "diff --git a/{path} b/{path}\nindex 0000000..1111111 100644\n--- a/{path}\n+++ b/{path}\n@@ -{start},6 +{start},6 @@\n{}-{old}\n+{new}\n{}",
            context(index - 3..index),
            context(index + 1..index + 3)
        ),
        index + 1,
    ))
}

/// The missing discriminators of the wildcard arm `_ => 2` (formerly
/// `_ => 0`) changed in `path`, one of `files` or `src/lib.rs`.
fn changed_wildcard(
    lib: &str,
    files: &[(&str, &str)],
    test: &str,
    path: &str,
) -> Result<Vec<String>, String> {
    let text = if path == "src/lib.rs" {
        lib
    } else {
        files
            .iter()
            .find(|(file, _)| *file == path)
            .map(|(_, text)| *text)
            .ok_or_else(|| format!("no fixture file {path}"))?
    };
    let changed = text
        .lines()
        .find(|line| line.trim() == "_ => 2,")
        .ok_or_else(|| format!("{path} has no `_ => 2,` arm"))?;
    let (diff, line) = diff_at(path, text, changed, &changed.replace('2', "0"))?;
    let repo = TempRepo::create(lib, files, test, &diff)?;
    named_discriminators(&repo.check()?, line)
}

const CONSTANTS: &str = "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub struct Kind(u8);\n\nimpl Kind {\n    pub const A: Kind = Kind(0);\n    pub const B: Kind = Kind(1);\n}\n";

#[test]
fn an_enum_imported_through_super_names_the_unselected_wildcard() -> Result<(), String> {
    // #7675: `super::kinds` from `src/shapes.rs` is `src/kinds.rs`.
    let shapes = format!("use super::kinds::Kind;\n\n{}", body(WILDCARD_ARMS, "i32"));
    let files = [("src/kinds.rs", ENUM), ("src/shapes.rs", shapes.as_str())];
    let lib = "mod kinds;\nmod shapes;\n\npub use kinds::Kind;\npub use shapes::f;\n";
    assert_eq!(
        changed_wildcard(lib, &files, WILDCARD_TEST, "src/shapes.rs")?,
        vec!["_"]
    );
    Ok(())
}

#[test]
fn an_owner_item_that_may_shadow_the_import_names_no_arm() -> Result<(), String> {
    // #7686 review: the match sits in an inline module whose own `Kind` is
    // a struct with constants, though the file imports the enum.
    let lib = format!(
        "mod kinds;\nuse kinds::Kind;\n\npub mod legacy {{\n{CONSTANTS}\n{}}}\n\npub fn g(k: Kind) -> bool {{\n    matches!(k, Kind::A)\n}}\n",
        body(WILDCARD_ARMS, "i32")
    );
    let test = "use uncased_variant::legacy::{Kind, f};\n\n#[test]\nfn kind_a_is_one() {\n    assert_eq!(f(Kind::A), 1);\n}\n";
    assert_eq!(
        changed_wildcard(&lib, &[("src/kinds.rs", ENUM)], test, "src/lib.rs")?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_cfg_alternative_inline_module_names_no_arm() -> Result<(), String> {
    // #7686 review: the compiled `kinds` is the inline module with
    // constants; `src/kinds.rs` holds the enum but is compiled out.
    let lib = format!(
        "#[cfg(any())]\nmod kinds;\n#[cfg(not(any()))]\nmod kinds {{\n{CONSTANTS}}}\npub use kinds::Kind;\n\n{}",
        body(WILDCARD_ARMS, "i32")
    );
    assert_eq!(
        changed_wildcard(&lib, &[("src/kinds.rs", ENUM)], WILDCARD_TEST, "src/lib.rs")?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_crate_path_from_below_an_inline_module_names_no_arm() -> Result<(), String> {
    // #7686 review: `mid` is declared inside `mod outer { .. }`, so the
    // owner's recorded chain starts at `src/outer/mid.rs`, not the crate
    // root. `crate::kinds` is `src/kinds.rs` (constants), not
    // `src/outer/mid/kinds.rs` (an enum).
    let owner = format!("use crate::kinds::Kind;\n\n{}", body(WILDCARD_ARMS, "i32"));
    let files = [
        ("src/kinds.rs", CONSTANTS),
        ("src/outer/mid.rs", "pub mod x;\nmod kinds;\n"),
        ("src/outer/mid/kinds.rs", ENUM),
        ("src/outer/mid/x.rs", owner.as_str()),
    ];
    let lib = "pub mod kinds;\npub mod outer {\n    pub mod mid;\n}\n";
    let test = "use uncased_variant::{kinds::Kind, outer::mid::x::f};\n\n#[test]\nfn kind_a_is_one() {\n    assert_eq!(f(Kind::A), 1);\n}\n";
    assert_eq!(
        changed_wildcard(lib, &files, test, "src/outer/mid/x.rs")?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_cfg_gated_enum_import_names_no_arm() -> Result<(), String> {
    // #7686 review: the enum import is compiled out, so `Kind` comes from
    // the glob of a module holding a struct with constants.
    let lib = format!(
        "#[cfg(any())]\nmod kinds;\n#[cfg(any())]\npub use kinds::Kind;\nmod constants;\npub use constants::*;\n\n{}",
        body(WILDCARD_ARMS, "i32")
    );
    let files = [("src/kinds.rs", ENUM), ("src/constants.rs", CONSTANTS)];
    assert_eq!(
        changed_wildcard(&lib, &files, WILDCARD_TEST, "src/lib.rs")?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_crate_root_named_file_below_inline_modules_names_no_arm() -> Result<(), String> {
    // #7686 review: `src/outer/src/main.rs` is a module below inline
    // modules, not a crate root, so `crate::kinds` is `src/kinds.rs`
    // (constants), not the enum beside it.
    let owner = format!(
        "use crate::kinds::Kind;\nmod kinds;\n\n{}",
        body(WILDCARD_ARMS, "i32")
    );
    let files = [
        ("src/kinds.rs", CONSTANTS),
        ("src/outer/src/kinds.rs", ENUM),
        ("src/outer/src/main.rs", owner.as_str()),
        ("src/outer/src/main/kinds.rs", ENUM),
    ];
    let lib =
        "pub mod kinds;\npub mod outer {\n    pub mod src {\n        pub mod main;\n    }\n}\n";
    let test = "use uncased_variant::{kinds::Kind, outer::src::main::f};\n\n#[test]\nfn kind_a_is_one() {\n    assert_eq!(f(Kind::A), 1);\n}\n";
    assert_eq!(
        changed_wildcard(lib, &files, test, "src/outer/src/main.rs")?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_statement_macro_that_may_shadow_the_type_names_no_arm() -> Result<(), String> {
    // #7686 review: `shadow!()` declares a local `Kind` whose `A` holds the
    // enum's `B`, so `f(Kind::A)` takes the changed wildcard arm.
    let lib = "mod kinds;\npub use kinds::Kind;\n\nmacro_rules! shadow {\n    () => {\n        struct Kind;\n        impl Kind {\n            const A: crate::kinds::Kind = crate::kinds::Kind::B;\n        }\n    };\n}\n\npub fn f(k: Kind) -> i32 {\n    shadow!();\n    match k {\n        Kind::A => 1,\n        _ => 2,\n    }\n}\n";
    let kinds = ENUM.replace("PartialEq)", "PartialEq, Eq)");
    let test = "use uncased_variant::{Kind, f};\n\n#[test]\nfn shadowed_a_is_two() {\n    assert_eq!(f(Kind::A), 2);\n}\n";
    assert_eq!(
        changed_wildcard(lib, &[("src/kinds.rs", kinds.as_str())], test, "src/lib.rs")?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_path_attribute_spelled_with_spaces_names_no_arm() -> Result<(), String> {
    // #7686 review: `#[ path = ..]` places the owner at
    // `src/outer/src/main.rs` below an inline module; `crate::kinds` there
    // is `src/kinds.rs` (constants).
    let owner = format!(
        "use crate::kinds::Kind;\nmod kinds;\n\n{}",
        body(WILDCARD_ARMS, "i32")
    );
    let files = [
        ("src/kinds.rs", CONSTANTS),
        ("src/outer/src/kinds.rs", ENUM),
        ("src/outer/src/main.rs", owner.as_str()),
        ("src/outer/src/main/kinds.rs", ENUM),
    ];
    let lib =
        "pub mod kinds;\npub mod outer {\n    #[ path = \"src/main.rs\"]\n    pub mod m;\n}\n";
    let test = "use uncased_variant::{kinds::Kind, outer::m::f};\n\n#[test]\nfn kind_a_is_one() {\n    assert_eq!(f(Kind::A), 1);\n}\n";
    assert_eq!(
        changed_wildcard(lib, &files, test, "src/outer/src/main.rs")?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_shadowed_std_macro_in_statement_position_names_no_arm() -> Result<(), String> {
    // #7686 review: a local `macro_rules! assert` takes precedence over the
    // standard one, so `assert!();` may declare a local `Kind`. Any
    // statement macro in the owner fails closed, whatever its name.
    let lib = "mod kinds;\npub use kinds::Kind;\n\nmacro_rules! assert {\n    () => {\n        struct Kind;\n        impl Kind {\n            const A: crate::kinds::Kind = crate::kinds::Kind::B;\n        }\n    };\n}\n\npub fn f(k: Kind) -> i32 {\n    assert!();\n    match k {\n        Kind::A => 1,\n        _ => 2,\n    }\n}\n";
    let kinds = ENUM.replace("PartialEq)", "PartialEq, Eq)");
    let test = "use uncased_variant::{Kind, f};\n\n#[test]\nfn shadowed_a_is_two() {\n    assert_eq!(f(Kind::A), 2);\n}\n";
    assert_eq!(
        changed_wildcard(lib, &[("src/kinds.rs", kinds.as_str())], test, "src/lib.rs")?,
        Vec::<String>::new()
    );
    // The same shadowing call in another function cannot reach the match,
    // so the read stands.
    let plain = lib
        .replace("    assert!();\n", "")
        .replace("pub fn f(", "pub fn g() {\n    assert!();\n}\n\npub fn f(");
    assert_eq!(
        changed_wildcard(
            &plain,
            &[("src/kinds.rs", kinds.as_str())],
            WILDCARD_TEST,
            "src/lib.rs"
        )?,
        vec!["_"]
    );
    Ok(())
}

/// A package whose `[lib]` is `lib.rs` at the package root, with the owner
/// at `src/lib.rs` placed as a module by `root` (#7686 review).
fn root_level_library(root: &str) -> Result<Vec<String>, String> {
    let owner = format!(
        "use crate::kinds::Kind;\nmod kinds;\n\n{}",
        body(WILDCARD_ARMS, "i32")
    );
    let manifest = "[package]\nname = \"uncased-variant\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"lib.rs\"\n\n[workspace]\n";
    let files = [
        ("Cargo.toml", manifest),
        ("lib.rs", root),
        ("kinds.rs", CONSTANTS),
        ("src/kinds.rs", ENUM),
        ("src/lib/kinds.rs", ENUM),
        ("x/kinds.rs", ENUM),
    ];
    let test = "use uncased_variant::kinds::Kind;\n\n#[test]\nfn kind_a_is_one() {\n    assert_eq!(uncased_variant::src::lib::f(Kind::A), 1);\n}\n";
    changed_wildcard(&owner, &files, test, "src/lib.rs")
}

#[test]
fn a_root_named_module_file_below_a_package_root_library_names_no_arm() -> Result<(), String> {
    // `src/lib.rs` is a module of the root-level `lib.rs`, so `crate::kinds`
    // is `kinds.rs` (constants), not `src/kinds.rs`.
    assert_eq!(
        root_level_library("pub mod kinds;\npub mod src {\n    pub mod lib;\n}\n")?,
        Vec::<String>::new()
    );
    // The same through a raw identifier.
    assert_eq!(
        root_level_library("pub mod kinds;\npub mod src {\n    pub mod r#lib;\n}\n")?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_nested_module_step_beside_a_renamed_import_names_no_arm() -> Result<(), String> {
    // #7686 review: `kinds` at the crate root is `constants` renamed; the
    // enum's `mod kinds;` is nested in `mod outer`, a different module.
    let lib = format!(
        "mod outer {{\n    mod kinds;\n}}\npub mod constants;\npub use constants as kinds;\npub use crate::kinds::Kind;\n\n{}",
        body(WILDCARD_ARMS, "i32")
    );
    let files = [
        ("src/constants.rs", CONSTANTS),
        ("src/outer/kinds.rs", ENUM),
    ];
    assert_eq!(
        changed_wildcard(&lib, &files, WILDCARD_TEST, "src/lib.rs")?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_root_named_module_generated_by_a_macro_names_no_arm() -> Result<(), String> {
    // #7686 review: the library root is `src/real.rs`, whose macro
    // generates `pub mod lib;`, so `src/lib.rs` is a module and its
    // `crate::kinds` is `constants` renamed, not `src/kinds.rs`.
    let owner = format!(
        "use crate::kinds::Kind;\nmod kinds;\n\n{}",
        body(WILDCARD_ARMS, "i32")
    );
    let manifest = "[package]\nname = \"uncased-variant\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/real.rs\"\n\n[workspace]\n";
    let root = "macro_rules! m {\n    ($n:ident) => {\n        pub mod $n;\n    };\n}\n\npub mod constants;\npub use constants as kinds;\nm!(lib);\n";
    let files = [
        ("Cargo.toml", manifest),
        ("src/real.rs", root),
        ("src/constants.rs", CONSTANTS),
        ("src/kinds.rs", ENUM),
        ("src/lib/kinds.rs", ENUM),
    ];
    let test = "use uncased_variant::{kinds::Kind, lib::f};\n\n#[test]\nfn kind_a_is_one() {\n    assert_eq!(f(Kind::A), 1);\n}\n";
    assert_eq!(
        changed_wildcard(&owner, &files, test, "src/lib.rs")?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_root_named_module_from_a_macro_given_the_mod_keyword_names_no_arm() -> Result<(), String> {
    // #7686 review: the macro receives `mod` as a token, so no `mod $name`
    // or `mod lib` text appears, yet `m!(mod, lib)` declares `pub mod lib;`
    // under the moved library root.
    let owner = format!(
        "use crate::kinds::Kind;\nmod kinds;\n\n{}",
        body(WILDCARD_ARMS, "i32")
    );
    let manifest = "[package]\nname = \"uncased-variant\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/real.rs\"\n\n[workspace]\n";
    let root = "macro_rules! m {\n    ($k:tt, $n:ident) => {\n        pub $k $n;\n    };\n}\n\npub mod constants;\npub use constants as kinds;\nm!(mod, lib);\n";
    let files = [
        ("Cargo.toml", manifest),
        ("src/real.rs", root),
        ("src/constants.rs", CONSTANTS),
        ("src/kinds.rs", ENUM),
        ("src/lib/kinds.rs", ENUM),
    ];
    let test = "use uncased_variant::{kinds::Kind, lib::f};\n\n#[test]\nfn kind_a_is_one() {\n    assert_eq!(f(Kind::A), 1);\n}\n";
    assert_eq!(
        changed_wildcard(&owner, &files, test, "src/lib.rs")?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_root_named_module_from_an_included_file_names_no_arm() -> Result<(), String> {
    // #7686 review: `include!` splices `src/decl.in`, which the index never
    // reads, and it declares `pub mod lib;` under the moved library root.
    let owner = format!(
        "use crate::kinds::Kind;\nmod kinds;\n\n{}",
        body(WILDCARD_ARMS, "i32")
    );
    let manifest = "[package]\nname = \"uncased-variant\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/real.rs\"\n\n[workspace]\n";
    let root = "pub mod constants;\npub use constants as kinds;\ninclude!(\"decl.in\");\n";
    let files = [
        ("Cargo.toml", manifest),
        ("src/real.rs", root),
        ("src/decl.in", "pub mod lib;\n"),
        ("src/constants.rs", CONSTANTS),
        ("src/kinds.rs", ENUM),
        ("src/lib/kinds.rs", ENUM),
    ];
    let test = "use uncased_variant::{kinds::Kind, lib::f};\n\n#[test]\nfn kind_a_is_one() {\n    assert_eq!(f(Kind::A), 1);\n}\n";
    assert_eq!(
        changed_wildcard(&owner, &files, test, "src/lib.rs")?,
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_module_file_compiled_out_by_an_inner_cfg_names_no_arm() -> Result<(), String> {
    // #7686 review: `#![cfg(any())]` drops `src/kinds.rs`, so `kinds` comes
    // from the glob and `Kind` is the constants struct.
    let owner = format!(
        "mod constants;\npub use constants::*;\nmod kinds;\npub use kinds::Kind;\n\n{}",
        body(WILDCARD_ARMS, "i32")
    );
    let constants = format!("pub mod kinds {{\n{CONSTANTS}}}\n");
    let kinds = format!("#![cfg(any())]\n\n{ENUM}");
    let files = [
        ("src/constants.rs", constants.as_str()),
        ("src/kinds.rs", kinds.as_str()),
    ];
    assert_eq!(
        changed_wildcard(&owner, &files, WILDCARD_TEST, "src/lib.rs")?,
        Vec::<String>::new()
    );
    Ok(())
}

/// The wildcard arm changed in `src/lib.rs`, which imports `Kind` from
/// `src/kinds.rs`, beside a `src/main.rs` holding `main`.
fn imported_wildcard_beside(main: &str) -> Result<Vec<String>, String> {
    let lib = format!(
        "mod kinds;\npub use kinds::Kind;\n\n{}",
        body(WILDCARD_ARMS, "i32")
    );
    changed_wildcard(
        &lib,
        &[("src/kinds.rs", ENUM), ("src/main.rs", main)],
        WILDCARD_TEST,
        "src/lib.rs",
    )
}

#[test]
fn a_sibling_root_that_may_expand_unread_items_names_no_arm() -> Result<(), String> {
    // #7686 review: a macro from a crate the index never reads, in the
    // library root, declares `pub mod main;`, so `src/main.rs` is also a
    // module of the library whose `crate::kinds` is `constants` renamed.
    let owner = format!(
        "use crate::kinds::Kind;\nmod kinds;\n\n{}\nfn main() {{}}\n",
        body(WILDCARD_ARMS, "i32")
    );
    let root = "pub mod constants;\npub use constants as kinds;\ndep::decl!();\n";
    let files = [
        ("src/main.rs", owner.as_str()),
        ("src/constants.rs", CONSTANTS),
        ("src/kinds.rs", ENUM),
        ("src/main/kinds.rs", ENUM),
    ];
    let test = "use uncased_variant::{kinds::Kind, main::f};\n\n#[test]\nfn kind_a_is_one() {\n    assert_eq!(f(Kind::A), 1);\n}\n";
    assert_eq!(
        changed_wildcard(root, &files, test, "src/main.rs")?,
        Vec::<String>::new()
    );
    // A proc-macro attribute on the sibling's `main` may emit any item; a
    // plain or builtin-derive sibling still reads the import.
    for main in [
        "#[dep::entry]\nfn main() {}\n",
        "#[derive(serde::Serialize)]\nstruct S;\nfn main() {}\n",
        "#[cfg_attr(all(), pm::make)]\nstruct S;\nfn main() {}\n",
        "use pm::Clone;\n#[derive(Clone)]\nstruct S;\nfn main() {}\n",
    ] {
        assert_eq!(
            imported_wildcard_beside(main)?,
            Vec::<String>::new(),
            "{main}"
        );
    }
    assert_eq!(
        imported_wildcard_beside("#[derive(Debug, Clone)]\nstruct S;\n\nfn main() {}\n")?,
        vec!["_"]
    );
    Ok(())
}

/// The class of the arm narrowed from `Kind::A | Kind::B => 1` to
/// `Kind::A => 1` beside a later `Kind::B => 2` arm, with `Kind` declared
/// in the owner's file, under `test`.
fn narrowed_arm_class(test: &str) -> Result<ExposureClass, String> {
    let enum_abc =
        "#[derive(Debug, Clone, Copy, PartialEq)]\npub enum Kind {\n    A,\n    B,\n    C,\n}\n";
    narrowed_arm_class_in(enum_abc, "        Kind::B => 2,\n", test)
}

fn narrowed_arm_class_in(kind: &str, later: &str, test: &str) -> Result<ExposureClass, String> {
    let arms = format!("        Kind::A => 1,\n{later}        _ => 0,\n");
    let lib = format!("{kind}\n{}", body(&arms, "i32"));
    let (diff, line) = diff_at(
        "src/lib.rs",
        &lib,
        "        Kind::A => 1,",
        "        Kind::A | Kind::B => 1,",
    )?;
    let repo = TempRepo::create(&lib, &[], test, &diff)?;
    let output = repo.check()?;
    let finding = output
        .findings
        .iter()
        .find(|finding| {
            finding.probe.family == ProbeFamily::MatchArm && finding.probe.location.line == line
        })
        .ok_or_else(|| format!("missing the narrowed match arm at line {line}"))?;
    Ok(finding.class.clone())
}

#[test]
fn an_input_moved_onto_a_later_uncased_arm_exposes_the_narrowed_arm() -> Result<(), String> {
    // #7670: `Kind::B` ran this arm (1) before the narrowing and now runs
    // the later `Kind::B => 2` arm, so the assertion notices the change.
    let moved = "use uncased_variant::{Kind, f};\n\n#[test]\nfn b_is_two() {\n    assert_eq!(f(Kind::B), 2);\n}\n";
    let stayed = "use uncased_variant::{Kind, f};\n\n#[test]\nfn c_is_zero() {\n    assert_eq!(f(Kind::C), 0);\n}\n";
    let moved_class = narrowed_arm_class(moved)?;
    let stayed_class = narrowed_arm_class(stayed)?;
    assert_eq!(moved_class, ExposureClass::Exposed);
    assert_ne!(stayed_class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_later_arm_naming_an_associated_constant_earns_no_moved_input_credit() -> Result<(), String> {
    // #7670: `Kind::B` is an associated constant here, not a variant, so
    // the later arm's pattern is unknown and the narrowing earns no credit.
    let kind = "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum Kind {\n    A,\n    C,\n}\n\nimpl Kind {\n    pub const B: Kind = Kind::C;\n}\n";
    let test = "use uncased_variant::{Kind, f};\n\n#[test]\nfn b_is_two() {\n    assert_eq!(f(Kind::B), 2);\n}\n";
    assert_ne!(
        narrowed_arm_class_in(kind, "        Kind::B => 2,\n", test)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_later_struct_constant_arm_earns_no_moved_input_credit() -> Result<(), String> {
    // #7670: a struct's uppercase constants are never variants.
    let kind = "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub struct Kind(u8);\n\nimpl Kind {\n    pub const A: Kind = Kind(0);\n    pub const B: Kind = Kind(1);\n}\n";
    let test = "use uncased_variant::{Kind, f};\n\n#[test]\nfn b_is_two() {\n    assert_eq!(f(Kind::B), 2);\n}\n";
    assert_ne!(
        narrowed_arm_class_in(kind, "        Kind::B => 2,\n", test)?,
        ExposureClass::Exposed
    );
    Ok(())
}
