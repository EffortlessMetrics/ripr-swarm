//! Out-parameter oracle credit (RIPR-SPEC-0351, #8999). A test that passes a
//! local to the changed method as `&mut` and then asserts on that very local
//! observes the method's output. Each case writes a small crate, changes the
//! pushed sign on line 5, and reads the finding on that line through
//! `check_workspace`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-out-parameter-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("src")).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str = "[package]\nname = \"out_parameter\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -4,3 +4,3 @@ impl OffsetFormat {\n     pub fn format(&self, out: &mut String, minutes: i32) {\n-        out.push('~');\n+        out.push('+');\n         out.push_str(&format!(\"{:02}\", minutes.abs()));\n";

/// The changed line is always line 5; `body` is the test body.
fn source(body: &str) -> String {
    format!(
        "pub struct OffsetFormat;\n\nimpl OffsetFormat {{\n    pub fn format(&self, out: &mut String, minutes: i32) {{\n        out.push('+');\n        out.push_str(&format!(\"{{:02}}\", minutes.abs()));\n    }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn formats() {{\n{body}\n    }}\n}}\n"
    )
}

fn finding(body: &str) -> Result<Value, String> {
    finding_in(source(body), DIFF.to_string())
}

/// Same crate with a `&mut self` owner, so the receiver is itself a `&mut`
/// parameter that must not shift the out-parameter positions.
fn finding_mut_self(body: &str) -> Result<Value, String> {
    finding_in(
        source(body).replace("(&self, out", "(&mut self, out"),
        DIFF.replace("(&self, out", "(&mut self, out"),
    )
}

fn finding_in(source: String, diff: String) -> Result<Value, String> {
    finding_at(source, diff, 5)
}

fn finding_at(source: String, diff: String, line: u64) -> Result<Value, String> {
    let all = all_findings(source, diff)?;
    all.iter()
        .find(|finding| finding["probe"]["line"] == line)
        .cloned()
        .ok_or_else(|| format!("no finding on line {line}: {all:?}"))
}

fn all_findings(source: String, diff: String) -> Result<Vec<Value>, String> {
    let scratch = Scratch::create()?;
    std::fs::write(scratch.0.join("Cargo.toml"), MANIFEST).map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    let diff_file = scratch.0.join("change.diff");
    std::fs::write(&diff_file, diff).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    json["findings"]
        .as_array()
        .cloned()
        .ok_or_else(|| "missing findings".to_string())
}

/// A negative must be uncredited, not merely unconfirmed. The changed line
/// names the `out` parameter, so any credit would also confirm observation:
/// an uncredited case is therefore neither `exposed` nor a `yes`
/// discriminator, and it stays in the weak-or-unknown classes.
fn assert_uncredited(finding: &Value, context: &str) {
    assert_ne!(finding["classification"], "exposed", "{context}: {finding}");
    assert_ne!(
        finding["ripr"]["discriminate"]["state"], "yes",
        "{context}: {finding}"
    );
    let class = finding["classification"].as_str().unwrap_or_default();
    assert!(
        matches!(
            class,
            "weakly_exposed"
                | "reachable_unrevealed"
                | "propagation_unknown"
                | "infection_unknown"
                | "static_unknown"
        ),
        "{context}: unexpected class {class}: {finding}"
    );
}

fn assert_credited(finding: &Value) {
    assert_eq!(finding["classification"], "exposed", "{finding}");
    assert_eq!(discriminate_state(finding), "yes", "{finding}");
}

const BUILD: &str =
    "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n";

#[test]
fn asserting_the_local_passed_as_mut_credits_the_call() -> Result<(), String> {
    let finding = finding(&format!("{BUILD}        assert_eq!(s, \"+570\");"))?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    Ok(())
}

#[test]
fn a_different_asserted_local_is_not_credited() -> Result<(), String> {
    for tail in [
        // another local, not the one passed `&mut`
        "        let t = String::from(\"+570\");\n        assert_eq!(t, \"+570\");",
        // whole word: `s2` is not `s`
        "        let s2 = s.clone();\n        assert_eq!(s2, \"+570\");",
        // assertion that is not on a local
        "        assert_eq!(s.len(), 4);",
    ] {
        let finding = finding(&format!("{BUILD}{tail}"))?;
        assert_uncredited(&finding, tail);
    }
    Ok(())
}

