//! #13037: a changed computed return (`base * 2` under `if express`) read
//! `weakly_exposed` in `ripr check` even when a test pinned the exact value
//! that branch returns (`assert_eq!(shipping_cost(100, 10, true), 800)`):
//! no assertion text names `base * 2`, and only a token match could confirm
//! observation. The repo seam view `ripr repair` reads already credited it.
//!
//! Credit now also comes from an exact pin of a bare owner call whose literal
//! arguments statically run the owner into the branch whose tail returns the
//! changed expression. Every statement before that tail must fold (a `let`
//! that calls nothing, or an `if` guard whose condition folds false), and
//! every condition on the path must fold. Any shape the fold cannot read
//! keeps today's `weakly_exposed`.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const AFTER: &str = "pub const FREE_OVER_CENTS: u32 = 5_000;

pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
    if order_cents >= FREE_OVER_CENTS && !express {
        return 0;
    }
    let base = if weight_g <= 500 { 400 } else { 900 };
    if express {
        base * 2
    } else {
        base
    }
}
";

const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -8,3 +8,3 @@ pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
     if express {
-        base * 3
+        base * 2
     } else {
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(source: &str, tests: &str) -> Result<Self, String> {
        Self::with_patch(source, tests, PATCH)
    }

    fn with_patch(source: &str, tests: &str, patch: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-computed-tail-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"computed-tail\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        let lib =
            format!("{source}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{tests}\n}}\n");
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), patch)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn computed_finding(&self) -> Result<Finding, String> {
        self.return_finding(9, "base * 2")
    }

    fn return_finding(&self, line: usize, expression: &str) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut computed = output.findings.into_iter().filter(|finding| {
            finding.probe.family == ProbeFamily::ReturnValue
                && finding.probe.location.line == line
                && finding.probe.expression == expression
        });
        match (computed.next(), computed.next()) {
            (Some(finding), None) => Ok(finding),
            other => Err(format!(
                "premise: exactly one return_value finding for `{expression}` on line {line}, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn test_fn(body: &str) -> String {
    format!("    #[test]\n    fn pins() {{\n        {body}\n    }}\n")
}

fn class_of(source: &str, body: &str) -> Result<ExposureClass, String> {
    Ok(TempRepo::create(source, &test_fn(body))?
        .computed_finding()?
        .class)
}

#[test]
fn premise_the_patch_changes_only_the_computed_tail() {
    let line = AFTER.lines().nth(8);
    assert_eq!(line, Some("        base * 2"));
}

#[test]
fn an_exact_pin_from_a_call_that_reaches_the_branch_is_exposed() -> Result<(), String> {
    assert_eq!(
        class_of(AFTER, "assert_eq!(shipping_cost(100, 10, true), 800);")?,
        ExposureClass::Exposed
    );
    // The guard before the tail reads the same-file constant and folds false.
    assert_eq!(
        class_of(AFTER, "assert_eq!(shipping_cost(600, 9000, true), 1800);")?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_pin_from_the_other_branch_only_is_not_credited() -> Result<(), String> {
    let class = class_of(AFTER, "assert_eq!(shipping_cost(100, 10, false), 400);")?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_inequality_pins_no_value() -> Result<(), String> {
    let class = class_of(AFTER, "assert_ne!(shipping_cost(100, 10, true), 1200);")?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_earlier_return_the_call_takes_is_not_credited() -> Result<(), String> {
    let source = AFTER.replace(
        "order_cents >= FREE_OVER_CENTS && !express",
        "order_cents >= FREE_OVER_CENTS",
    );
    let class = class_of(&source, "assert_eq!(shipping_cost(100, 9000, true), 0);")?;
    assert_ne!(class, ExposureClass::Exposed);
    // Control: the same owner with a call under the threshold is credited.
    assert_eq!(
        class_of(&source, "assert_eq!(shipping_cost(100, 10, true), 800);")?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn shapes_the_fold_cannot_read_keep_todays_reading() -> Result<(), String> {
    let call = "assert_eq!(shipping_cost(100, 10, true), 800);";
    // A computed constant does not resolve, so the guard does not fold.
    let computed_constant = AFTER.replace(
        "pub const FREE_OVER_CENTS: u32 = 5_000;",
        "pub const FREE_OVER_CENTS: u32 = 5 * 1_000;",
    );
    // A `let` that calls a function may not return (or may panic).
    let called = AFTER.replace(
        "let base = if weight_g <= 500 { 400 } else { 900 };",
        "let base = rate(weight_g);",
    ) + "\nfn rate(weight_g: u32) -> u32 {\n    if weight_g <= 500 { 400 } else { 900 }\n}\n";
    // A rebound parameter makes the call's arguments stale (kept on the
    // `let` line so the changed tail stays on the patched line).
    let rebound = AFTER.replace(
        "    let base = if",
        "    let weight_g = weight_g + 1; let base = if",
    );
    // A same-named stub could take the bare call instead of the owner.
    let stub = AFTER.to_string()
        + "\npub mod legacy {\n    pub fn shipping_cost(_w: u32, _o: u32, _e: bool) -> u32 {\n        800\n    }\n}\n";
    // A `#[cfg]` can swap the statement the fold reads.
    let attributed = AFTER.replace(
        "    let base = if",
        "    #[cfg(not(test))] let base = 0; let base = if",
    );
    // A constant also bound by a `use` may not be the one the owner reads.
    let imported = AFTER.replace(
        "pub const FREE_OVER_CENTS: u32 = 5_000;",
        "pub const FREE_OVER_CENTS: u32 = 5_000; use crate::FREE_OVER_CENTS as _;",
    );
    for (label, source) in [
        ("same-named function", stub),
        ("attributed statement", attributed),
        ("constant named in a use", imported),
        ("computed constant", computed_constant),
        ("called initializer", called),
        ("rebound parameter", rebound),
    ] {
        let class = class_of(&source, call)?;
        assert_ne!(class, ExposureClass::Exposed, "{label}");
    }
    Ok(())
}

#[test]
fn a_constant_declared_only_inside_the_test_module_does_not_resolve() -> Result<(), String> {
    // The owner reads a constant the file never declares at top level; the
    // test module's own `FREE_OVER_CENTS` is a different binding.
    let source = AFTER.replace(
        "pub const FREE_OVER_CENTS: u32 = 5_000;",
        "use limits::FREE_OVER_CENTS;",
    ) + "\nmod limits {\n    pub const FREE_OVER_CENTS: u32 = 5_000;\n}\n";
    let body = "assert_eq!(shipping_cost(100, 10, true), 800);";
    let class = class_of(&source, body)?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_wrapped_or_computed_operand_is_not_a_bare_pin() -> Result<(), String> {
    for body in [
        "assert_eq!(shipping_cost(100, 10, true) / 2, 400);",
        "let weight = 100;\n        assert_eq!(shipping_cost(weight, 10, true), 800);",
    ] {
        let class = class_of(AFTER, body)?;
        assert_ne!(class, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

#[test]
fn a_wrapping_local_a_static_or_a_shadowing_closure_is_not_credited() -> Result<(), String> {
    // #14950 review: `n + 100` wraps a `u8` local to 44 with overflow checks
    // off, so the call takes the `else` branch. A fold held only to the
    // parameters' `u32` width would compute 300 and credit this pin of the
    // other branch's value.
    let narrow_local = AFTER
        .replace(
            "    let base = if weight_g <= 500 { 400 } else { 900 };",
            "    let base = if weight_g <= 500 { 400 } else { 900 }; let n: u8 = 200; let total = n + 100;",
        )
        .replace("    if express {", "    if total > 250 {");
    assert_eq!(
        class_of(
            &narrow_local,
            "assert_eq!(shipping_cost(100, 10, true), 400);"
        )
        .map(|class| class == ExposureClass::Exposed),
        Ok(false),
        "narrow local"
    );
    // #14950 review: an alias hides the narrow width from a name scan.
    let aliased_local = AFTER
        .replace(
            "    let base = if weight_g <= 500 { 400 } else { 900 };",
            "    let base = if weight_g <= 500 { 400 } else { 900 }; let n: Byte = 200; let total = n + 100;",
        )
        .replace("    if express {", "    if total > 250 {")
        + "\npub type Byte = u8;\n";
    assert_eq!(
        class_of(
            &aliased_local,
            "assert_eq!(shipping_cost(100, 10, true), 400);"
        )
        .map(|class| class == ExposureClass::Exposed),
        Ok(false),
        "aliased local"
    );
    // #14950 review: a module-level alias named like a primitive (or a `use`
    // that rebinds one) changes the width a primitive-spelled type has.
    for (label, rebinding) in [
        ("primitive-named alias", "pub type i32 = u8;"),
        ("primitive-named import", "use core::primitive::u8 as i32;"),
        ("raw-identifier alias", "pub type r#i32 = u8;"),
        (
            "macro-made alias",
            "macro_rules! alias { ($name:ident) => { pub type $name = u8; }; }\nalias!(i32);",
        ),
    ] {
        let source = AFTER
            .replace(
                "    let base = if weight_g <= 500 { 400 } else { 900 };",
                "    let base = if weight_g <= 500 { 400 } else { 900 }; let n: i32 = 200; let total = n + 100;",
            )
            .replace("    if express {", "    if total > 250 {")
            + "\n"
            + rebinding
            + "\n";
        assert_eq!(
            class_of(&source, "assert_eq!(shipping_cost(100, 10, true), 800);")
                .map(|class| class == ExposureClass::Exposed),
            Ok(false),
            "{label}"
        );
    }
    // A `static` can be reassigned before the owner reads it.
    let static_threshold = AFTER.replace(
        "pub const FREE_OVER_CENTS: u32 = 5_000;",
        "pub static FREE_OVER_CENTS: u32 = 5_000;",
    );
    assert_ne!(
        class_of(
            &static_threshold,
            "assert_eq!(shipping_cost(600, 9000, true), 1800);"
        )?,
        ExposureClass::Exposed,
        "static threshold"
    );
    // #14950 review: the asserted call reaches a closure that shadows the
    // owner, not the owner.
    let shadowed = "let _ = shipping_cost(100, 10, false);\n        let shipping_cost = |_: u32, _: u32, _: bool| 800u32;\n        assert_eq!(shipping_cost(100, 10, true), 800);";
    assert_ne!(
        class_of(AFTER, shadowed)?,
        ExposureClass::Exposed,
        "shadowed"
    );
    // Control: the same constant read as a `const` is still credited.
    assert_eq!(
        class_of(AFTER, "assert_eq!(shipping_cost(600, 9000, true), 1800);")?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn an_expected_value_the_test_computes_by_calling_code_is_not_credited() -> Result<(), String> {
    // #14950 review: the expected side calls a helper that calls the owner,
    // so a changed `base * 2` moves both sides of the assertion together.
    let helper = AFTER.to_string()
        + "\npub fn express_cost(weight_g: u32) -> u32 {\n    shipping_cost(weight_g, 10, true)\n}\n";
    for body in [
        "let expected = express_cost(100);\n        assert_eq!(shipping_cost(100, 10, true), expected);",
        "assert_eq!(shipping_cost(100, 10, true), express_cost(100));",
    ] {
        assert_ne!(class_of(&helper, body)?, ExposureClass::Exposed, "{body}");
    }
    // Control: an expected value the test fixes through a `let` literal.
    assert_eq!(
        class_of(
            &helper,
            "let expected = 800;\n        assert_eq!(shipping_cost(100, 10, true), expected);"
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_release_elided_assertion_or_a_glob_import_is_not_credited() -> Result<(), String> {
    // `debug_assert_eq!` is compiled out of a release test run.
    assert_ne!(
        class_of(
            AFTER,
            "debug_assert_eq!(shipping_cost(100, 10, true), 800);"
        )?,
        ExposureClass::Exposed,
        "debug assertion"
    );
    // A top-level glob import may bring another binding of the name.
    let glob = AFTER.replace(
        "pub const FREE_OVER_CENTS: u32 = 5_000;",
        "pub const FREE_OVER_CENTS: u32 = 5_000; use limits::*;",
    ) + "\nmod limits {\n    pub const FREE_OVER_CENTS: u32 = 1;\n}\n";
    assert_ne!(
        class_of(&glob, "assert_eq!(shipping_cost(600, 9000, true), 1800);")?,
        ExposureClass::Exposed,
        "glob import"
    );
    Ok(())
}

#[test]
fn a_remainder_by_a_minus_one_argument_is_not_folded() -> Result<(), String> {
    // #14950 review: `i8::MIN % -1` panics in Rust even with overflow checks
    // off, so this call never reaches the tail. The fold works in `i128`,
    // where the same remainder is 0, so it refuses every `-1` divisor.
    let source = AFTER
        .replace("weight_g: u32,", "weight_g: i8, divisor: i8,")
        .replace("if weight_g <= 500 {", "if weight_g % divisor == 0 {");
    let class = class_of(
        &source,
        "assert_eq!(shipping_cost(-128, -1, 10, true), 800);",
    )?;
    assert_ne!(class, ExposureClass::Exposed);
    // Control: a divisor the remainder does not overflow on is credited.
    assert_eq!(
        class_of(&source, "assert_eq!(shipping_cost(-14, 7, 10, true), 800);")?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_compound_assignment_to_the_compared_binding_is_not_a_bare_pin() -> Result<(), String> {
    // #14950 review: a later `got op= ..` changes the value the assertion
    // compares, so the binding's initializer is not what the test pins.
    for op in ["%= 1600", "|= 0", "&= 0x320", "^= 0", "<<= 0", ">>= 0"] {
        let body = format!(
            "let mut got = shipping_cost(100, 10, true);\n        got {op};\n        assert_eq!(got, 800);"
        );
        let class = class_of(AFTER, &body)?;
        assert_ne!(class, ExposureClass::Exposed, "{op}");
    }
    // Control: the same binding without a write is credited.
    assert_eq!(
        class_of(
            AFTER,
            "let got = shipping_cost(100, 10, true);\n        assert_eq!(got, 800);"
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[path = "computed_return_tail/tuple_guard.rs"]
mod tuple_guard;

#[path = "computed_return_tail/bare_name_tail.rs"]
mod bare_name_tail;
