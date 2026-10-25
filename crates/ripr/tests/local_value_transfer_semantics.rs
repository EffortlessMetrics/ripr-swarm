//! Integer-width and eager-default controls for RIPR-SPEC-0158 local transfer.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir();
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let path = root.join(format!(
            "ripr-value-transfer-width-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        let scratch = Self(path);
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        Ok(scratch)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn finding_for(ty: &str, initializer: &str, argument: &str, bound: &str) -> Result<Value, String> {
    finding_with_prelude("", ty, initializer, argument, bound)
}

fn finding_with_prelude(
    prelude: &str,
    ty: &str,
    initializer: &str,
    argument: &str,
    bound: &str,
) -> Result<Value, String> {
    finding_with_shape(
        prelude,
        ty,
        initializer,
        argument,
        bound,
        OperandShape::Local,
    )
}

enum OperandShape {
    Local,
    Direct,
    Reversed,
}

fn finding_with_shape(
    prelude: &str,
    ty: &str,
    expression: &str,
    argument: &str,
    bound: &str,
    shape: OperandShape,
) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    if prelude.contains("mod std;") || prelude.contains("mod r#std;") {
        std::fs::write(
            scratch.0.join("src/std.rs"),
            "#[allow(non_camel_case_types)]\npub type usize = i32;\npub mod core { #[allow(non_camel_case_types)] pub type usize = i32; }\n",
        )
        .map_err(|error| error.to_string())?;
    }
    std::fs::write(
        scratch.0.join("Cargo.toml"),
        "[package]\nname = \"integer_width\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n",
    )
    .map_err(|error| error.to_string())?;
    let (binding, before, after, lines, predicate_offset) = match shape {
        OperandShape::Local => (
            format!("    let k = {expression};\n"),
            format!("k < {bound}"),
            format!("k <= {bound}"),
            4,
            2,
        ),
        OperandShape::Direct => (
            String::new(),
            format!("{expression} < {bound}"),
            format!("{expression} <= {bound}"),
            3,
            1,
        ),
        OperandShape::Reversed => (
            String::new(),
            format!("{bound} > {expression}"),
            format!("{bound} >= {expression}"),
            3,
            1,
        ),
    };
    let source = format!(
        "{prelude}pub fn score(n: {ty}) -> bool {{\n{binding}    {after}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn anchor() {{ assert!(score({argument})); }}\n}}\n"
    );
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    let diff = scratch.0.join("change.diff");
    let start = 1 + prelude.lines().count();
    std::fs::write(
        &diff,
        format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{start},{lines} +{start},{lines} @@\n pub fn score(n: {ty}) -> bool {{\n{}-    {before}\n+    {after}\n }}\n",
            binding.replace("    let", "     let")
        ),
    )
    .map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let rendered = render_check(&report, &OutputFormat::Json)?;
    let json: Value = serde_json::from_str(&rendered).map_err(|error| error.to_string())?;
    let findings: Vec<_> = json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .filter(|finding| finding["probe"]["family"] == "predicate")
        .collect();
    assert_eq!(findings.len(), 1, "one changed predicate: {json}");
    let finding = findings[0];
    assert_eq!(finding["probe"]["family"], "predicate", "{finding}");
    assert_eq!(
        finding["probe"]["line"],
        start + predicate_offset,
        "{finding}"
    );
    assert!(
        finding["related_tests"]
            .as_array()
            .ok_or("missing related tests")?
            .iter()
            .any(|test| test["name"] == "anchor" && test["relation_reason"] == "direct_owner_call"),
        "setup: the literal call reaches the owner: {finding}"
    );
    Ok(finding.clone())
}