#[test]
fn a_rebound_or_reused_local_is_not_credited() -> Result<(), String> {
    for body in [
        // rebound after the call: the assertion reads a fresh local
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        let mut s = String::from(\"+570\");\n        assert_eq!(s, \"+570\");",
        // written by another statement between definition and call
        "        let mut s = String::new();\n        s.push('x');\n        OffsetFormat.format(&mut s, 570);\n        assert_eq!(s, \"x+570\");",
        // assertion before the call
        "        let mut s = String::new();\n        assert_eq!(s, \"\");\n        OffsetFormat.format(&mut s, 570);",
        // the call sits in another test
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n    }\n\n    #[test]\n    fn other() {\n        let mut s = String::from(\"+570\");\n        assert_eq!(s, \"+570\");",
        // passed twice
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        OffsetFormat.format(&mut s, 5);\n        assert_eq!(s, \"+570+05\");",
    ] {
        let finding = finding(body)?;
        assert_uncredited(&finding, body);
    }
    Ok(())
}

#[test]
fn a_mut_self_owner_keeps_receiver_exclusive_positions() -> Result<(), String> {
    // The `&mut self` receiver is not an argument: `&mut s` is argument 0 and
    // maps to `out`, so asserting `s` is credited.
    let credited = finding_mut_self(&format!("{BUILD}        assert_eq!(s, \"+570\");"))?;
    assert_eq!(credited["classification"], "exposed", "{credited}");
    // Passing the local at the `minutes` position (not `&mut`) or a different
    // local at `out` stays uncredited.
    let other = finding_mut_self(
        "        let mut s = String::new();\n        let mut t = String::new();\n        OffsetFormat.format(&mut t, 570);\n        assert_eq!(s, \"\");",
    )?;
    assert_uncredited(&other, "other local");
    Ok(())
}

#[test]
fn loop_closure_macro_and_alias_mentions_are_not_credited() -> Result<(), String> {
    for body in [
        // call inside a loop
        "        let mut s = String::new();\n        for _ in 0..1 {\n            OffsetFormat.format(&mut s, 570);\n        }\n        assert_eq!(s, \"+570\");",
        // call inside a closure
        "        let mut s = String::new();\n        let run = || OffsetFormat.format(&mut s, 570);\n        run();\n        assert_eq!(s, \"+570\");",
        // local mentioned by a macro before the assertion
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        println!(\"{}\", s);\n        assert_eq!(s, \"+570\");",
        // alias of the local is asserted
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        let alias = &s;\n        assert_eq!(alias, \"+570\");",
        // intervening post-call write
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        s = String::from(\"+570\");\n        assert_eq!(s, \"+570\");",
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        s.clear();\n        s.push_str(\"+570\");\n        assert_eq!(s, \"+570\");",
    ] {
        let finding = finding(body)?;
        assert_uncredited(&finding, body);
    }
    Ok(())
}

/// The discriminator state the public JSON reports for the line-5 finding.
fn discriminate_state(finding: &Value) -> String {
    finding["ripr"]["discriminate"]["state"]
        .as_str()
        .or_else(|| finding["discriminate"]["state"].as_str())
        .unwrap_or("absent")
        .to_string()
}

#[test]
fn a_callee_local_shadow_of_the_out_parameter_gets_no_credit() -> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"+570\");");
    let ordinary = finding(&test)?;
    assert_credited(&ordinary);
    for owner in [
        // changed line writes a shadow that is then discarded
        "        let mut out = String::new();\n        out.push('+');\n        let _ = out;",
        // shadow introduced before the changed line
        "        let mut out = String::from(\"x\");\n        out.push('+');",
        // parameter reassigned to another buffer
        "        out = &mut String::new();\n        out.push('+');",
    ] {
        let finding = finding_custom(&owner_with(&[owner]), OLD, NEW, &test)?;
        assert_uncredited(&finding, owner);
    }
    Ok(())
}

#[test]
fn a_read_only_out_parameter_is_not_credited() -> Result<(), String> {
    let changed = "        out.reserve(minutes as usize + '+' as usize);";
    let owner = owner_with(&[changed, "        let _ = out.len();"]);
    let test = format!("{BUILD}        assert_eq!(s, \"+570\");");
    let finding = finding_custom(&owner, OLD, changed, &test)?;
    assert_uncredited(&finding, &owner);
    Ok(())
}

#[test]
fn a_same_named_method_on_another_or_unresolved_receiver_is_not_credited() -> Result<(), String> {
    let extra = "    }\n\n    struct Other;\n    impl Other {\n        fn format(&self, out: &mut String, _m: i32) {\n            out.push('x');\n        }\n    }\n\n    #[test]\n    fn other_type() {\n";
    for body in [
        // different type with a same-named method
        "        let mut s = String::new();\n        Other.format(&mut s, 570);\n        assert_eq!(s, \"x\");".to_string(),
        // unresolved receiver
        "        let mut s = String::new();\n        make().format(&mut s, 570);\n        assert_eq!(s, \"+570\");".to_string(),
    ] {
        let body = if body.contains("Other.") {
            format!("        let _ = 0;\n{extra}{body}")
        } else {
            body
        };
        let finding = finding(&body)?;
        assert_uncredited(&finding, &body);
    }
    Ok(())
}

/// A crate whose owner is `owner_fn` (full method text, starting at line 4),
/// with `new_line` replacing `old_line` in the diff.
fn finding_custom(
    owner_fn: &str,
    old_line: &str,
    new_line: &str,
    test_body: &str,
) -> Result<Value, String> {
    finding_custom_in("", owner_fn, old_line, new_line, test_body)
}

fn finding_custom_in(
    prelude: &str,
    owner_fn: &str,
    old_line: &str,
    new_line: &str,
    test_body: &str,
) -> Result<Value, String> {
    let src = format!(
        "{prelude}pub struct OffsetFormat;\n\nimpl OffsetFormat {{\n{owner_fn}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn formats() {{\n{test_body}\n    }}\n}}\n"
    );
    let changed = src
        .lines()
        .position(|text| text == new_line)
        .ok_or("changed line missing")?
        + 1;
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{changed},1 +{changed},1 @@\n-{old_line}\n+{new_line}\n"
    );
    finding_at(src, diff, changed as u64)
}

#[test]
fn a_name_mention_on_an_unrelated_changed_line_is_not_confirmed() -> Result<(), String> {
    let owner = "    pub fn format(&self, out: &mut String, other: &mut String, minutes: i32) {\n        out.push('x');\n        other.push_str(&out.len().to_string());\n    }";
    let test = "        let mut a = String::new();\n        let mut b = String::new();\n        OffsetFormat.format(&mut a, &mut b, 570);\n        assert_eq!(a, \"x\");";
    let finding = finding_custom(
        owner,
        "        other.push_str(&out.capacity().to_string());",
        "        other.push_str(&out.len().to_string());",
        test,
    )?;
    assert_uncredited(&finding, "mention of out on the other's line");
    Ok(())
}

#[test]
fn string_expectations_that_may_equal_the_initial_state_are_not_credited() -> Result<(), String> {
    for expected in [
        "String::new()",
        "\"\"",
        "String::from(\"\")",
        "String::default()",
        "\"\".to_string()",
        "\"\".to_owned()",
        "String::with_capacity(8)",
        "format!(\"\")",
        "std::string::String::new()",
    ] {
        let finding = finding(&format!("{BUILD}        assert_eq!(s, {expected});"))?;
        assert_uncredited(&finding, expected);
    }
    let local = finding(&format!(
        "{BUILD}        let e = String::new();\n        assert_eq!(s, e);"
    ))?;
    assert_uncredited(&local, "empty expected local");
    Ok(())
}

/// A `&mut Vec<u8>` owner whose changed line is `out.push(b'+')`, asserted
/// against `expected`.
fn vec_finding(lines: &[&str], expected: &str) -> Result<Value, String> {
    let owner = format!(
        "    pub fn format(&self, out: &mut Vec<u8>, minutes: i32) {{\n{}\n    }}",
        lines.join("\n")
    );
    let test = format!(
        "        let mut s = Vec::new();\n        OffsetFormat.format(&mut s, 570);\n        assert_eq!(s, {expected});"
    );
    finding_custom(
        &owner,
        "        out.push(b'~');",
        "        out.push(b'+');",
        &test,
    )
}

#[test]
fn vec_expectations_that_may_equal_the_initial_state_are_not_credited() -> Result<(), String> {
    let push = "        out.push(b'+');";
    assert_credited(&vec_finding(&[push], "vec![b'+']")?);
    for expected in [
        "Vec::<u8>::default()",
        "Vec::with_capacity(4)",
        "[]",
        "Vec::from([])",
        "b\"\".to_vec()",
        "<Vec<u8>>::new()",
        "vec![]",
    ] {
        assert_uncredited(&vec_finding(&[push], expected)?, expected);
    }
    Ok(())
}

#[test]
fn no_op_writers_are_not_credited() -> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"+570\");");
    for changed in [
        "        out.clear();",
        "        out.truncate(0);",
        "        out.truncate(100);",
        "        out.drain(..0);",
        "        out.retain(|_| true);",
        "        out.push_str(\"\");",
    ] {
        let owner = owner_with(&[changed, "        let _ = minutes;"]);
        let old = changed
            .replace(");", ", 1);")
            .replace("clear()", "clear(1)");
        assert_uncredited(&finding_custom(&owner, &old, changed, &test)?, changed);
    }
    Ok(())
}

