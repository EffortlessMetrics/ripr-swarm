//! Public-API controls for framework oracles ripr does not evaluate
//! (RIPR-SPEC-0240).
//!
//! A related test that checks the owner through an environment variable it
//! sets, a mockall closure that reads its arguments, an expect-test debug
//! literal, or an insta snapshot of a field no redaction names reads as
//! `static_unknown` under `rust_framework_oracle_unresolved`. Each control
//! below changes one thing about the shape and keeps the gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-framework-oracle-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"framework-oracle\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        Ok(output.findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A one-line edit of `line` in `src/lib.rs`.
fn diff(line: usize, before: &str, after: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{line},1 +{line},1 @@
-{before}
+{after}
"
    )
}

/// Every finding on `line`: one edit can carry several probes.
fn findings_on(lib: &str, diff: &str, line: usize) -> Result<Vec<Finding>, String> {
    let findings = TempRepo::create(lib, diff)?.findings()?;
    let on_line: Vec<Finding> = findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == line)
        .collect();
    if on_line.is_empty() {
        return Err(format!("premise: a finding on line {line}"));
    }
    Ok(on_line)
}

const LIMIT: &str = "rust_framework_oracle_unresolved";

fn is_gap(finding: &Finding) -> bool {
    matches!(
        finding.class,
        ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
    )
}

fn limited(finding: &Finding) -> bool {
    finding.static_limit_kind.map(|kind| kind.as_str()) == Some(LIMIT)
}

/// Some finding on the line carries the limit and none is still a gap.
fn withheld(lib: &str, diff: &str, line: usize, label: &str) -> Result<(), String> {
    let findings = findings_on(lib, diff, line)?;
    if findings.iter().any(is_gap) || !findings.iter().any(limited) {
        return Err(format!(
            "{label}: expected the framework limit, got {:?}",
            findings
                .iter()
                .map(|f| (
                    f.probe.family.as_str(),
                    f.class.as_str(),
                    f.static_limit_kind
                ))
                .collect::<Vec<_>>()
        ));
    }
    Ok(())
}

/// No finding carries the limit and some finding is still a gap.
fn keeps_gap(lib: &str, diff: &str, line: usize, label: &str) -> Result<(), String> {
    let findings = findings_on(lib, diff, line)?;
    if findings.iter().any(limited) || !findings.iter().any(is_gap) {
        return Err(format!(
            "{label}: expected the gap kept, got {:?}",
            findings
                .iter()
                .map(|f| (
                    f.probe.family.as_str(),
                    f.class.as_str(),
                    f.static_limit_kind
                ))
                .collect::<Vec<_>>()
        ));
    }
    Ok(())
}

fn env_source(owner_read: &str, test_body: &str) -> String {
    format!(
        "pub fn retries() -> u32 {{
    match {owner_read} {{
        Ok(text) => text.parse().unwrap_or(3),
        Err(_) => 1 + 2,
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn retries_read_the_environment() {{
        {test_body}
    }}
}}
"
    )
}

