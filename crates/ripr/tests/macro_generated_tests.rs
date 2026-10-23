//! Public-API controls for same-file `macro_rules!` test generators (#5334).
//!
//! A test written through a same-file macro reads like the same test
//! written by hand. Shapes the bounded expander refuses, and a generator
//! that is never invoked, add no test.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const GATE: &str = "pub fn gate(input: u32) -> bool {\n    input >= 10\n}\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn gate(input: u32) -> bool {
-    input > 10
+    input >= 10
 }
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(tests: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-macro-generated-tests-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"macro-generated-tests\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(
            root.join("src/lib.rs"),
            format!("{GATE}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{tests}}}\n"),
        )
        .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    /// The finding for the changed `input >= 10` predicate.
    fn predicate(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut predicates = output.findings.into_iter().filter(|finding| {
            finding.probe.family == ProbeFamily::Predicate && finding.probe.location.line == 2
        });
        match (predicates.next(), predicates.next()) {
            (Some(finding), None) => Ok(finding),
            other => Err(format!(
                "premise: exactly one predicate finding on line 2, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn state(stage: &impl std::fmt::Debug) -> String {
    format!("{stage:?}")
}

const GENERATOR: &str = "    macro_rules! gate_case {\n        ($name:ident, $input:expr, $want:expr) => {\n            #[test]\n            fn $name() {\n                assert_eq!(gate($input), $want);\n            }\n        };\n    }\n\n";

#[test]
fn a_macro_generated_test_reads_like_the_hand_written_test() -> Result<(), String> {
    let hand_written = TempRepo::create(
        "    #[test]\n    fn nine_is_closed() {\n        assert_eq!(gate(9), false);\n    }\n\n    #[test]\n    fn ten_is_open() {\n        assert_eq!(gate(10), true);\n    }\n",
    )?
    .predicate()?;
    assert_eq!(
        hand_written.class,
        ExposureClass::Exposed,
        "premise: the hand-written boundary tests expose the predicate: {hand_written:?}"
    );

    let generated = TempRepo::create(&format!(
        "{GENERATOR}    gate_case!(nine_is_closed, 9, false);\n    gate_case!(ten_is_open, 10, true);\n"
    ))?
    .predicate()?;
    assert_eq!(state(&generated.ripr.reach.state), "Yes", "{generated:?}");
    assert_eq!(generated.class, hand_written.class, "{generated:?}");
    let mut names: Vec<_> = generated
        .related_tests
        .iter()
        .map(|test| (test.name.as_str(), test.line))
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![("nine_is_closed", 18), ("ten_is_open", 19)],
        "generated tests point at their invocations: {generated:?}"
    );
    Ok(())
}

#[test]
fn refused_shapes_and_uninvoked_generators_add_no_test() -> Result<(), String> {
    // A repetition matcher is outside the bounded shape.
    let repetition = TempRepo::create(
        "    macro_rules! gate_cases {\n        ($($name:ident: $input:expr => $want:expr),*) => {\n            $( #[test] fn $name() { assert_eq!(gate($input), $want); } )*\n        };\n    }\n\n    gate_cases!(nine_is_closed: 9 => false, ten_is_open: 10 => true);\n",
    )?
    .predicate()?;
    assert!(repetition.related_tests.is_empty(), "{repetition:?}");
    assert_ne!(state(&repetition.ripr.reach.state), "Yes", "{repetition:?}");

    let uninvoked = TempRepo::create(GENERATOR)?.predicate()?;
    assert!(uninvoked.related_tests.is_empty(), "{uninvoked:?}");
    assert_ne!(state(&uninvoked.ripr.reach.state), "Yes", "{uninvoked:?}");
    Ok(())
}

#[test]
fn a_deferred_assertion_in_a_generated_test_is_refused_like_a_hand_written_one()
-> Result<(), String> {
    // The closure is never called, so its assertion never runs.
    let hand_written = TempRepo::create(
        "    #[test]\n    fn nine_is_closed() {\n        let _check = || assert_eq!(gate(9), false);\n    }\n\n    #[test]\n    fn ten_is_open() {\n        let _check = || assert_eq!(gate(10), true);\n    }\n",
    )?
    .predicate()?;
    assert_ne!(
        hand_written.class,
        ExposureClass::Exposed,
        "premise: an uncalled closure's assertion is not credited: {hand_written:?}"
    );

    let generated = TempRepo::create(
        "    macro_rules! gate_case {\n        ($name:ident, $input:expr, $want:expr) => {\n            #[test]\n            fn $name() {\n                let _check = || assert_eq!(gate($input), $want);\n            }\n        };\n    }\n\n    gate_case!(nine_is_closed, 9, false);\n    gate_case!(ten_is_open, 10, true);\n",
    )?
    .predicate()?;
    assert_eq!(state(&generated.ripr.reach.state), "Yes", "{generated:?}");
    assert_eq!(generated.class, hand_written.class, "{generated:?}");
    Ok(())
}

#[test]
fn a_shadowing_let_in_a_generated_test_reads_like_the_hand_written_one() -> Result<(), String> {
    // The local closure, not the changed function, is what the test calls.
    let hand_written = TempRepo::create(
        "    #[test]\n    fn nine_is_closed() {\n        let gate = |_: u32| false;\n        assert_eq!(gate(9), false);\n    }\n\n    #[test]\n    fn ten_is_open() {\n        let gate = |_: u32| false;\n        assert_eq!(gate(10), false);\n    }\n",
    )?
    .predicate()?;

    let generated = TempRepo::create(
        "    macro_rules! gate_case {\n        ($name:ident, $input:expr, $want:expr) => {\n            #[test]\n            fn $name() {\n                let gate = |_: u32| false;\n                assert_eq!(gate($input), $want);\n            }\n        };\n    }\n\n    gate_case!(nine_is_closed, 9, false);\n    gate_case!(ten_is_open, 10, false);\n",
    )?
    .predicate()?;
    // Parity, not a verdict on shadowing: the generated test must read
    // exactly as the hand-written one does, whatever that reading is.
    assert_eq!(generated.class, hand_written.class, "{generated:?}");
    assert_eq!(
        state(&generated.ripr.reach.state),
        state(&hand_written.ripr.reach.state),
        "{generated:?}"
    );
    let names = |finding: &Finding| {
        let mut names: Vec<_> = finding
            .related_tests
            .iter()
            .map(|test| test.name.clone())
            .collect();
        names.sort_unstable();
        names
    };
    assert_eq!(names(&generated), names(&hand_written), "{generated:?}");
    Ok(())
}

#[test]
fn a_cfg_gated_invocation_adds_no_test() -> Result<(), String> {
    let gated = TempRepo::create(&format!(
        "{GENERATOR}    #[cfg(any())]\n    gate_case!(nine_is_closed, 9, false);\n"
    ))?
    .predicate()?;
    assert!(gated.related_tests.is_empty(), "{gated:?}");
    assert_ne!(gated.class, ExposureClass::Exposed, "{gated:?}");
    Ok(())
}

const IDENTITY: &str = "    macro_rules! suite {\n        ($($t:tt)*) => { $($t)* };\n    }\n\n";
const SUITE_BODY: &str = "        #[test]\n        fn nine_is_closed() {\n            assert_eq!(gate(9), false);\n        }\n\n        #[test]\n        fn ten_is_open() {\n            assert_eq!(gate(10), true);\n        }\n";

#[test]
fn literal_tests_in_an_identity_macro_read_like_the_hand_written_tests() -> Result<(), String> {
    let hand_written = TempRepo::create(
        "    #[test]\n    fn nine_is_closed() {\n        assert_eq!(gate(9), false);\n    }\n\n    #[test]\n    fn ten_is_open() {\n        assert_eq!(gate(10), true);\n    }\n",
    )?
    .predicate()?;
    assert_eq!(
        hand_written.class,
        ExposureClass::Exposed,
        "{hand_written:?}"
    );

    let wrapped =
        TempRepo::create(&format!("{IDENTITY}    suite! {{\n{SUITE_BODY}    }}\n"))?.predicate()?;
    assert_eq!(state(&wrapped.ripr.reach.state), "Yes", "{wrapped:?}");
    assert_eq!(wrapped.class, hand_written.class, "{wrapped:?}");
    let mut names: Vec<_> = wrapped
        .related_tests
        .iter()
        .map(|test| test.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, vec!["nine_is_closed", "ten_is_open"], "{wrapped:?}");
    Ok(())
}

#[test]
fn literal_tests_in_a_macro_that_is_not_the_identity_add_no_test() -> Result<(), String> {
    // Controls for the same tokens: a macro that drops them, a macro
    // defined in another file (here: not defined at all), and a gated
    // invocation each leave the tests unindexed.
    let cases = [
        "    macro_rules! suite {\n        ($($t:tt)*) => {};\n    }\n\n    suite! {\n".to_string()
            + SUITE_BODY
            + "    }\n",
        "    other_suite! {\n".to_string() + SUITE_BODY + "    }\n",
        format!("{IDENTITY}    #[cfg(any())]\n    suite! {{\n{SUITE_BODY}    }}\n"),
    ];
    for tests in cases {
        let finding = TempRepo::create(&tests)?.predicate()?;
        assert!(finding.related_tests.is_empty(), "{tests}\n{finding:?}");
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{tests}\n{finding:?}"
        );
    }
    Ok(())
}

#[test]
fn a_function_local_weak_alias_in_a_wrapped_test_is_never_stronger_than_by_hand()
-> Result<(), String> {
    // `assert_eq` is rebound to a weak helper inside the test body. By hand
    // the rebinding is visible; wrapped, it must not let the exemption turn
    // the call into a recognized strong oracle.
    let body = "fn weak_alias() {\n            use crate::weak::weak as assert_eq;\n            assert_eq!(gate(10), true);\n        }\n";
    let hand_written = TempRepo::create(&format!("    #[test]\n    {body}"))?.predicate()?;
    let wrapped = TempRepo::create(&format!(
        "{IDENTITY}    suite! {{\n        #[test]\n        {body}    }}\n"
    ))?
    .predicate()?;
    for finding in [&hand_written, &wrapped] {
        assert!(
            !matches!(
                finding.class,
                ExposureClass::Exposed | ExposureClass::WeaklyExposed
            ),
            "{finding:?}"
        );
    }
    assert_eq!(
        hand_written.class,
        ExposureClass::ReachableUnrevealed,
        "{hand_written:?}"
    );
    // Fail closed: never stronger than the hand-written twin.
    assert!(
        matches!(
            wrapped.class,
            ExposureClass::ReachableUnrevealed | ExposureClass::StaticUnknown
        ),
        "{wrapped:?}"
    );
    Ok(())
}

#[test]
fn an_import_alias_after_the_identity_definition_adds_no_test() -> Result<(), String> {
    // `use other::real as suite;` rebinds the name after the definition, so
    // the later invocation is not provably the identity.
    let cases = [
        format!("{IDENTITY}    use other::real as suite;\n    suite! {{\n{SUITE_BODY}    }}\n"),
        format!(
            "{IDENTITY}    mod inner {{\n        use other::real as suite;\n        suite! {{\n{SUITE_BODY}        }}\n    }}\n"
        ),
    ];
    for tests in cases {
        let finding = TempRepo::create(&tests)?.predicate()?;
        assert!(finding.related_tests.is_empty(), "{tests}\n{finding:?}");
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{tests}\n{finding:?}"
        );
    }
    Ok(())
}