#[test]
fn a_fmt_write_owner_and_a_non_empty_initial_local_behave_as_specified() -> Result<(), String> {
    let new = "        write!(out, \"+\").unwrap();";
    let owner = owner_with(&["        use std::fmt::Write;", new]);
    let test = format!("{BUILD}        assert_eq!(s, \"+\");");
    assert_credited(&finding_custom(&owner, &new.replace('+', "~"), new, &test)?);
    // A local that starts non-empty may already equal the expected value.
    for init in ["+", "x"] {
        let body = format!(
            "        let mut s = String::from(\"{init}\");\n        OffsetFormat.format(&mut s, 570);\n        assert_eq!(s, \"x\");"
        );
        assert_uncredited(&finding(&body)?, init);
    }
    Ok(())
}

#[test]
fn a_clear_before_the_changed_write_stays_credited() -> Result<(), String> {
    let owner = owner_with(&["        out.clear();", NEW]);
    let test = format!("{BUILD}        assert_eq!(s, \"+\");");
    assert_credited(&finding_custom(&owner, OLD, NEW, &test)?);
    Ok(())
}

const OLD: &str = "        out.push('~');";
const NEW: &str = "        out.push('+');";

fn owner_with(lines: &[&str]) -> String {
    format!(
        "    pub fn format(&self, out: &mut String, minutes: i32) {{\n{}\n    }}",
        lines.join("\n")
    )
}

