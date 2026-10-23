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

/// Actual typed public-path fixture; the diff's old tail is an independently
/// specified alternative rather than an expectation derived from the analyzer.
fn typed_arithmetic_pin(
    prelude: &str,
    kind: &str,
    tail: &str,
    before: &str,
    test: &str,
) -> Result<Finding, String> {
    let lib = format!(
        "{prelude}\npub fn make_adder(n: {kind}) -> impl Fn({kind}) -> {kind} {{\n    {tail}\n}}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn pinned() {{ {test} }}\n}}\n"
    );
    let wanted = format!("    {tail}");
    let line = lib
        .lines()
        .position(|line| line == wanted)
        .ok_or("fixture tail not found")?
        + 1;
    finding(
        &lib,
        line,
        &format!("    {before}"),
        &wanted,
        ProbeFamily::ReturnValue,
    )
}

#[test]
fn effective_body_values_must_discriminate_every_arithmetic_node() -> Result<(), String> {
    for (prelude, tail, before, test) in [
        (
            "const OFFSET: i32 = 3;",
            "move |x| n + (x - OFFSET)",
            "move |x| n - (x - OFFSET)",
            "assert_eq!(make_adder(2)(3), 2);",
        ),
        (
            "const OFFSET: i32 = 3;",
            "move |x| n * (x - OFFSET + 1)",
            "move |x| n / (x - OFFSET + 1)",
            "assert_eq!(make_adder(2)(3), 2);",
        ),
        (
            "",
            "move |x| n + (x - 3)",
            "move |x| n - (x - 3)",
            "assert_eq!(make_adder(2)(3), 2);",
        ),
        (
            "fn OFFSET(x: i32) -> i32 { x - 3 }",
            "move |x| n + OFFSET(x)",
            "move |x| n - OFFSET(x)",
            "assert_eq!(make_adder(2)(3), 2);",
        ),
        (
            "",
            "move |x| n * x",
            "move |x| n / x",
            "assert_eq!(make_adder(5)(1), 5);",
        ),
        (
            "",
            "move |x| n * x",
            "move |x| n / x",
            "assert_eq!(make_adder(5)(-1), -5);",
        ),
        (
            "",
            "move |x| n - x",
            "move |x| n / x",
            "assert_eq!(make_adder(4)(2), 2);",
        ),
        (
            "",
            "move |x| n + x",
            "move |x| x - n",
            "let _2 = 0; assert_eq!(make_adder(_2)(3), 3);",
        ),
        (
            "",
            "move |n| n + 2",
            "move |n| n * 2",
            "assert_eq!(make_adder(3)(2), 4);",
        ),
    ] {
        let found = typed_arithmetic_pin(prelude, "i32", tail, before, test)?;
        assert_ne!(
            found.class,
            ExposureClass::Exposed,
            "{prelude} {tail} {test}: {:?}",
            found.class
        );
    }
    Ok(())
}

#[test]
fn typed_discriminating_arithmetic_and_literal_captures_keep_credit() -> Result<(), String> {
    for (prelude, kind, tail, before, test) in [
        (
            "",
            "i32",
            "move |x| n + x",
            "move |x| n - x",
            "assert_eq!(make_adder(2)(3), 5);",
        ),
        (
            "",
            "i32",
            "move |x| n + x",
            "move |x| n - x",
            "assert_eq!(make_adder(-2)(3), 1);",
        ),
        (
            "",
            "i32",
            "move |x| n + x",
            "move |x| n - x",
            "assert_eq!(make_adder(5)(1), 6);",
        ),
        (
            "",
            "i32",
            "move |x| n / x",
            "move |x| n % x",
            "assert_eq!(make_adder(7)(3), 2);",
        ),
        (
            "",
            "u32",
            "move |x| n + x",
            "move |x| n - x",
            "assert_eq!(make_adder(5)(3), 8);",
        ),
        (
            "",
            "f64",
            "move |x| n + x",
            "move |x| n - x",
            "assert_eq!(make_adder(2.0)(3.0), 5.0);",
        ),
        (
            "",
            "f32",
            "move |x| n + x",
            "move |x| n - x",
            "assert_eq!(make_adder(16_777_216.0)(3.0), 16_777_220.0);",
        ),
        (
            "const TWO: i32 = 2;",
            "i32",
            "move |x| n + x * TWO",
            "move |x| n - x * TWO",
            "assert_eq!(make_adder(2)(3), 8);",
        ),
        (
            "mod sibling { pub const TWO: i32 = 0; }\nconst TWO: i32 = 2;",
            "i32",
            "move |x| n + x * TWO",
            "move |x| n - x * TWO",
            "assert_eq!(make_adder(2)(3), 8);",
        ),
    ] {
        let found = typed_arithmetic_pin(prelude, kind, tail, before, test)?;
        assert_eq!(
            found.class,
            ExposureClass::Exposed,
            "{prelude} {kind} {tail} {test}: {:?} {:?}",
            found.class,
            limit(&found)
        );
    }
    Ok(())
}

