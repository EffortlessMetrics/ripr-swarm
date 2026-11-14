//! Public-API controls for the RIPR-SPEC-0197 let-bound lone-equality pin
//! (#14085).
//!
//! `let v = owner(..); assert!(v == 4);` fails exactly when
//! `assert_eq!(v, 4)` does, so the #6974 binding rules read the binding of a
//! plain one-`==` `assert!` as they read an `assert_eq!` operand. Every
//! shape those rules or the lone-equality rule refuse stays unconfirmed.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// A scalar owner: `score` on lines 1-3, edited from `x * 3`.
const SCORE: (&str, &str) = (
    "pub fn score(x: u32) -> u32 {
    x * 2
}
",
    "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn score(x: u32) -> u32 {
-    x * 3
+    x * 2
 }
",
);

/// A non-scalar owner: `label` on lines 1-3, edited from `\"a{x}\"`.
const LABEL: (&str, &str) = (
    "pub fn label(x: u32) -> String {
    format!(\"n{x}\")
}
",
    "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn label(x: u32) -> String {
-    format!(\"a{x}\")
+    format!(\"n{x}\")
 }
",
);

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    /// `owner` followed by a test module holding `helpers` and one test
    /// whose body is `body`, declared with `signature`.
    fn create(
        (owner, diff): (&str, &str),
        helpers: &str,
        signature: &str,
        body: &str,
    ) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-let-bound-lone-equality-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let lib = format!(
            "{owner}
#[cfg(test)]
mod tests {{
    use super::*;
{helpers}    #[test]
    fn pins_owner(){signature} {{
{body}    }}
}}
"
        );
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"lone-equality\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    /// The one return-value finding on the changed line 2.
    fn changed(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        // The removed line may keep its own static_unknown finding.
        let mut on_line = output.findings.iter().filter(|finding| {
            finding.probe.location.line == 2 && finding.probe.family == ProbeFamily::ReturnValue
        });
        match (on_line.next(), on_line.next()) {
            (Some(finding), None) => Ok(finding.clone()),
            other => Err(format!(
                "premise: exactly one return_value finding on line 2, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn class(
    owner: (&str, &str),
    helpers: &str,
    signature: &str,
    body: &str,
) -> Result<ExposureClass, String> {
    Ok(TempRepo::create(owner, helpers, signature, body)?
        .changed()?
        .class)
}

#[test]
fn a_let_bound_result_compared_by_a_lone_equality_pins_the_owner() -> Result<(), String> {
    for (name, owner, body) in [
        (
            "the assert_eq! twin (control)",
            SCORE,
            "        let v = score(2);
        assert_eq!(v, 4);
",
        ),
        (
            "the inline lone equality (control)",
            SCORE,
            "        assert!(score(2) == 4);
",
        ),
        (
            "binding on the left",
            SCORE,
            "        let v = score(2);
        assert!(v == 4);
",
        ),
        (
            "binding on the right, with a message",
            SCORE,
            "        let v = score(2);
        assert!(4 == v, \"wrong score\");
",
        ),
        (
            "a message that only renders the binding inline",
            SCORE,
            "        let v = score(2);
        assert!(v == 4, \"v = {v}\");
",
        ),
        (
            "a scalar binding also read by an earlier plain assertion",
            SCORE,
            "        let v = score(2);
        assert!(v > 0);
        assert!(v == 4);
",
        ),
        (
            "a non-scalar binding asserted by the next statement",
            LABEL,
            "        let v = label(2);
        assert!(v == \"n2\");
",
        ),
    ] {
        let class = class(owner, "", "", body)?;
        assert_eq!(class, ExposureClass::Exposed, "{name}");
    }
    Ok(())
}

#[test]
fn refused_bindings_and_conditions_lend_no_lone_equality_pin() -> Result<(), String> {
    const TOUCH: &str = "    fn touch() {}\n";
    for (name, owner, helpers, signature, body) in [
        (
            "a mutable binding",
            SCORE,
            "",
            "",
            "        let mut v = score(2);
        v += 0;
        assert!(v == 4);
",
        ),
        (
            "the binding named again in a message argument",
            SCORE,
            "",
            "",
            "        let v = score(2);
        assert!(v == 4, \"{}\", v);
",
        ),
        (
            "a use of the binding outside an assertion",
            SCORE,
            "",
            "",
            "        let v = score(2);
        let w = v + 0;
        assert!(v == 4);
        let _ = w;
",
        ),
        (
            "a shadowing second binding",
            SCORE,
            "",
            "",
            "        let v = score(2);
        let v = 4;
        assert!(v == 4);
",
        ),
        (
            "an inner block that rebinds the name",
            SCORE,
            "",
            "",
            "        let v = score(2);
        {
            let v = 4;
            assert!(v == 4);
        }
        let _ = v;
",
        ),
        (
            "an inequality",
            SCORE,
            "",
            "",
            "        let v = score(2);
        assert!(v != 4);
",
        ),
        (
            "an ordering",
            SCORE,
            "",
            "",
            "        let v = score(2);
        assert!(v <= 4);
",
        ),
        (
            "a conjunction",
            SCORE,
            "",
            "",
            "        let v = score(2);
        assert!(v == 4 && true);
",
        ),
        (
            "a disjunction",
            SCORE,
            "",
            "",
            "        let v = score(2);
        assert!(v == 4 || true);
",
        ),
        (
            "a negated equality",
            SCORE,
            "",
            "",
            "        let v = score(2);
        assert!(!(v == 4));
",
        ),
        (
            "the binding compared with itself",
            SCORE,
            "",
            "",
            "        let v = score(2);
        assert!(v == v);
",
        ),
        (
            "an expected side that calls the owner",
            SCORE,
            "",
            "",
            "        let v = score(2);
        assert!(v == score(2));
",
        ),
        (
            "an expected side bound from the owner",
            SCORE,
            "",
            "",
            "        let v = score(2);
        let w = score(2);
        assert!(v == w);
",
        ),
        (
            "an operand that is more than the binding",
            SCORE,
            "",
            "",
            "        let v = score(2);
        assert!(v + 0 == 4);
",
        ),
        (
            "an input the edit cannot change",
            SCORE,
            "",
            "",
            "        let v = score(0);
        assert!(v == 0);
",
        ),
        (
            "an Err-return guard over the binding",
            SCORE,
            "",
            " -> Result<(), String>",
            "        let v = score(2);
        if v != 4 {
            return Err(\"bad\".to_string());
        }
        Ok(())
",
        ),
        (
            "a non-scalar binding with a statement before the assertion",
            LABEL,
            TOUCH,
            "",
            "        let v = label(2);
        touch();
        assert!(v == \"n2\");
",
        ),
    ] {
        let class = class(owner, helpers, signature, body)?;
        assert_eq!(class, ExposureClass::WeaklyExposed, "{name}");
    }
    Ok(())
}

#[test]
fn an_equality_the_test_never_runs_stays_unrevealed() -> Result<(), String> {
    for (name, body) in [
        (
            "debug_assert! is compiled out of release tests",
            "        let v = score(2);
        debug_assert!(v == 4);
",
        ),
        (
            "an assertion inside a closure that is never called",
            "        let v = score(2);
        let f = || assert!(v == 4);
        let _ = f;
",
        ),
    ] {
        let class = class(SCORE, "", "", body)?;
        assert_eq!(class, ExposureClass::ReachableUnrevealed, "{name}");
    }
    Ok(())
}