#[test]
fn a_later_use_of_the_output_other_than_a_plain_write_is_not_credited() -> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"fixed\");");
    for tail in [
        // destructive callee overwrites after the changed write
        "        out.clear();\n        out.push_str(\"fixed\");",
        "        *out = String::from(\"fixed\");",
        "        std::mem::take(out);\n        out.push_str(\"fixed\");",
        // the output is passed onward
        "        wipe(out);",
        // parenthesized deref receiver
        "        (*out).clear();\n        (*out).push_str(\"fixed\");",
        "        out.clone_from(&String::from(\"fixed\"));",
        "        let _ = out.split_off(0);\n        out.push_str(\"fixed\");",
        "        let wipe = |s: &mut String| { s.clear(); s.push_str(\"fixed\"); };\n        wipe(out);",
        "        let r: &mut String = out;\n        r.clear();\n        r.push_str(\"fixed\");",
        "        out.deref_mut().clear();\n        out.push_str(\"fixed\");",
    ] {
        let owner = owner_with(&[NEW, tail]);
        let finding = finding_custom(&owner, OLD, NEW, &test)?;
        assert_uncredited(&finding, tail);
    }
    Ok(())
}

#[test]
fn a_later_use_of_a_vec_output_other_than_a_plain_write_is_not_credited() -> Result<(), String> {
    let push = "        out.push(b'+');";
    for tail in [
        "        out.clone_from(&vec![9u8]);",
        "        let _ = out.split_off(0);\n        out.push(9u8);",
        "        out.splice(.., [9u8]);",
    ] {
        assert_uncredited(&vec_finding(&[push, tail], "vec![9u8]")?, tail);
    }
    Ok(())
}

#[test]
fn a_changed_write_that_may_not_execute_is_not_credited() -> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"570\");");
    let tail = "        out.push_str(\"570\");";
    for lines in [
        format!("        if minutes < 0 {{\n{NEW}\n        }}\n{tail}"),
        format!("        for _ in 0..0 {{\n{NEW}\n        }}\n{tail}"),
        format!("{tail}\n        return;\n{NEW}"),
        format!("{tail}\n        if minutes > 0 {{\n            return;\n        }}\n{NEW}"),
    ] {
        let owner = owner_with(&[&lines]);
        let finding = finding_custom(&owner, OLD, NEW, &test)?;
        assert_uncredited(&finding, &owner);
    }
    // The changed token sits on the unexercised side of a conditional, or
    // the changed write may write the empty branch.
    for (new, expected) in [
        (
            "        out.push(if minutes < 0 { '-' } else { '+' });",
            "+570",
        ),
        (
            "        out.push_str(if minutes < 0 { \"-\" } else { \"\" });",
            "570",
        ),
    ] {
        let owner = owner_with(&[new, tail]);
        let test = format!("{BUILD}        assert_eq!(s, \"{expected}\");");
        let finding = finding_custom(&owner, &new.replace('-', "~"), new, &test)?;
        assert_uncredited(&finding, new);
    }
    Ok(())
}