#[test]
fn unsupported_numeric_authority_or_arithmetic_stays_withheld() -> Result<(), String> {
    for (prelude, kind, tail, before, test) in [
        (
            "type i32 = u8;",
            "i32",
            "move |x| n + x",
            "move |x| n - x",
            "assert_eq!(make_adder(2)(3), 5);",
        ),
        (
            "const TWO: i32 = 1 + 1;",
            "i32",
            "move |x| n + x * TWO",
            "move |x| n - x * TWO",
            "assert_eq!(make_adder(2)(3), 8);",
        ),
        (
            "",
            "i8",
            "move |x| n + x",
            "move |x| n - x",
            "assert_eq!(make_adder(120)(3), 123);",
        ),
        (
            "",
            "f64",
            "move |x| n + x",
            "move |x| n * x",
            "assert_eq!(make_adder(3.0)(1.5), 4.5);",
        ),
        (
            "",
            "f32",
            "move |x| n + x",
            "move |x| n * x",
            "assert_eq!(make_adder(16_777_216.0)(1.0), 16_777_216.0);",
        ),
        (
            "",
            "i32",
            "move |x| n + x as i32",
            "move |x| n - x as i32",
            "assert_eq!(make_adder(2)(3), 5);",
        ),
    ] {
        let found = typed_arithmetic_pin(prelude, kind, tail, before, test)?;
        assert_ne!(
            found.class,
            ExposureClass::Exposed,
            "{prelude} {kind} {tail} {test}: {:?}",
            found.class
        );
    }
    Ok(())
}