const ENV_EXACT: &str = "std::env::set_var(\"APP_RETRIES\", \"7\");
        assert_eq!(retries(), 7);
        std::env::remove_var(\"APP_RETRIES\");
        assert_eq!(retries(), 3);";

#[test]
fn an_arm_chosen_by_the_environment_is_withheld() -> Result<(), String> {
    let edit = diff(4, "        Err(_) => 3,", "        Err(_) => 1 + 2,");
    withheld(
        &env_source("std::env::var(\"APP_RETRIES\")", ENV_EXACT),
        &edit,
        4,
        "environment arm",
    )
}

#[test]
fn an_environment_shape_that_differs_keeps_the_gap() -> Result<(), String> {
    let edit = diff(4, "        Err(_) => 3,", "        Err(_) => 1 + 2,");
    // The owner reads no environment variable.
    keeps_gap(
        &env_source("\"7\".parse::<u32>().map_err(|_| ())", ENV_EXACT),
        &edit,
        4,
        "owner without env",
    )?;
    // The test never changes the environment.
    keeps_gap(
        &env_source(
            "std::env::var(\"APP_RETRIES\")",
            "assert_eq!(retries(), 3);",
        ),
        &edit,
        4,
        "test without set_var",
    )?;
    // The test changes a different variable.
    keeps_gap(
        &env_source(
            "std::env::var(\"APP_RETRIES\")",
            &ENV_EXACT.replace("APP_RETRIES", "OTHER"),
        ),
        &edit,
        4,
        "other variable",
    )?;
    // The only assertion is relational.
    keeps_gap(
        &env_source(
            "std::env::var(\"APP_RETRIES\")",
            "std::env::remove_var(\"APP_RETRIES\");\n        assert!(retries() > 0);",
        ),
        &edit,
        4,
        "relational oracle",
    )?;
    // The write follows the only call, so it activates nothing.
    keeps_gap(
        &env_source(
            "std::env::var(\"APP_RETRIES\")",
            "assert_eq!(retries(), 3);\n        std::env::set_var(\"APP_RETRIES\", \"7\");",
        ),
        &edit,
        4,
        "write after the call",
    )
}

/// The `authored-lib-mockall` verdict-corpus subject, with the recipient
/// test's `returning` closure left to the caller. Line 15 sends the mail.
const MOCK_SUBJECT: &str = "//! Overdue reminders sent through a mocked mailer. The tests use mockall\n\
//! expectations: some pin the call count and arguments, one accepts any\n\
//! arguments, and one only checks the mock's return value flows back.\n\
\n\
#[cfg_attr(test, mockall::automock)]\n\
pub trait Mailer {\n\
    fn send(&self, to: &str, body: &str) -> bool;\n\
}\n\
\n\
pub const GRACE_DAYS: u32 = 30;\n\
\n\
pub fn remind(mailer: &dyn Mailer, user: &str, overdue_days: u32) -> bool {\n\
    if overdue_days > GRACE_DAYS {\n\
        let body = format!(\"{user}, your book is {overdue_days} days late\");\n\
        return mailer.send(user, &body);\n\
    }\n\
    false\n\
}\n\
\n\
pub fn remind_all(mailer: &dyn Mailer, users: &[(&str, u32)]) -> usize {\n\
    users\n\
        .iter()\n\
        .filter(|(user, days)| remind(mailer, user, *days))\n\
        .count()\n\
}\n\
\n\
#[cfg(test)]\n\
mod tests {\n\
    use super::*;\n\
    use mockall::predicate::{always, eq};\n\
\n\
    #[test]\n\
    fn reminds_only_after_the_grace_period() {\n\
        let mut mailer = MockMailer::new();\n\
        mailer.expect_send().with(eq(\"ann\"), always()).times(1).return_const(true);\n\
        assert!(remind(&mailer, \"ann\", 31));\n\
        assert!(!remind(&mailer, \"ann\", 30));\n\
    }\n\
\n\
    #[test]\n\
    fn sends_some_body() {\n\
        let mut mailer = MockMailer::new();\n\
        mailer.expect_send().times(1).returning(|_, _| true);\n\
        remind(&mailer, \"bob\", 45);\n\
    }\n\
\n\
    #[test]\n\
    fn counts_the_delivered_reminders() {\n\
        let mut mailer = MockMailer::new();\n\
        mailer.expect_send().returning(CLOSURE);\n\
        assert_eq!(remind_all(&mailer, &[(\"ann\", 40), (\"cy\", 40), (\"dee\", 2)]), 1);\n\
    }\n\
}\n\
";

fn mock_source(closure: &str) -> String {
    MOCK_SUBJECT.replace("CLOSURE", closure)
}

#[test]
fn a_mock_closure_that_reads_the_arguments_is_withheld() -> Result<(), String> {
    let edit = diff(
        15,
        "        return mailer.send(user, &body);",
        "        return mailer.send(&user[..], &body);",
    );
    withheld(
        &mock_source("|to, _| to != \"cy\""),
        &edit,
        15,
        "mock closure",
    )
}

#[test]
fn a_mock_shape_that_differs_keeps_the_gap() -> Result<(), String> {
    let edit = diff(
        15,
        "        return mailer.send(user, &body);",
        "        return mailer.send(&user[..], &body);",
    );
    // The closure ignores the call's arguments.
    keeps_gap(&mock_source("|_, _| true"), &edit, 15, "ignored arguments")?;
    keeps_gap(&mock_source("|to, _| true"), &edit, 15, "unused argument")?;
    // The changed line is the grace predicate, which no closure decides.
    let guard = diff(
        13,
        "    if overdue_days >= GRACE_DAYS {",
        "    if overdue_days > GRACE_DAYS {",
    );
    keeps_gap(
        &mock_source("|to, _| to != \"cy\""),
        &guard,
        13,
        "predicate line",
    )
}

fn expect_source(assertion: &str) -> String {
    format!(
        "pub fn tokenize(src: &str) -> Vec<u32> {{
    src.split_whitespace()
        .map(|w| w.parse::<u32>().map_or(0, |n| 10 * n))
        .collect()
}}

#[cfg(test)]
mod tests {{
    use super::*;
    use expect_test::expect;

    #[test]
    fn tokens_debug_inline() {{
        let tokens = tokenize(\"7\");
        {assertion}
    }}
}}
"
    )
}