#[test]
fn a_user_defined_write_macro_is_not_a_writer() -> Result<(), String> {
    let new = "        write!(out, \"+\").unwrap();";
    let old = "        write!(out, \"~\").unwrap();";
    let owner = owner_with(&[
        "        use std::fmt::Write;",
        new,
        "        out.push_str(\"fixed\");",
    ]);
    // The macro precedes the owner textually, so it is the `write!` the
    // owner resolves; it expands to a value `.unwrap()` accepts.
    let prelude =
        "macro_rules! write { ($o:expr, $($t:tt)*) => {{ $o.clear(); Ok::<(), ()>(()) }}; }\n";
    let test = format!("{BUILD}        assert_eq!(s, \"fixed\");");
    let finding = finding_custom_in(prelude, &owner, old, new, &test)?;
    assert_uncredited(&finding, "user write! macro");
    Ok(())
}

#[test]
fn an_attribute_on_the_changed_write_is_not_credited() -> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"570\");");
    let tail = "        out.push_str(&format!(\"{:02}\", minutes.abs()));";
    for attribute in [
        "        #[cfg(feature = \"nope\")]",
        "        #[cfg(not(test))]",
        "        #[cfg(windows)]",
        "        #[cfg(any())]",
    ] {
        let owner = owner_with(&[attribute, NEW, tail]);
        let finding = finding_custom(&owner, OLD, NEW, &test)?;
        assert_uncredited(&finding, attribute);
    }
    Ok(())
}

#[test]
fn divergence_hidden_in_a_macro_or_exit_before_the_changed_write_is_not_credited()
-> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"570\");");
    let first = "        out.push_str(\"570\");";
    let quit = "macro_rules! quit { () => { return }; }\n";
    let guarded = |inner: &str| {
        format!("{first}\n        if minutes == 570 {{\n            {inner}\n        }}\n{NEW}")
    };
    for (prelude, lines) in [
        (quit, format!("{first}\n        quit!();\n{NEW}")),
        (quit, guarded("quit!();")),
        ("", guarded("std::process::exit(0);")),
        ("", guarded("panic!(\"stop\");")),
    ] {
        let owner = owner_with(&[&lines]);
        let finding = finding_custom_in(prelude, &owner, OLD, NEW, &test)?;
        assert_uncredited(&finding, &owner);
    }
    Ok(())
}

#[test]
fn a_constant_conditional_sign_row_does_not_come_from_the_out_parameter_route() -> Result<(), String>
{
    let new = "        out.push(if minutes < 0 { '+' } else { '+' });";
    let old = "        out.push(if minutes < 0 { '~' } else { '+' });";
    let owner = owner_with(&[new, "        out.push_str(\"570\");"]);
    let rows = |test: &str| -> Result<Vec<(String, String, String)>, String> {
        let src = format!(
            "pub struct OffsetFormat;\n\nimpl OffsetFormat {{\n{owner}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn formats() {{\n{test}\n    }}\n}}\n"
        );
        let changed = src.lines().position(|text| text == new).ok_or("missing")? + 1;
        let diff = format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{changed},1 +{changed},1 @@\n-{old}\n+{new}\n"
        );
        let mut rows: Vec<_> = all_findings(src, diff)?
            .iter()
            .map(|row| {
                (
                    row["probe"]["expression"].to_string(),
                    row["classification"].to_string(),
                    row["ripr"]["discriminate"]["state"].to_string(),
                )
            })
            .collect();
        rows.sort();
        Ok(rows)
    };
    // The same call asserted on the out-parameter local, and on a local the
    // route can never credit: the rows must not differ.
    let credited_shape = rows(&format!("{BUILD}        assert_eq!(s, \"+570\");"))?;
    let route_disabled = rows(
        "        let mut s = String::new();\n        let mut t = String::new();\n        OffsetFormat.format(&mut s, 570);\n        t.push_str(\"+570\");\n        assert_eq!(t, \"+570\");",
    )?;
    assert_eq!(credited_shape, route_disabled);
    Ok(())
}