#[test]
fn shadowed_integer_names_cannot_credit_direct_or_reversed_arithmetic() -> Result<(), String> {
    for (prelude, ty) in [
        (
            "#[allow(non_camel_case_types)]\ntype usize = u8;\n",
            "usize",
        ),
        (
            "#[allow(non_camel_case_types)]\ntype r#usize = u8;\n",
            "usize",
        ),
        ("use std::primitive::u8 as usize;\n", "usize"),
        ("#[allow(non_camel_case_types)]\ntype u32 = u8;\n", "u32"),
        ("mod std;\nuse std::core;\nuse core::usize;\n", "usize"),
    ] {
        for shape in [OperandShape::Direct, OperandShape::Reversed] {
            let finding = finding_with_shape(prelude, ty, "(n + 100) / 2", "200", "150", shape)?;
            assert_eq!(
                finding["classification"], "infection_unknown",
                "{prelude}: {finding}"
            );
            assert_eq!(
                finding["ripr"]["infect"]["state"], "unknown",
                "{prelude}: {finding}"
            );
            assert!(
                finding["activation"]["missing_discriminators"]
                    .as_array()
                    .ok_or("missing discriminator array")?
                    .is_empty(),
                "a shadowed spelling cannot invent an equality or a missing input: {finding}"
            );
            assert!(
                finding["activation"]["observed_values"]
                    .as_array()
                    .ok_or("missing observed values")?
                    .iter()
                    .all(|value| !value["value"]
                        .as_str()
                        .is_some_and(|value| value.contains(" == "))),
                "a refused expression cannot retain an equality observation: {finding}"
            );
        }
    }
    Ok(())
}

#[test]
fn direct_intrinsics_and_literal_only_operands_keep_their_credit() -> Result<(), String> {
    for shape in [OperandShape::Direct, OperandShape::Reversed] {
        let finding = finding_with_shape(
            "use std::primitive::usize;\n",
            "usize",
            "(n + 100) / 2",
            "200",
            "150",
            shape,
        )?;
        assert_eq!(finding["ripr"]["infect"]["state"], "yes", "{finding}");
    }
    for shape in [OperandShape::Local, OperandShape::Direct] {
        let finding = finding_with_shape("mod std;\n", "u8", "1 + 2", "200", "3", shape)?;
        assert_eq!(finding["ripr"]["infect"]["state"], "yes", "{finding}");
    }
    let reversed = finding_with_shape(
        "mod std;\n",
        "u8",
        "1 + 2",
        "200",
        "3",
        OperandShape::Reversed,
    )?;
    assert_eq!(
        reversed["ripr"]["infect"]["state"], "unknown",
        "the existing literal-only reversed refusal is retained: {reversed}"
    );
    Ok(())
}

#[test]
fn non_usize_checked_arithmetic_keeps_the_local_boundary_unknown() -> Result<(), String> {
    for (ty, initializer, argument) in [
        ("u8", "n.checked_add(1).map_or(0, |i| i)", "255"),
        ("u16", "n.checked_add(1).map_or(0, |i| i)", "65535"),
        ("u32", "n.checked_add(1).map_or(0, |i| i)", "4294967295"),
        ("i8", "n.checked_sub(1).map_or(0, |i| i)", "0"),
        ("i16", "n.checked_sub(1).map_or(0, |i| i)", "0"),
        ("i32", "n.checked_sub(1).map_or(0, |i| i)", "0"),
        ("i64", "n.checked_sub(1).map_or(0, |i| i)", "0"),
        ("i128", "n.checked_sub(1).map_or(0, |i| i)", "0"),
        ("isize", "n.checked_sub(1).map_or(0, |i| i)", "0"),
    ] {
        let finding = finding_for(ty, initializer, argument, "0")?;
        assert_eq!(
            finding["classification"], "infection_unknown",
            "{ty}: {finding}"
        );
        assert_eq!(
            finding["ripr"]["infect"]["state"], "unknown",
            "{ty}: {finding}"
        );
        assert!(
            finding["activation"]["missing_discriminators"]
                .as_array()
                .ok_or("missing discriminator array")?
                .is_empty(),
            "an unmodelled integer width cannot name a missing input: {ty}: {finding}"
        );
    }
    Ok(())
}

