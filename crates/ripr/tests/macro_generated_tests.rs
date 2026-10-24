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
    let weak = "    #[macro_export]\n    macro_rules! weak {\n        ($left:expr, $right:expr) => {\n            let _ = ($left, $right);\n        };\n    }\n\n";
    let body = "fn weak_alias() {\n            use crate::weak as assert_eq;\n            assert_eq!(gate(10), true);\n        }\n";
    let hand_written = TempRepo::create(&format!("{weak}    #[test]\n    {body}"))?.predicate()?;
    let wrapped = TempRepo::create(&format!(
        "{IDENTITY}{weak}    suite! {{\n        #[test]\n        {body}    }}\n"
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

const SHADOW_FN: &str = "    fn gate(_input: u32) -> bool {\n        true\n    }\n\n";
const GATE_TEST: &str =
    "    #[test]\n    fn ten_is_open() {\n        assert_eq!(gate(10), true);\n    }\n";

/// Wrapped-versus-twin shadowing repros: (label, wrapped source, hand-written
/// twin source). The twin is the same items with the `suite!` wrapper removed.
fn shadow_cases() -> Vec<(&'static str, String, String)> {
    let mk = "    macro_rules! mk {\n        () => {\n            fn gate(_input: u32) -> bool {\n                true\n            }\n        };\n    }\n\n";
    let helper_call =
        "    #[test]\n    fn ten_is_open() {\n        assert_eq!(helper::gate(10), true);\n    }\n";
    let helper = "    mod helper {\n        pub fn gate(_input: u32) -> bool {\n            true\n        }\n    }\n\n";
    let second = "    macro_rules! second {\n        ($($i:item)*) => { $($i)* };\n    }\n\n";
    let both = format!("{IDENTITY}{second}");
    let wrap = |body: &str| format!("{IDENTITY}    suite! {{\n{body}    }}\n");
    vec![
        (
            "sibling fn in the same suite",
            wrap(&format!("{SHADOW_FN}{GATE_TEST}")),
            format!("{SHADOW_FN}{GATE_TEST}"),
        ),
        (
            "module-level fn beside the suite",
            format!("{IDENTITY}{SHADOW_FN}    suite! {{\n{GATE_TEST}    }}\n"),
            format!("{SHADOW_FN}{GATE_TEST}"),
        ),
        (
            "second suite declares the fn",
            format!(
                "{IDENTITY}    suite! {{\n{SHADOW_FN}    }}\n    suite! {{\n{GATE_TEST}    }}\n"
            ),
            format!("{SHADOW_FN}{GATE_TEST}"),
        ),
        (
            "fn in an enclosing module reached through use super::*",
            format!(
                "{IDENTITY}    mod outer {{\n        fn gate(_input: u32) -> bool {{\n            true\n        }}\n\n        mod inner {{\n            use super::*;\n\n            suite! {{\n{GATE_TEST}            }}\n        }}\n    }}\n"
            ),
            format!(
                "    mod outer {{\n        fn gate(_input: u32) -> bool {{\n            true\n        }}\n\n        mod inner {{\n            use super::*;\n\n{GATE_TEST}        }}\n    }}\n"
            ),
        ),
        (
            "named import of a sibling-module fn",
            format!(
                "{IDENTITY}{helper}    use helper::gate;\n\n    suite! {{\n{GATE_TEST}    }}\n"
            ),
            format!("{helper}    use helper::gate;\n\n{GATE_TEST}"),
        ),
        (
            "fn generated by another item macro",
            format!("{IDENTITY}{mk}    mk!();\n\n    suite! {{\n{GATE_TEST}    }}\n"),
            format!("{mk}    mk!();\n\n{GATE_TEST}"),
        ),
        (
            "item macro invoked after the suite",
            format!("{IDENTITY}{mk}    suite! {{\n{GATE_TEST}    }}\n\n    mk!();\n"),
            format!("{mk}{GATE_TEST}\n    mk!();\n"),
        ),
        (
            "attributed item emitted beside the test",
            wrap(&format!("    #[shadow]\n    fn decoy() {{}}\n{GATE_TEST}")),
            format!("    #[shadow]\n    fn decoy() {{}}\n{GATE_TEST}"),
        ),
        (
            "attributed item in a separate sibling call",
            format!(
                "{both}    suite! {{\n{GATE_TEST}    }}\n    second! {{\n    #[shadow]\n    fn decoy() {{}}\n    }}\n"
            ),
            format!("{GATE_TEST}\n    #[shadow]\n    fn decoy() {{}}\n"),
        ),
        (
            "raw-identifier fn in a sibling call",
            format!(
                "{both}    suite! {{\n{GATE_TEST}    }}\n    second! {{\n    fn r#gate(_input: u32) -> bool {{\n        true\n    }}\n    }}\n"
            ),
            format!("{GATE_TEST}\n    fn r#gate(_input: u32) -> bool {{\n        true\n    }}\n"),
        ),
        (
            "test fn named like the callee in a sibling call",
            format!(
                "{both}    suite! {{\n{GATE_TEST}    }}\n    second! {{\n    #[test]\n    fn gate() {{}}\n    }}\n"
            ),
            format!("{GATE_TEST}\n    #[test]\n    fn gate() {{}}\n"),
        ),
        (
            "fn in a different identity macro after the suite",
            format!("{both}    suite! {{\n{GATE_TEST}    }}\n    second! {{\n{SHADOW_FN}    }}\n"),
            format!("{GATE_TEST}\n{SHADOW_FN}"),
        ),
        (
            "module in a different identity macro under a qualified call",
            format!("{both}    suite! {{\n{helper_call}    }}\n    second! {{\n{helper}    }}\n"),
            format!("{helper_call}\n{helper}"),
        ),
        (
            "fn in a nested invocation of the same identity macro",
            format!(
                "{IDENTITY}    suite! {{\n{GATE_TEST}    }}\n    suite! {{\n        suite! {{\n{SHADOW_FN}        }}\n    }}\n"
            ),
            format!("{GATE_TEST}\n{SHADOW_FN}"),
        ),
        (
            "fn in a nested invocation of a different identity macro",
            format!(
                "{both}    suite! {{\n{GATE_TEST}    }}\n    second! {{\n        second! {{\n{SHADOW_FN}        }}\n    }}\n"
            ),
            format!("{GATE_TEST}\n{SHADOW_FN}"),
        ),
        (
            "alias to a non-identity macro under an identity name",
            format!(
                "{IDENTITY}{mk}    use mk as second;\n    mod unrelated {{\n        macro_rules! second {{\n            ($($i:item)*) => {{ $($i)* }};\n        }}\n    }}\n\n    suite! {{\n{GATE_TEST}    }}\n    second!();\n"
            ),
            format!("{mk}{GATE_TEST}\n    mk!();\n"),
        ),
        (
            "path-qualified call into a local module",
            format!("{IDENTITY}{helper}    suite! {{\n{helper_call}    }}\n"),
            format!("{helper}{helper_call}"),
        ),
    ]
}

#[test]
fn a_binding_that_could_shadow_the_callee_never_promotes_a_wrapped_test() -> Result<(), String> {
    // Every wrapped form is refused (no test indexed, NoStaticPath). The
    // hand-written twin's own class is pinned too, so a change on either side
    // is visible; the wrapped class must never be stronger than it.
    let expected_twin = [
        ExposureClass::StaticUnknown,
        ExposureClass::StaticUnknown,
        ExposureClass::StaticUnknown,
        ExposureClass::StaticUnknown,
        ExposureClass::NoStaticPath,
        ExposureClass::Exposed,
        ExposureClass::Exposed,
        ExposureClass::Exposed,
        ExposureClass::Exposed,
        ExposureClass::Exposed,
        ExposureClass::InfectionUnknown,
        ExposureClass::StaticUnknown,
        ExposureClass::NoStaticPath,
        ExposureClass::StaticUnknown,
        ExposureClass::StaticUnknown,
        ExposureClass::Exposed,
        ExposureClass::NoStaticPath,
    ];
    let cases = shadow_cases();
    assert_eq!(cases.len(), expected_twin.len());
    let mut failures = Vec::new();
    for ((label, wrapped_source, twin_source), twin_class) in cases.into_iter().zip(expected_twin) {
        let twin = TempRepo::create(&twin_source)?.predicate()?;
        let wrapped = TempRepo::create(&wrapped_source)?.predicate()?;
        if twin.class != twin_class
            || wrapped.class != ExposureClass::NoStaticPath
            || !wrapped.related_tests.is_empty()
        {
            failures.push(format!(
                "{label}: wrapped {:?} (tests {}) twin {:?}, expected twin {twin_class:?}",
                wrapped.class,
                wrapped.related_tests.len(),
                twin.class
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}