#[test]
fn an_earlier_same_named_call_does_not_resolve_the_target_receiver() -> Result<(), String> {
    let extra = "    }\n\n    struct Other;\n    impl Other {\n        fn format(&self, out: &mut String, _m: i32) {\n            out.push('x');\n        }\n    }\n\n    #[test]\n    fn earlier() {\n";
    for earlier in [
        // an earlier call on another type
        "        let mut x = String::new();\n        Other.format(&mut x, 1);\n",
        // an earlier call on the owner type itself
        "        let mut x = String::new();\n        OffsetFormat.format(&mut x, 1);\n",
    ] {
        let body = format!(
            "        let _ = 0;\n{extra}{earlier}        let mut s = String::new();\n        make().format(&mut s, 570);\n        assert_eq!(s, \"+570\");"
        );
        let finding = finding(&body)?;
        assert_uncredited(&finding, earlier);
    }
    Ok(())
}

#[test]
fn capacity_only_and_empty_extension_operations_are_not_writes() -> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"+570\");");
    for changed in [
        "        out.reserve_exact(8);",
        "        out.shrink_to_fit();",
        "        out.extend_from_slice(&[]);",
        "        out.extend(\"\".chars());",
    ] {
        let owner = owner_with(&[changed]);
        let finding = finding_custom(&owner, "        out.reserve_exact(1);", changed, &test)?;
        assert_uncredited(&finding, changed);
    }
    Ok(())
}

#[test]
fn macro_argument_exits_and_bare_calls_before_the_last_write_are_not_credited() -> Result<(), String>
{
    let test = format!("{BUILD}        assert_eq!(s, \"570\");");
    let first = "        out.push_str(\"570\");";
    for (prelude, middle) in [
        (
            "",
            "        assert!({ if minutes == 570 { return; } true });",
        ),
        (
            "",
            "        let _n = format!(\"{}\", { if minutes == 570 { return; } 1 });",
        ),
        ("", "        let _n = format!(\"{}\", helper(minutes)?);"),
        (
            "use std::process::exit as bye;\n",
            "        if minutes == 570 {\n            bye(0);\n        }",
        ),
        ("", "        helper(minutes);"),
    ] {
        let owner = owner_with(&[&format!("{first}\n{middle}\n{NEW}")]);
        let finding = finding_custom_in(prelude, &owner, OLD, NEW, &test)?;
        assert_uncredited(&finding, &owner);
    }
    Ok(())
}

#[test]
fn an_enclosing_item_attribute_or_a_same_named_twin_is_not_credited() -> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"+570\");");
    let owner = owner_with(&[NEW, "        out.push_str(\"570\");"]);
    // The owner fn itself is cfg-gated away.
    let gated = format!("    #[cfg(any())]\n{owner}");
    let finding = finding_custom(&gated, OLD, NEW, &test)?;
    assert_uncredited(&finding, "cfg on the owner fn");
    // A second method of the same name sits in the same impl.
    let twin = format!(
        "    #[cfg(not(any()))]\n    pub fn format(&self, out: &mut String, minutes: i32) {{\n        let _ = (out, minutes);\n    }}\n{owner}"
    );
    let finding = finding_custom(&twin, OLD, NEW, &test)?;
    assert_uncredited(&finding, "same-named twin");
    // Inert attributes keep the credit.
    let inert = format!("    #[inline]\n    #[must_use]\n{owner}");
    let finding = finding_custom(&inert, OLD, NEW, &test)?;
    assert_credited(&finding);
    Ok(())
}

#[test]
fn a_generic_or_trait_typed_output_is_not_credited() -> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"+570\");");
    for signature in [
        "    pub fn format<T: Sink>(&self, out: &mut T, minutes: i32) {",
        "    pub fn format(&self, out: &mut impl Sink, minutes: i32) {",
        "    pub fn format(&self, out: &mut dyn Sink, minutes: i32) {",
    ] {
        let owner = format!("{signature}\n{NEW}\n        out.push('x');\n    }}");
        let finding = finding_custom(&owner, OLD, NEW, &test)?;
        assert_uncredited(&finding, signature);
    }
    Ok(())
}