#[test]
fn usize_checked_arithmetic_retains_exact_local_boundary_credit() -> Result<(), String> {
    for (ty, initializer, argument, bound) in [
        ("usize", "n.checked_add(1).map_or(0, |i| i)", "255", "256"),
        ("usize", "n.checked_sub(1).map_or(0, |i| i)", "0", "0"),
        (
            "&str",
            "n.len().checked_add(1).map_or(0, |i| i)",
            "\"abc\"",
            "4",
        ),
        (
            "char",
            "n.len_utf8().checked_add(1).map_or(0, |i| i)",
            "'é'",
            "3",
        ),
        ("bool", "if n { 1 + 2 } else { 3 }", "true", "3"),
    ] {
        let finding = finding_for(ty, initializer, argument, bound)?;
        assert_eq!(finding["ripr"]["infect"]["state"], "yes", "{ty}: {finding}");
    }
    Ok(())
}

#[test]
fn bounded_local_arithmetic_preserves_the_scalar_width_gate() -> Result<(), String> {
    for (ty, argument, bound, state) in [
        ("u8", "254", "255", "unknown"),
        ("u16", "254", "255", "unknown"),
        ("u32", "254", "255", "yes"),
        ("usize", "254", "255", "yes"),
    ] {
        let finding = finding_for(ty, "n + 1", argument, bound)?;
        assert_eq!(finding["ripr"]["infect"]["state"], state, "{ty}: {finding}");
    }
    Ok(())
}

#[test]
fn map_or_evaluates_a_failing_default_on_both_option_arms() -> Result<(), String> {
    for needle in ['b', 'x'] {
        let initializer = format!("\"ab\".find('{needle}').map_or(1 / n, |i| i)");
        let finding = finding_for("usize", &initializer, "0", "1")?;
        assert_eq!(finding["classification"], "infection_unknown", "{finding}");
        assert_eq!(finding["ripr"]["infect"]["state"], "unknown", "{finding}");
    }
    Ok(())
}

#[test]
fn signed_checked_sub_does_not_invent_an_equality_boundary() -> Result<(), String> {
    let finding = finding_for("i32", "n.checked_sub(1).map_or(0, |i| i)", "0", "0")?;
    assert_eq!(finding["classification"], "infection_unknown", "{finding}");
    assert_eq!(finding["ripr"]["infect"]["state"], "unknown", "{finding}");
    Ok(())
}

#[test]
fn primitive_name_aliases_cannot_supply_integer_width_evidence() -> Result<(), String> {
    for prelude in [
        "#[allow(non_camel_case_types)]\ntype usize = i32;\n",
        "#[allow(non_camel_case_types)]\ntype r#usize = i32;\n",
        "use std::primitive::i32 as usize;\n",
        "use std::primitive::i32 as r#usize;\n",
        "mod std;\nuse std::usize;\n",
        "mod r#std;\nuse r#std::usize;\n",
        "mod std;\nuse std::core;\nuse core::usize;\n",
    ] {
        let finding = finding_with_prelude(
            prelude,
            "usize",
            "n.checked_sub(1).map_or(0, |i| i)",
            "0",
            "0",
        )?;
        assert_eq!(
            finding["ripr"]["infect"]["state"], "unknown",
            "{prelude}: {finding}"
        );
    }
    let control = finding_with_prelude(
        "use std::primitive::usize;\n",
        "usize",
        "n.checked_add(1).map_or(0, |i| i)",
        "255",
        "256",
    )?;
    assert_eq!(control["ripr"]["infect"]["state"], "yes", "{control}");
    Ok(())
}

#[test]
fn map_or_retains_safe_computed_and_literal_defaults() -> Result<(), String> {
    for initializer in [
        "\"ab\".find('b').map_or(1 / n, |i| i)",
        "\"ab\".find('x').map_or(1 / n, |i| i)",
        "\"ab\".find('b').map_or(1, |i| i)",
        "\"ab\".find('x').map_or(1, |i| i)",
    ] {
        let finding = finding_for("usize", initializer, "1", "1")?;
        assert_eq!(finding["ripr"]["infect"]["state"], "yes", "{finding}");
    }
    Ok(())
}
