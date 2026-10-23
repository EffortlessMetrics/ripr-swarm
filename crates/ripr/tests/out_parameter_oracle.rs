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

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -4,3 +4,3 @@ impl OffsetFormat {\n     pub fn format(&self, out: &mut String, minutes: i32) {\n-        out.push(if minutes < 0 { '-' } else { '~' });\n+        out.push(if minutes < 0 { '-' } else { '+' });\n         out.push_str(&format!(\"{:02}\", minutes.abs()));\n";

/// The changed line is always line 5; `body` is the test body.
fn source(body: &str) -> String {
    format!(
        "pub struct OffsetFormat;\n\nimpl OffsetFormat {{\n    pub fn format(&self, out: &mut String, minutes: i32) {{\n        out.push(if minutes < 0 {{ '-' }} else {{ '+' }});\n        out.push_str(&format!(\"{{:02}}\", minutes.abs()));\n    }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn formats() {{\n{body}\n    }}\n}}\n"
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
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["line"] == line)
        .cloned()
        .ok_or_else(|| format!("no finding on line {line}: {json}"))
}

/// A negative must be uncredited, not merely unconfirmed. The changed line
/// names the `out` parameter, so any credit would also confirm observation:
/// an uncredited case is therefore neither `exposed` nor a `yes`
/// discriminator.
fn assert_uncredited(finding: &Value, context: &str) {
    assert_ne!(finding["classification"], "exposed", "{context}: {finding}");
    assert_ne!(
        finding["ripr"]["discriminate"]["state"], "yes",
        "{context}: {finding}"
    );
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
fn an_assertion_before_the_call_is_not_credited() -> Result<(), String> {
    let body = "        let mut s = String::new();\n        assert_eq!(s, \"\");\n        OffsetFormat.format(&mut s, 570);";
    let finding = finding(body)?;
    assert_uncredited(&finding, "negative");
    Ok(())
}

#[test]
fn a_rebound_or_reused_local_is_not_credited() -> Result<(), String> {
    for body in [
        // rebound after the call: the assertion reads a fresh local
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        let mut s = String::from(\"+570\");\n        assert_eq!(s, \"+570\");",
        // written by another statement between definition and call
        "        let mut s = String::new();\n        s.push('x');\n        OffsetFormat.format(&mut s, 570);\n        assert_eq!(s, \"x+570\");",
        // passed twice
        "        let mut s = String::new();\n        OffsetFormat.format(&mut s, 570);\n        OffsetFormat.format(&mut s, 5);\n        assert_eq!(s, \"+570+05\");",
    ] {
        let finding = finding(body)?;
        assert_uncredited(&finding, body);
    }
    Ok(())
}

#[test]
fn a_call_in_another_test_is_not_credited() -> Result<(), String> {
    let body = format!(
        "{BUILD}    }}\n\n    #[test]\n    fn other() {{\n        let mut s = String::from(\"+570\");\n        assert_eq!(s, \"+570\");"
    );
    let finding = finding(&body)?;
    assert_uncredited(&finding, "negative");
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

/// Source whose owner body is `owner_body`; the changed line is the one that
/// pushes the sign, wherever it falls in the body.
fn finding_with_owner(owner_body: &str, test_body: &str) -> Result<Value, String> {
    finding_with_owner_changed(
        owner_body,
        "        out.push(if minutes < 0 { '-' } else { '+' });",
        test_body,
    )
}

fn finding_with_owner_changed(
    owner_body: &str,
    line5: &str,
    test_body: &str,
) -> Result<Value, String> {
    let rest = "        out.push_str(&format!(\"{:02}\", minutes.abs()));";
    let original = "        out.push(if minutes < 0 { '-' } else { '+' });";
    let src =
        source(test_body).replace(&format!("{original}\n{rest}\n"), &format!("{owner_body}\n"));
    let changed = src
        .lines()
        .position(|text| text == line5)
        .ok_or("changed line missing")?
        + 1;
    let old = line5.replace("'+'", "'~'");
    let line5 = line5.to_string();
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{changed},1 +{changed},1 @@\n-{old}\n+{line5}\n"
    );
    finding_at(src, diff, changed as u64)
}

#[test]
fn a_callee_local_shadow_of_the_out_parameter_gets_no_credit() -> Result<(), String> {
    let test = format!("{BUILD}        assert_eq!(s, \"+570\");");
    let ordinary = finding(&test)?;
    assert_eq!(ordinary["classification"], "exposed", "{ordinary}");
    assert_eq!(discriminate_state(&ordinary), "yes", "{ordinary}");
    for owner in [
        // changed line writes a shadow that is then discarded
        "        let mut out = String::new();\n        out.push(if minutes < 0 { '-' } else { '+' });\n        let _ = out;",
        // shadow introduced before the changed line
        "        let mut out = String::from(\"x\");\n        out.push(if minutes < 0 { '-' } else { '+' });",
        // parameter reassigned to another buffer
        "        out = &mut String::new();\n        out.push(if minutes < 0 { '-' } else { '+' });",
    ] {
        let src_owner = owner.to_string();
        let finding = finding_with_owner(&src_owner, &test)?;
        let state = discriminate_state(&finding);
        assert_ne!(state, "yes", "{owner}: {finding}");
        assert_uncredited(&finding, owner);
    }
    Ok(())
}

#[test]
fn an_expected_value_equal_to_the_initial_state_is_not_credited() -> Result<(), String> {
    for expected in ["String::new()", "\"\""] {
        let finding = finding(&format!("{BUILD}        assert_eq!(s, {expected});"))?;
        assert_uncredited(&finding, expected);
    }
    Ok(())
}

#[test]
fn a_read_only_out_parameter_is_not_credited() -> Result<(), String> {
    let changed = "        out.reserve(minutes as usize + '+' as usize);";
    let owner = format!("{changed}\n        let _ = out.len();");
    let test = format!("{BUILD}        assert_eq!(s, \"+570\");");
    let finding = finding_with_owner_changed(&owner, changed, &test)?;
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
    let src = format!(
        "pub struct OffsetFormat;\n\nimpl OffsetFormat {{\n{owner_fn}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn formats() {{\n{test_body}\n    }}\n}}\n"
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

#[test]
fn vec_expectations_that_may_equal_the_initial_state_are_not_credited() -> Result<(), String> {
    let owner = "    pub fn format(&self, out: &mut Vec<u8>, minutes: i32) {\n        out.push(if minutes < 0 { b'-' } else { b'+' });\n    }";
    let old = "        out.push(if minutes < 0 { b'-' } else { b'~' });";
    let new = "        out.push(if minutes < 0 { b'-' } else { b'+' });";
    let build = "        let mut s = Vec::new();\n        OffsetFormat.format(&mut s, 570);\n";
    let credited = finding_custom(
        owner,
        old,
        new,
        &format!("{build}        assert_eq!(s, vec![b'+']);"),
    )?;
    assert_eq!(credited["classification"], "exposed", "{credited}");
    assert_eq!(
        credited["ripr"]["discriminate"]["state"], "yes",
        "{credited}"
    );
    for expected in [
        "Vec::<u8>::default()",
        "Vec::with_capacity(4)",
        "[]",
        "Vec::from([])",
        "b\"\".to_vec()",
        "<Vec<u8>>::new()",
        "vec![]",
    ] {
        let finding = finding_custom(
            owner,
            old,
            new,
            &format!("{build}        assert_eq!(s, {expected});"),
        )?;
        assert_uncredited(&finding, expected);
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
        let owner = format!(
            "    pub fn format(&self, out: &mut String, minutes: i32) {{\n{changed}\n        let _ = minutes;\n    }}"
        );
        let old = changed
            .replace(");", ", 1);")
            .replace("clear()", "clear(1)");
        let finding = finding_custom(&owner, &old, changed, &test)?;
        assert_uncredited(&finding, changed);
    }
    Ok(())
}

#[test]
fn a_callee_overwrite_of_the_changed_output_is_not_confirmed() -> Result<(), String> {
    let old = "        out.push(if minutes < 0 { '-' } else { '~' });";
    let new = "        out.push(if minutes < 0 { '-' } else { '+' });";
    let test = format!("{BUILD}        assert_eq!(s, \"fixed\");");
    for tail in [
        "        out.clear();\n        out.push_str(\"fixed\");",
        "        *out = String::from(\"fixed\");",
        "        std::mem::take(out);\n        out.push_str(\"fixed\");",
    ] {
        let owner = format!(
            "    pub fn format(&self, out: &mut String, minutes: i32) {{\n{new}\n{tail}\n    }}"
        );
        let finding = finding_custom(&owner, old, new, &test)?;
        assert_uncredited(&finding, tail);
    }
    Ok(())
}

#[test]
fn a_fmt_write_owner_and_a_non_empty_initial_local_behave_as_specified() -> Result<(), String> {
    let old = "        write!(out, \"{}\", if minutes < 0 { '-' } else { '~' }).unwrap();";
    let new = "        write!(out, \"{}\", if minutes < 0 { '-' } else { '+' }).unwrap();";
    let owner = format!(
        "    pub fn format(&self, out: &mut String, minutes: i32) {{\n        use std::fmt::Write;\n{new}\n    }}"
    );
    let healthy = finding_custom(
        &owner,
        old,
        new,
        &format!("{BUILD}        assert_eq!(s, \"+\");"),
    )?;
    assert_eq!(healthy["classification"], "exposed", "{healthy}");
    assert_eq!(healthy["ripr"]["discriminate"]["state"], "yes", "{healthy}");
    // A local that starts non-empty may already equal the expected value.
    for build in [
        "        let mut s = String::from(\"+\");\n        OffsetFormat.format(&mut s, 570);\n",
        "        let mut s = String::from(\"x\");\n        OffsetFormat.format(&mut s, 570);\n",
    ] {
        let finding = finding(&format!("{build}        assert_eq!(s, \"x\");"))?;
        assert_uncredited(&finding, build);
    }
    Ok(())
}

#[test]
fn a_clear_before_the_changed_write_stays_credited() -> Result<(), String> {
    let old = "        out.push(if minutes < 0 { '-' } else { '~' });";
    let new = "        out.push(if minutes < 0 { '-' } else { '+' });";
    let owner = format!(
        "    pub fn format(&self, out: &mut String, minutes: i32) {{\n        out.clear();\n{new}\n    }}"
    );
    let finding = finding_custom(
        &owner,
        old,
        new,
        &format!("{BUILD}        assert_eq!(s, \"+\");"),
    )?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    assert_eq!(finding["ripr"]["discriminate"]["state"], "yes", "{finding}");
    Ok(())
}