#[test]
fn a_user_type_named_like_a_std_buffer_is_not_credited() -> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"+570\");");
    for prelude in [
        "mod m { pub struct String; impl String { pub fn new() -> Self { String } pub fn push(&mut self, _c: char) {} } }\nuse m::String;\n",
        "struct Noop;\ntype String = Noop;\n",
        "struct String;\n",
        "use other::*;\n",
    ] {
        let owner = owner_with(&[NEW, "        out.push_str(\"570\");"]);
        let finding = finding_custom_in(prelude, &owner, OLD, NEW, &test)?;
        assert_uncredited(&finding, prelude);
    }
    // A std import of the same names keeps the credit.
    let owner = owner_with(&[NEW, "        out.push_str(\"570\");"]);
    let finding = finding_custom_in("use std::collections::VecDeque;\n", &owner, OLD, NEW, &test)?;
    assert_credited(&finding);
    Ok(())
}

#[test]
fn only_certainly_non_empty_write_arguments_are_credited() -> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"+570\");");
    for changed in [
        "        out.write_all(b\"\");",
        "        out.push_str(&format!(\"\"));",
        "        out.extend_from_slice(&[0u8; 0]);",
        "        out.append(&mut Vec::<u8>::new());",
        "        write!(out, \"\").unwrap();",
        "        write!(out, \"{}\", &\"\").unwrap();",
        "        out.extend(None::<char>);",
        "        out.extend(Vec::<char>::new());",
        "        out.push_str(&String::new());",
    ] {
        let owner = owner_with(&["        use std::fmt::Write;", changed]);
        let finding = finding_custom(&owner, "        // old", changed, &test)?;
        assert_uncredited(&finding, changed);
    }
    // Certainly non-empty arguments keep the credit.
    for changed in [
        "        write!(out, \"+\").unwrap();",
        "        out.push_str(&format!(\"{:02}\", minutes.abs()));",
        "        out.push_str(\"+\");",
    ] {
        let owner = owner_with(&["        use std::fmt::Write;", changed]);
        let finding = finding_custom(&owner, "        // old", changed, &test)?;
        assert_credited(&finding);
    }
    Ok(())
}

#[test]
fn a_helper_call_between_setup_and_assertion_is_not_credited() -> Result<(), String> {
    for body in [
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        quit_now();\n        assert_eq!(s, \"+570\");",
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        util::quit_now();\n        assert_eq!(s, \"+570\");",
        "        util::quit_now();\n        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        assert_eq!(s, \"+570\");",
    ] {
        let finding = finding(body)?;
        assert_uncredited(&finding, body);
    }
    Ok(())
}

/// The finding for an owner made of `lines` whose changed line is `changed`.
fn changed_finding(
    prelude: &str,
    lines: &[&str],
    changed: &str,
    test: &str,
) -> Result<Value, String> {
    finding_custom_in(prelude, &owner_with(lines), "        // old", changed, test)
}

#[test]
fn a_trait_resolved_writer_needs_a_std_write_import_and_no_user_trait() -> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"+570\");");
    let sink = "trait Sink { fn write_str(&mut self, s: &str); }\nimpl Sink for String { fn write_str(&mut self, _s: &str) {} }\n";
    let import = "        use std::fmt::Write;";
    let write_str = "        out.write_str(\"+\").unwrap();";
    let write_macro = "        write!(out, \"+\").unwrap();";
    for (prelude, lines, what) in [
        (sink, vec![write_str], "user trait, no import"),
        (sink, vec![import, write_str], "user trait with import"),
        ("", vec![write_macro], "write! without import"),
    ] {
        let finding = changed_finding(prelude, &lines, lines[lines.len() - 1], &test)?;
        assert_uncredited(&finding, what);
    }
    // The import alone keeps the credit.
    assert_credited(&changed_finding(
        "",
        &[import, write_str],
        write_str,
        &test,
    )?);
    Ok(())
}

#[test]
fn a_top_level_glob_import_from_another_module_is_not_credited() -> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"+570\");");
    let owner = owner_with(&[NEW, "        out.push_str(\"570\");"]);
    for prelude in ["use super::*;\n", "use crate::*;\n", "use other::*;\n"] {
        let finding = finding_custom_in(prelude, &owner, OLD, NEW, &test)?;
        assert_uncredited(&finding, prelude);
    }
    Ok(())
}

