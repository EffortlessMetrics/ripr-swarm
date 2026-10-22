//! Public-API controls for an exact pin of a returned closure applied inline
//! (#9490, RIPR-SPEC-0265 rule 5).
//!
//! An owner whose whole body is one closure (`move |x| n + x`) returns a
//! function. A test that applies it in the pinned expression
//! (`make_adder(2)(3)`) runs the closure once on non-degenerate inputs, so the
//! exact pin observes a change to the closure body. A zero input, a closure
//! that is not applied, or a closure with a branch keep the earlier reading.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-returned-closure-pin-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"returned-closure\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The one finding of `family` on `line` after `old` becomes `new` there.
fn finding(
    lib: &str,
    line: usize,
    old: &str,
    new: &str,
    family: ProbeFamily,
) -> Result<Finding, String> {
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{line},1 +{line},1 @@
-{old}
+{new}
"
    );
    let repo = TempRepo::create(lib, &diff)?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let mut on_line = output
        .findings
        .iter()
        .filter(|finding| finding.probe.location.line == line && finding.probe.family == family);
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one {family:?} finding on line {line}, got {other:?} in {:?}",
            output.findings
        )),
    }
}

fn limit(finding: &Finding) -> Option<&'static str> {
    finding.static_limit_kind.map(|kind| kind.as_str())
}

const ADDER: &str = "pub fn make_adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| n + x
}";

fn adder(body: &str) -> Result<Finding, String> {
    let lib = format!(
        "{ADDER}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn pinned() {{
        {body}
    }}
}}
"
    );
    finding(
        &lib,
        2,
        "    move |x| x + n",
        "    move |x| n + x",
        ProbeFamily::ReturnValue,
    )
}

#[test]
fn an_exact_pin_of_an_applied_returned_closure_is_credited() -> Result<(), String> {
    let finding = adder("assert_eq!(make_adder(2)(3), 5);")?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?} {:?}",
        finding.class,
        limit(&finding)
    );
    Ok(())
}

#[test]
fn inputs_that_cannot_tell_the_operators_apart_keep_the_gap() -> Result<(), String> {
    // 0 is the identity of `+` and absorbs `*`; a repeated value makes `+` and
    // `*` agree (2 + 2 == 2 * 2); a non-literal argument is unread.
    for body in [
        "assert_eq!(make_adder(0)(7), 7);",
        "assert_eq!(make_adder(7)(0), 7);",
        "assert_eq!(make_adder(2)(2), 4);",
        "let n = 2; assert_eq!(make_adder(n)(3), 5);",
        "assert_eq!(make_adder(2)(1 + 2), 5);",
    ] {
        let finding = adder(body)?;
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{body}: {:?} {:?}",
            finding.class,
            limit(&finding)
        );
    }
    Ok(())
}

#[test]
fn a_closure_that_is_not_applied_keeps_the_gap() -> Result<(), String> {
    for body in [
        "let add = make_adder(2); assert_eq!(add(3), 5);",
        "let _add = make_adder(2); assert_eq!(1 + 1, 2);",
    ] {
        let finding = adder(body)?;
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{body}: {:?} {:?}",
            finding.class,
            limit(&finding)
        );
    }
    Ok(())
}

#[test]
fn a_closure_with_a_branch_keeps_the_gap() -> Result<(), String> {
    let lib = "pub fn make_adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| if x > 0 { n + x } else { n }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned() {
        assert_eq!(make_adder(2)(3), 5);
    }
}
";
    let finding = finding(
        lib,
        2,
        "    move |x| if x > 0 { x + n } else { n }",
        "    move |x| if x > 0 { n + x } else { n }",
        ProbeFamily::ReturnValue,
    )?;
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "{:?} {:?}",
        finding.class,
        limit(&finding)
    );
    Ok(())
}

#[test]
fn a_two_argument_closure_needs_both_arguments_applied() -> Result<(), String> {
    let lib = "pub fn combiner(k: i32) -> impl Fn(i32, i32) -> i32 {
    move |x, y| k * x + y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned() {
        assert_eq!(combiner(2)(3, 4), 10);
    }

    #[test]
    fn partial() {
        let half = combiner(2);
        assert_eq!(half(3, 4), 10);
    }
}
";
    let finding = finding(
        lib,
        2,
        "    move |x, y| x * k + y",
        "    move |x, y| k * x + y",
        ProbeFamily::ReturnValue,
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?} {:?}",
        finding.class,
        limit(&finding)
    );
    Ok(())
}