#[test]
fn qualified_owner_survives_a_rebound_local_callee() -> Result<(), String> {
    let found = adder(
        "let make_adder = |_| |_: i32| 5; let _unused = make_adder(9); assert_eq!(crate::make_adder(2)(3), 5);",
    )?;
    assert_eq!(found.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn raw_primitive_bindings_do_not_supply_builtin_widths() -> Result<(), String> {
    let found = typed_arithmetic_pin(
        "type r#i32 = i8;",
        "i32",
        "move |x| n + x",
        "move |x| n * x",
        "assert_eq!(make_adder(20)(28), 48);",
    )?;
    assert_ne!(found.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn unrelated_type_annotations_do_not_shadow_the_primitive() -> Result<(), String> {
    let found = typed_arithmetic_pin(
        "const UNRELATED: core::primitive::i32 = 0;",
        "i32",
        "move |x| n + x",
        "move |x| n - x",
        "assert_eq!(make_adder(2)(3), 5);",
    )?;
    assert_eq!(found.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_parent_alias_does_not_rebind_a_child_module_primitive() -> Result<(), String> {
    let lib = "type i32 = u8;\nmod factory {\n    pub fn make_adder(n: i32) -> impl Fn(i32) -> i32 {\n        move |x| n + x\n    }\n    #[cfg(test)]\n    mod tests {\n        use super::*;\n        #[test]\n        fn pinned() { assert_eq!(make_adder(2)(3), 5); }\n    }\n}\n";
    let found = finding(
        lib,
        4,
        "        move |x| n - x",
        "        move |x| n + x",
        ProbeFamily::ReturnValue,
    )?;
    assert_eq!(found.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn import_syntax_cannot_hide_a_primitive_binding() -> Result<(), String> {
    for import in [
        "use aliases::Int as i32;",
        "use aliases::Int as /* binding */ i32;",
        "use aliases::{Int as r#i32};",
        "use aliases::i32;",
        "use aliases::i32::{self};",
    ] {
        let prelude = format!("mod aliases {{ pub type Int = i8; pub type i32 = i8; }}\n{import}");
        let found = typed_arithmetic_pin(
            &prelude,
            "i32",
            "move |x| n + x",
            "move |x| n * x",
            "assert_eq!(make_adder(20)(28), 48);",
        )?;
        assert_ne!(found.class, ExposureClass::Exposed, "{import}");
    }
    Ok(())
}

#[test]
fn irrelevant_imports_traits_and_raw_literal_constants_keep_credit() -> Result<(), String> {
    for prelude in [
        "use core::primitive::i32 as Scalar;",
        "mod markers { pub struct Marker; } use crate::{markers::Marker};",
        "mod markers { pub struct Marker; } use self::{markers::Marker};",
        "trait FnOnce {}",
        "const r#TWO: i32 = 2;",
    ] {
        let tail = if prelude.starts_with("const") {
            "move |x| n + x * TWO"
        } else {
            "move |x| n + x"
        };
        let before = tail.replacen('+', "-", 1);
        let test = if prelude.starts_with("const") {
            "assert_eq!(make_adder(2)(3), 8);"
        } else {
            "assert_eq!(make_adder(2)(3), 5);"
        };
        let found = typed_arithmetic_pin(prelude, "i32", tail, &before, test)?;
        assert_eq!(found.class, ExposureClass::Exposed, "{prelude}");
    }
    Ok(())
}

#[test]
fn raw_parameter_spelling_does_not_defeat_closure_shadowing() -> Result<(), String> {
    let lib = "pub fn make_adder(r#n: i32) -> impl Fn(i32) -> i32 {\n    move |n| r#n + 2\n}\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn pinned() { assert_eq!(make_adder(3)(2), 4); }\n}\n";
    let found = finding(
        lib,
        2,
        "    move |n| r#n * 2",
        "    move |n| r#n + 2",
        ProbeFamily::ReturnValue,
    )?;
    assert_ne!(found.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn grouped_parent_import_does_not_hide_builtin_numeric_authority() -> Result<(), String> {
    let lib = "struct Marker;\nmod factory {\n    use super::{Marker};\n    pub fn make_adder(n: i32) -> impl Fn(i32) -> i32 {\n        move |x| n + x\n    }\n    #[cfg(test)]\n    mod tests {\n        use super::*;\n        #[test]\n        fn pinned() { assert_eq!(make_adder(2)(3), 5); }\n    }\n}\n";
    let found = finding(
        lib,
        5,
        "        move |x| n - x",
        "        move |x| n + x",
        ProbeFamily::ReturnValue,
    )?;
    assert_eq!(found.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn token_coincidence_cannot_replace_a_returned_closure_pin() -> Result<(), String> {
    for (prelude, test, exposed) in [
        (
            "",
            "let captured = 5; assert_eq!(make_adder(5)(1), captured);",
            false,
        ),
        (
            "",
            "let _unused = make_adder(5); let argument = 5; assert_eq!(argument, 5);",
            false,
        ),
        (
            "type i32 = u8;",
            "let captured = 5; assert_eq!(make_adder(5)(1), captured);",
            false,
        ),
        ("", "assert_eq!(make_adder(2)(3), 6);", true),
    ] {
        let lib = format!(
            "{prelude}\npub fn make_adder(captured: i32) -> impl Fn(i32) -> i32 {{\n    move |argument| captured * argument\n}}\n#[test]\nfn pinned() {{ {test} }}\n"
        );
        let result = finding(
            &lib,
            3,
            "    move |argument| captured / argument",
            "    move |argument| captured * argument",
            ProbeFamily::ReturnValue,
        )?;
        assert_eq!(
            result.class == ExposureClass::Exposed,
            exposed,
            "{prelude} {test}: {:?}",
            result.class
        );
    }
    Ok(())
}

// Attribution controls for RIPR-SPEC-0265 rule 5. Reach and a strong oracle
// must observe the changed closure result, rather than a replaced value or
// a same-name rival. Native +/- fixture pairs are retained separately.
mod returned_closure_attribution {
    use super::*;

    fn attribution_finding(body: &str) -> Result<Finding, String> {
        let lib = format!(
            r#"pub fn make_adder(captured: i32) -> impl Fn(i32) -> i32 {{
    move |argument| captured + argument
}}

mod rival {{
    pub fn make_adder(captured: i32) -> impl Fn(i32) -> i32 {{
        move |argument| captured + argument
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;
    #[test]
    fn pinned() {{
        {body}
    }}
}}
"#
        );
        finding(
            &lib,
            2,
            "    move |argument| captured - argument",
            "    move |argument| captured + argument",
            ProbeFamily::ReturnValue,
        )
    }

    #[test]
    fn direct_result_observed() -> Result<(), String> {
        let finding = attribution_finding(r#"assert_eq!(make_adder(2)(3), 5);"#)?;
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
    fn qualified_result_observed_despite_rival_import() -> Result<(), String> {
        let finding = attribution_finding(
            r#"use crate::rival::make_adder;
let _rival = make_adder(2);
assert_eq!(crate::make_adder(2)(3), 5);"#,
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

    #[test]
    fn applied_result_overwritten() -> Result<(), String> {
        let finding = attribution_finding(
            r#"let mut captured = make_adder(2)(3);
std::hint::black_box(captured);
captured = 5;
assert_eq!(captured, 5);"#,
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
    fn applied_result_shadowed() -> Result<(), String> {
        let finding = attribution_finding(
            r#"let captured = make_adder(2)(3);
std::hint::black_box(captured);
let captured = 5;
assert_eq!(captured, 5);"#,
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
    fn returned_closure_replaced_before_invocation() -> Result<(), String> {
        let finding = attribution_finding(
            r#"let mut captured: Box<dyn Fn(i32) -> i32> = Box::new(make_adder(2));
std::hint::black_box(&captured);
captured = Box::new(|argument| argument + 2);
assert_eq!(captured(3), 5);"#,
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
    fn qualified_rival_assertion_after_discarded_owner_result() -> Result<(), String> {
        let finding = attribution_finding(
            r#"let _ignored = crate::make_adder(2)(3);
let captured = 5;
assert_eq!(crate::rival::make_adder(2)(3), captured);"#,
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
    fn imported_rival_assertion_after_discarded_owner_result() -> Result<(), String> {
        let finding = attribution_finding(
            r#"use crate::rival::make_adder;
let _ignored = crate::make_adder(2)(3);
let captured = 5;
assert_eq!(make_adder(2)(3), captured);"#,
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
    fn exact_assertion_inside_uninvoked_closure() -> Result<(), String> {
        let finding = attribution_finding(
            r#"let _deferred = || assert_eq!(make_adder(2)(3), 5);
let captured = 5;
assert_eq!(captured, 5);"#,
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
}