#[test]
fn an_expect_debug_literal_is_withheld() -> Result<(), String> {
    let edit = diff(
        3,
        "        .map(|w| w.parse::<u32>().map_or(0, |n| n * 10))",
        "        .map(|w| w.parse::<u32>().map_or(0, |n| 10 * n))",
    );
    withheld(
        &expect_source(
            "expect![[r#\"\n            [\n                70,\n            ]\n        \"#]]\n        .assert_debug_eq(&tokens);",
        ),
        &edit,
        3,
        "expect debug",
    )
}

#[test]
fn an_expect_shape_that_differs_keeps_the_gap() -> Result<(), String> {
    let edit = diff(
        3,
        "        .map(|w| w.parse::<u32>().map_or(0, |n| n * 10))",
        "        .map(|w| w.parse::<u32>().map_or(0, |n| 10 * n))",
    );
    // An empty expectation is another rule's case.
    keeps_gap(
        &expect_source("expect![[\"\"]].assert_debug_eq(&tokens);"),
        &edit,
        3,
        "empty expectation",
    )?;
    // A smoke assertion is not a snapshot.
    keeps_gap(
        &expect_source("assert!(!tokens.is_empty());"),
        &edit,
        3,
        "no expectation",
    )?;
    // The literal checks a value the owner did not produce.
    keeps_gap(
        &expect_source(
            "let other = vec![1u32];\n        expect![[\"[1]\"]].assert_debug_eq(&other);",
        ),
        &edit,
        3,
        "operand not from the owner",
    )
}

fn insta_source(assertion: &str) -> String {
    format!(
        "pub struct Record {{
    pub id: String,
    pub items: usize,
}}