#[test]
fn a_call_inside_an_assertion_or_macro_between_call_and_assertion_is_not_credited()
-> Result<(), String> {
    for between in [
        "        assert_eq!(util::Q.go(), 1);",
        "        assert!(util::Q.go() == 1);",
        "        let _x = format!(\"{}\", util::Q.go());",
        "        let _v = vec![util::Q.go()];",
    ] {
        let body = format!("{BUILD}{between}\n        assert_eq!(s, \"+570\");");
        let finding = finding(&body)?;
        assert_uncredited(&finding, between);
    }
    // Plain setup between the call and the assertion keeps the credit.
    let body =
        format!("{BUILD}        let _m = format!(\"{{}}\", 1);\n        assert_eq!(s, \"+570\");");
    let kept = finding(&body)?;
    assert_credited(&kept);
    Ok(())
}

#[test]
fn positional_width_and_escaped_literals_do_not_prove_a_write() -> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"+570\");");
    for lines in [
        vec!["        out.push_str(&format!(\"{:1$}\", minutes, 0));"],
        vec!["        write!(out, \"{:.*}\", 0, minutes).unwrap();"],
        vec!["        out.push_str(\"\\", "            \");"],
        vec!["        out.push_str(\"\\u{0}\");"],
    ] {
        let owner = owner_with(&[&["        use std::fmt::Write;"][..], &lines[..]].concat());
        let finding = finding_custom(&owner, "        // old", lines[0], &test)?;
        assert_uncredited(&finding, lines[0]);
    }
    Ok(())
}

fn assert_not_confirmed(finding: &Value, what: &str) {
    assert_ne!(finding["classification"], "exposed", "{what}: {finding}");
    assert_ne!(discriminate_state(finding), "yes", "{what}: {finding}");
}

#[test]
fn a_write_macro_into_an_expression_starting_with_the_parameter_is_not_credited()
-> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"+570\");");
    let import = "        use std::fmt::Write;";
    for changed in [
        "        write!(out.clone(), \"+\").unwrap();",
        "        write!(out.to_string(), \"+\").unwrap();",
        "        writeln!(out.clone()).unwrap();",
        "        write!(&mut out, \"+\").unwrap();",
    ] {
        let finding = changed_finding("", &[import, changed], changed, &test)?;
        assert_not_confirmed(&finding, changed);
    }
    // The bare parameter as destination keeps the credit.
    let changed = "        write!(out, \"+\").unwrap();";
    assert_credited(&changed_finding("", &[import, changed], changed, &test)?);
    Ok(())
}

#[test]
fn turbofish_parenthesised_and_argument_calls_are_not_followed() -> Result<(), String> {
    let assertion = "        assert_eq!(s, \"+570\");";
    let call = |arg: &str| {
        format!(
            "        let mut s = String::new();\n        OffsetFormat.format(&mut s, {arg});\n{assertion}"
        )
    };
    for body in [
        format!("{BUILD}        assert!(bye::<u8>());\n{assertion}"),
        format!("{BUILD}        assert!((bye)());\n{assertion}"),
        call("util::go()"),
        call("std::process::exit(0)"),
        call("bye::<i32>()"),
        call("570").replace("570);", "570).wrap::<u8>();"),
    ] {
        assert_not_confirmed(&finding(&body)?, &body);
    }
    // The same shape in the owner, before the changed write.
    let test = format!("{BUILD}{assertion}");
    for before in ["        bye::<u8>();", "        (bye)();"] {
        let finding = changed_finding("", &[before, NEW], NEW, &test)?;
        assert_not_confirmed(&finding, before);
    }
    Ok(())
}

#[test]
fn a_workspace_macro_shadowing_a_pure_macro_in_test_setup_is_not_credited() -> Result<(), String> {
    // The owner body has no `!`, so only the test-side scan can see the macro.
    for (name, setup) in [
        ("format", "let _m = format!(\"{}\", 1);"),
        ("vec", "let _m = vec![1];"),
    ] {
        let body = format!("{BUILD}        {setup}\n        assert_eq!(s, \"+570\");");
        let owner = owner_with(&[NEW]);
        // Unshadowed, the same setup keeps the credit.
        let kept = finding_custom_in("", &owner, OLD, NEW, &body)?;
        assert_credited(&kept);
        // A workspace macro of the same name that returns early is refused.
        let prelude = format!("macro_rules! {name} {{ ($($t:tt)*) => {{ {{ return; }} }}; }}\n");
        let shadowed = finding_custom_in(&prelude, &owner, OLD, NEW, &body)?;
        assert_not_confirmed(&shadowed, name);
    }
    Ok(())
}
