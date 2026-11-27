//! Public-API controls for a related test whose only check is an
//! assertion-like macro ripr does not read (#15237).
//!
//! strsim's `assert_delta!` is a test-local `macro_rules!` whose body is
//! `if ($x - $y).abs() > $d { panic!(..) }`. ripr grants it no oracle (#9122
//! credits only transcribers that show a recognized assertion), but the row
//! must not claim the test "has no assertion". Every negative below keeps the
//! absence claim, and no shape gains an oracle.

use ripr::{CheckInput, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = "pub fn bump(x: u32) -> u32 {\n    x.wrapping_add(2)\n}\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn bump(x: u32) -> u32 {
-    x.wrapping_add(1)
+    x.wrapping_add(2)
 }
";

const NEAR_MACRO: &str = "    macro_rules! assert_near {\n        ($x:expr, $y:expr) => {\n            if $x != $y {\n                panic!(\"not near\");\n            }\n        };\n    }\n\n";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(tests: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-unread-assertion-macro-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let this = Self { root };
        std::fs::create_dir_all(this.root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            this.root.join("Cargo.toml"),
            "[package]\nname = \"unread-assertion-macro\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        let source = format!(
            "{OWNER}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{NEAR_MACRO}{tests}}}\n"
        );
        std::fs::write(this.root.join("src/lib.rs"), source)
            .map_err(|error| format!("write src/lib.rs failed: {error}"))?;
        std::fs::write(this.root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(this)
    }

    /// The one finding on the changed line 2.
    fn finding(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut on_line = output
            .findings
            .into_iter()
            .filter(|finding| finding.probe.location.line == 2);
        match (on_line.next(), on_line.next()) {
            (Some(finding), None) => Ok(finding),
            other => Err(format!(
                "premise: exactly one finding on line 2, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const MACRO_LINE: &str = "assertion-like macro not read: `holds` checks through `assert_near!`";

/// The `holds` row's miss token, whether the finding names the macro, and the
/// finding's class.
type Reading = (Option<&'static str>, bool, &'static str);

fn read(tests: &str) -> Result<Reading, String> {
    let finding = TempRepo::create(tests)?.finding()?;
    let Some(row) = finding
        .related_tests
        .iter()
        .find(|test| test.name == "holds")
    else {
        return Err(format!("premise: `holds` is related\n{tests}"));
    };
    if row.oracle.is_some() {
        return Err(format!("no oracle may be granted: {row:?}\n{tests}"));
    }
    let named = finding
        .evidence
        .iter()
        .any(|line| line.starts_with(MACRO_LINE));
    Ok((
        row.miss.map(|miss| miss.as_str()),
        named,
        finding.class.as_str(),
    ))
}

#[test]
fn an_unread_assertion_macro_is_not_read_as_missing_an_assertion() -> Result<(), String> {
    let plain = read("    #[test]\n    fn holds() {\n        let _ = bump(1);\n    }\n")?;
    if plain.0 != Some("no_assertion") || plain.1 {
        return Err(format!(
            "premise: an assertion-free test reads no_assertion: {plain:?}"
        ));
    }
    let tests = "    #[test]\n    fn holds() {\n        assert_near!(bump(1), 3);\n    }\n";
    let (miss, named, class) = read(tests)?;
    if miss != Some("assertion_admission_unproven") || !named || class != plain.2 {
        return Err(format!(
            "the macro must name the limit without credit or a class change (class without it: {}): miss={miss:?} named={named} class={class}",
            plain.2
        ));
    }
    Ok(())
}

/// #15475: a test-local `macro_rules!` that wraps the unread macro and that the
/// test then invokes runs the check, so the row names the inner macro.
#[test]
fn an_invoked_local_wrapper_names_the_macro_it_wraps() -> Result<(), String> {
    let plain = read("    #[test]\n    fn holds() {\n        let _ = bump(1);\n    }\n")?;
    let tests = "    #[test]\n    fn holds() {\n        macro_rules! check {\n            () => { assert_near!(bump(1), 3) };\n        }\n        check!();\n    }\n";
    let (miss, named, class) = read(tests)?;
    if miss != Some("assertion_admission_unproven") || !named || class != plain.2 {
        return Err(format!(
            "an invoked local wrapper must name the macro without credit or a class change (class without it: {}): miss={miss:?} named={named} class={class}",
            plain.2
        ));
    }
    Ok(())
}

#[test]
fn other_shapes_keep_the_absence_claim() -> Result<(), String> {
    for tests in [
        // A macro that is not assertion-like.
        "    #[test]\n    fn holds() {\n        let v = vec![bump(1)];\n        println!(\"{v:?}\");\n    }\n",
        // The assertion-like spelling only in a comment or a string.
        "    #[test]\n    fn holds() {\n        // assert_near!(bump(1), 3);\n        let _ = \"assert_near!(x, y)\";\n        let _ = bump(1);\n    }\n",
        // Inside another macro's input, which never runs it as code.
        "    #[test]\n    fn holds() {\n        let _ = bump(1);\n        let _ = stringify!(assert_near!(bump(1), 3));\n    }\n",
        // Only inside a `macro_rules!` the test defines and never invokes.
        "    #[test]\n    fn holds() {\n        macro_rules! unused {\n            () => { assert_near!(bump(1), 3) };\n        }\n        let _ = bump(1);\n    }\n",
        // A local wrapper invoked only inside another macro's input.
        "    #[test]\n    fn holds() {\n        macro_rules! check {\n            () => { assert_near!(bump(1), 3) };\n        }\n        let _ = bump(1);\n        let _ = stringify!(check!());\n    }\n",
        // An invoked local wrapper whose body only stringifies the macro.
        "    #[test]\n    fn holds() {\n        macro_rules! show {\n            () => { stringify!(assert_near!(bump(1), 3)) };\n        }\n        let _ = bump(1);\n        let _ = show!();\n    }\n",
        // An invoked local wrapper whose body compares an `assert_*` variable.
        "    #[test]\n    fn holds() {\n        let assert_count = bump(1);\n        macro_rules! check {\n            ($x:expr) => { if assert_count != $x { println!(\"{}\", $x); } };\n        }\n        check!(3);\n    }\n",
        // A wrapper defined only inside another macro's input.
        "    #[test]\n    fn holds() {\n        let _ = bump(1);\n        let _ = stringify!(macro_rules! check { () => { assert_near!(bump(1), 3) } });\n        check!();\n    }\n",
        // An invocation that selects the wrapper's quiet arm.
        "    #[test]\n    fn holds() {\n        macro_rules! check {\n            (quiet) => { let _ = bump(1); };\n            (loud) => { assert_near!(bump(1), 3) };\n        }\n        check!(quiet);\n    }\n",
        // A wrapper defined in a block that ended before the invocation.
        "    #[test]\n    fn holds() {\n        {\n            macro_rules! check {\n                () => { assert_near!(bump(1), 3) };\n            }\n        }\n        let _ = bump(1);\n        check!();\n    }\n",
        // A later definition of the wrapper that checks nothing shadows it.
        "    #[test]\n    fn holds() {\n        macro_rules! check {\n            () => { assert_near!(bump(1), 3) };\n        }\n        macro_rules! check {\n            () => {};\n        }\n        let _ = bump(1);\n        check!();\n    }\n",
    ] {
        let (miss, named, _) = read(tests)?;
        if named || miss != Some("no_assertion") {
            return Err(format!(
                "this shape must keep its absence claim: miss={miss:?} named={named}\n{tests}"
            ));
        }
    }
    Ok(())
}