#[test]
fn a_local_fn_shadowing_the_callee_never_promotes_a_wrapped_test() -> Result<(), String> {
    // A local `fn gate` in the same module shadows the glob-imported real
    // `gate`. Each expansion is summarised alone, so the wrapped test must
    // be refused rather than credited with the real callee.
    let shadow = "    fn gate(_input: u32) -> bool {\n        true\n    }\n\n";
    let test = "    #[test]\n    fn ten_is_open() {\n        assert_eq!(gate(10), true);\n    }\n";
    let cases = [
        // (wrapped, hand-written twin)
        (
            format!("{IDENTITY}    suite! {{\n{shadow}{test}    }}\n"),
            format!("{shadow}{test}"),
        ),
        (
            format!("{IDENTITY}{shadow}    suite! {{\n{test}    }}\n"),
            format!("{shadow}{test}"),
        ),
        (
            format!("{IDENTITY}    suite! {{\n{shadow}    }}\n    suite! {{\n{test}    }}\n"),
            format!("{shadow}{test}"),
        ),
    ];
    for (wrapped_source, twin_source) in cases {
        let twin = TempRepo::create(&twin_source)?.predicate()?;
        let wrapped = TempRepo::create(&wrapped_source)?.predicate()?;
        assert!(
            !matches!(
                wrapped.class,
                ExposureClass::Exposed | ExposureClass::WeaklyExposed
            ) || wrapped.class == twin.class,
            "{wrapped_source}\nwrapped {wrapped:?}\ntwin {twin:?}"
        );
        assert_ne!(wrapped.class, ExposureClass::Exposed, "{wrapped_source}");
    }
    Ok(())
}