pub fn make_record(prefix: &str, items: &[u32]) -> Record {{
    Record {{
        id: format!(\"{{}}-1\", prefix),
        items: items.iter().count(),
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn record_snapshot() {{
        let record = make_record(\"ord\", &[1, 2]);
        {assertion}
    }}
}}
"
    )
}

#[test]
fn an_unredacted_insta_field_is_withheld() -> Result<(), String> {
    let edit = diff(
        9,
        "        items: items.len(),",
        "        items: items.iter().count(),",
    );
    for assertion in [
        "insta::assert_json_snapshot!(record, { \".id\" => \"[id]\" });",
        "insta::assert_debug_snapshot!(record);",
    ] {
        withheld(&insta_source(assertion), &edit, 9, assertion)?;
    }
    Ok(())
}

#[test]
fn an_insta_shape_that_differs_keeps_the_gap() -> Result<(), String> {
    // The redaction names the changed field.
    let items = diff(
        9,
        "        items: items.len(),",
        "        items: items.iter().count(),",
    );
    keeps_gap(
        &insta_source("insta::assert_json_snapshot!(record, { \".items\" => \"[n]\" });"),
        &items,
        9,
        "redacted field",
    )?;
    // The redaction names the changed id.
    let id = diff(
        8,
        "        id: format!(\"{}-0\", prefix),",
        "        id: format!(\"{}-1\", prefix),",
    );
    keeps_gap(
        &insta_source("insta::assert_json_snapshot!(record, { \".id\" => \"[id]\" });"),
        &id,
        8,
        "redacted id",
    )?;
    // An inline snapshot is the other insta rules' case.
    keeps_gap(
        &insta_source("insta::assert_debug_snapshot!(record, @\"Record\");"),
        &items,
        9,
        "inline snapshot",
    )?;
    // A snapshot of a value the owner did not produce.
    keeps_gap(
        &insta_source("let other = 5;\n        insta::assert_debug_snapshot!(other);"),
        &items,
        9,
        "operand not from the owner",
    )
}

#[test]
fn a_later_qualifying_snapshot_counts_after_one_that_does_not() -> Result<(), String> {
    let edit = diff(
        9,
        "        items: items.len(),",
        "        items: items.iter().count(),",
    );
    withheld(
        &insta_source(
            "insta::assert_debug_snapshot!(record, @\"Record\");\n        insta::assert_debug_snapshot!(record);",
        ),
        &edit,
        9,
        "inline then external",
    )
}

// RIPR-SPEC-0240 / #8925 / #8926: the framework reader admits only code that
// runs. The binding that reaches the assertion must be the owner's result, and
// a comment or quoted text is never a write, a binding or an assertion.

const EXPECT_BINDING: &str = "let tokens = tokenize(\"7\");";
const INSTA_BINDING: &str = "let record = make_record(\"ord\", &[1, 2]);";
const EXPECT_ASSERT: &str = "expect![[\"[1]\"]].assert_debug_eq(&tokens);";
const INSTA_ASSERT: &str = "insta::assert_debug_snapshot!(record);";

fn expect_edit() -> String {
    diff(
        3,
        "        .map(|w| w.parse::<u32>().map_or(0, |n| n * 10))",
        "        .map(|w| w.parse::<u32>().map_or(0, |n| 10 * n))",
    )
}

fn insta_edit() -> String {
    diff(
        9,
        "        items: items.len(),",
        "        items: items.iter().count(),",
    )
}

/// The test body `setup`/`assertion` for each framework, spelled with the
/// expect-test names and mapped to the insta ones.
fn both_frameworks(setup: &str, assertion: &str) -> [(String, String, usize, String); 2] {
    let insta_setup = setup
        .replace("tokens", "record")
        .replace("tokenize(\"7\")", "make_record(\"ord\", &[1, 2])")
        .replace("vec![1u32]", "5");
    let insta_assertion = assertion
        .replace("expect![[\"[1]\"]].assert_debug_eq(&tokens);", INSTA_ASSERT)
        .replace("tokens", "record");
    [
        (
            expect_source(assertion).replacen(EXPECT_BINDING, setup, 1),
            expect_edit(),
            3,
            "expect-test".to_string(),
        ),
        (
            insta_source(&insta_assertion).replacen(INSTA_BINDING, &insta_setup, 1),
            insta_edit(),
            9,
            "insta".to_string(),
        ),
    ]
}

fn all_keep_gap(setup: &str, assertion: &str) -> Result<(), String> {
    for (lib, edit, line, framework) in both_frameworks(setup, assertion) {
        keeps_gap(
            &lib,
            &edit,
            line,
            &format!("{framework}: {setup} / {assertion}"),
        )?;
    }
    Ok(())
}

fn all_withheld(setup: &str, assertion: &str) -> Result<(), String> {
    for (lib, edit, line, framework) in both_frameworks(setup, assertion) {
        withheld(
            &lib,
            &edit,
            line,
            &format!("{framework}: {setup} / {assertion}"),
        )?;
    }
    Ok(())
}

#[test]
fn an_owner_binding_must_reach_the_assertion() -> Result<(), String> {
    let assertion = EXPECT_ASSERT;
    // The owner binding is replaced, reassigned or closed before the assertion.
    for setup in [
        "let tokens = tokenize(\"7\"); let tokens = vec![1u32];",
        "let tokens = tokenize(\"7\");\n        let tokens = vec![1u32];",
        "let mut tokens = tokenize(\"7\"); tokens = vec![1u32];",
        "let tokens = vec![1u32]; { let tokens = tokenize(\"7\"); }",
        "let tokens = tokenize(\"7\"); { let tokens = vec![1u32];",
    ] {
        all_keep_gap(setup, assertion)?;
    }
    // The owner binding is made only after the assertion.
    all_keep_gap(
        "let tokens = vec![1u32];",
        &format!("{EXPECT_ASSERT} let tokens = tokenize(\"7\");"),
    )
}

#[test]
fn a_binding_that_reaches_the_assertion_is_withheld() -> Result<(), String> {
    // Controls for the shadowing pair: same text with the live binding last,
    // or the replacement confined to a closed block.
    for setup in [
        EXPECT_BINDING,
        "let mut tokens = tokenize(\"7\");",
        "let tokens = vec![1u32]; let tokens = tokenize(\"7\");",
        "let tokens = tokenize(\"7\"); { let tokens = vec![1u32]; }",
        "let tokens = tokenize(\"7\"); // let tokens = vec![1u32];",
    ] {
        all_withheld(setup, EXPECT_ASSERT)?;
    }
    Ok(())
}

#[test]
fn non_code_cannot_supply_a_binding() -> Result<(), String> {
    for setup in [
        "tokenize(\"7\");\n        // let tokens = tokenize(\"7\");\n        let tokens = vec![1u32];",
        "tokenize(\"7\");\n        /* outer /* nested */ let tokens = tokenize(\"7\"); */\n        let tokens = vec![1u32];",
        "tokenize(\"7\");\n        let _note = r#\"let tokens = tokenize(\"7\");\"#;\n        let tokens = vec![1u32];",
    ] {
        all_keep_gap(setup, EXPECT_ASSERT)?;
    }
    Ok(())
}

#[test]
fn non_code_cannot_supply_an_assertion() -> Result<(), String> {
    for assertion in [
        "// expect![[\"[70]\"]].assert_debug_eq(&tokens);\n        assert!(!tokens.is_empty());",
        "let _note = r#\"expect![[\"[70]\"]].assert_debug_eq(&tokens);\"#;\n        assert!(!tokens.is_empty());",
    ] {
        for (lib, edit, line, framework) in [
            (expect_source(assertion), expect_edit(), 3, "expect-test"),
            (
                insta_source(
                    &assertion
                        .replace(
                            "expect![[\"[70]\"]].assert_debug_eq(&tokens);",
                            INSTA_ASSERT,
                        )
                        .replace("tokens", "record"),
                ),
                insta_edit(),
                9,
                "insta",
            ),
        ] {
            keeps_gap(&lib, &edit, line, &format!("{framework}: {assertion}"))?;
        }
    }
    Ok(())
}

#[test]
fn a_commented_environment_write_activates_nothing() -> Result<(), String> {
    let edit = diff(4, "        Err(_) => 3,", "        Err(_) => 1 + 2,");
    let owner = "std::env::var(\"APP_RETRIES\")";
    for body in [
        "// unsafe { std::env::remove_var(\"APP_RETRIES\"); }\n        assert_eq!(retries(), 3);",
        "/* std::env::remove_var(\"APP_RETRIES\"); */\n        assert_eq!(retries(), 3);",
        "let _note = r#\"std::env::remove_var(\"APP_RETRIES\");\"#;\n        assert_eq!(retries(), 3);",
    ] {
        keeps_gap(&env_source(owner, body), &edit, 4, body)?;
    }
    // Control: the same test with the write live is withheld.
    withheld(
        &env_source(
            owner,
            "unsafe { std::env::remove_var(\"APP_RETRIES\"); }\n        assert_eq!(retries(), 3);",
        ),
        &edit,
        4,
        "live write",
    )?;
    // A commented read in the owner is not a read either.
    keeps_gap(
        &env_source(
            "\"7\".parse::<u32>().map_err(|_| ()) /* std::env::var(\"APP_RETRIES\") */",
            ENV_EXACT,
        ),
        &edit,
        4,
        "commented owner read",
    )
}

#[test]
fn a_shadow_inside_a_callback_reaches_an_assertion_inside_it() -> Result<(), String> {
    // Each case is the setup, then what closes the open block after the
    // assertion (nothing when the shadow is not in a block).
    let cases: [(&str, &str); 25] = [
        // The assertion sits in the callback block that rebinds the local.
        (
            "let tokens = tokenize(\"7\");\n        apply(|| {\n            let tokens = vec![1u32];",
            "\n        });",
        ),
        (
            "let tokens = tokenize(\"7\");\n        let f = || {\n            let tokens = vec![1u32];",
            "\n        };",
        ),
        (
            "let tokens = tokenize(\"7\");\n        std::thread::spawn(move || {\n            let tokens = vec![1u32];",
            "\n        });",
        ),
        (
            "let tokens = tokenize(\"7\");\n        apply(|| {\n            if ready() {\n                let tokens = vec![1u32];",
            "\n            }\n        });",
        ),
        // A closure parameter or `if let` binder of the same name.
        (
            "let tokens = tokenize(\"7\");\n        run(|tokens| {",
            "\n        });",
        ),
        (
            "let tokens = tokenize(\"7\");\n        run(a, |tokens| {\n            foo();",
            "\n        });",
        ),
        (
            "let tokens = tokenize(\"7\");\n        let g = |tokens| {",
            "\n        };",
        ),
        (
            "let tokens = tokenize(\"7\");\n        items.iter().for_each(|(i, tokens)| {",
            "\n        });",
        ),
        (
            "let tokens = tokenize(\"7\"); run(|tokens| {",
            "\n        });",
        ),
        (
            "let tokens = tokenize(\"7\");\n        foo(if let Some(tokens) = p {",
            "\n        } else {\n        });",
        ),
        (
            "let tokens = tokenize(\"7\");\n        let r = if let Some(tokens) = p {",
            "\n        } else {\n        };",
        ),
        // A bitwise or earlier in the header does not split the real `|x|`.
        (
            "let tokens = tokenize(\"7\");\n        run(a | b, |tokens| {",
            "\n        });",
        ),
        (
            "let tokens = tokenize(\"7\");\n        run(FLAG | M, move |tokens| {",
            "\n        });",
        ),
        (
            "let tokens = tokenize(\"7\");\n        run(f(a | b), |tokens| {",
            "\n        });",
        ),
        // Struct patterns bind inside braces.
        (
            "let tokens = tokenize(\"7\");\n        if let Foo { tokens, .. } = foo() {",
            "\n        }",
        ),
        (
            "let tokens = tokenize(\"7\");\n        match p {\n            Foo { tokens } => {",
            "\n            }\n        }",
        ),
        (
            "let tokens = tokenize(\"7\");\n        run(|Foo { tokens }| {",
            "\n        });",
        ),
        // The enclosing `let` binds the same name as the header binder.
        (
            "let tokens = tokenize(\"7\");\n        let tokens = |tokens: usize| {",
            "\n        };",
        ),
        (
            "let tokens = tokenize(\"7\");\n        let tokens = if let Some(tokens) = Some(2usize) {",
            "\n        } else {\n        };",
        ),
        // `||` earlier in the header does not hide the parameter list.
        (
            "let tokens = tokenize(\"7\");\n        run(|| 1, |tokens| {",
            "\n        });",
        ),
        (
            "let tokens = tokenize(\"7\");\n        run(a || b, |tokens| {",
            "\n        });",
        ),
        (
            "let tokens = tokenize(\"7\");\n        let r = foo(a || b, |tokens| {",
            "\n        });",
        ),
        // A closure body that rewrites a `mut` owner binding.
        (
            "let mut tokens = tokenize(\"7\");\n        run(|| {\n            tokens = vec![1u32];\n        });",
            "",
        ),
        // Attribute-prefixed shadow, and a borrow that can rewrite the value.
        (
            "let tokens = tokenize(\"7\");\n        #[inline]\n        let tokens = vec![1u32];",
            "",
        ),
        (
            "let mut tokens = tokenize(\"7\");\n        std::mem::replace(&mut tokens, vec![1u32]);",
            "",
        ),
    ];
    for (setup, close) in cases {
        all_keep_gap(setup, &format!("{EXPECT_ASSERT}{close}"))?;
    }
    Ok(())
}

#[test]
fn a_closed_callback_shadow_leaves_the_owner_binding() -> Result<(), String> {
    // Controls: the same callback text, closed before the assertion.
    for setup in [
        "let tokens = tokenize(\"7\");\n        apply(|| {\n            let tokens = vec![1u32];\n        });",
        "let tokens = tokenize(\"7\");\n        let f = || {\n            let tokens = vec![1u32];\n        };",
        "let tokens = tokenize(\"7\");\n        apply(|| {\n            let other = vec![1u32];\n        });",
        "let tokens = tokenize(\"7\");\n        run(|other| {\n            foo();\n        });",
        "let tokens = tokenize(\"7\");\n        run(|| 1, |other| {\n            foo();\n        });",
        "let tokens = tokenize(\"7\");\n        let other = |count: usize| {\n            foo();\n        };",
        "let tokens = tokenize(\"7\");\n        run(a | b, |other| {\n            foo();\n        });",
        "let tokens = tokenize(\"7\");\n        if let Foo { other } = foo() {\n            bar();\n        }",
        "let tokens = tokenize(\"7\");\n        run(|| {\n            foo(tokens);\n        });",
        "let tokens = tokenize(\"7\");\n        foo(if let Some(other) = p {\n            bar();\n        } else {\n            baz();\n        });",
    ] {
        all_withheld(setup, EXPECT_ASSERT)?;
    }
    Ok(())
}
