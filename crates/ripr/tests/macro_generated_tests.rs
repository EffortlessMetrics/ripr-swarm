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
