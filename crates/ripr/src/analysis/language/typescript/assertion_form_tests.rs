//! RIPR-SPEC-0243 rules 10, 16 and 17 end to end: assertion forms of chai
//! `throw`, `node:assert` `match` / `rejects` and `node:test` mock call
//! counts, each with a discriminating negative that must stay weak.

use super::*;

/// Runs the adapter over a unified diff of `src_path` in a package holding
/// `files` and returns the finding on `line`.
fn finding_for(
    label: &str,
    runner: &str,
    files: &[(&str, &str)],
    diff: &str,
    line: usize,
) -> Result<Finding, String> {
    let root = super::tests::ts_unique_tempdir(label)?;
    let write =
        |path: &str, contents: &str| super::tests::ts_write_file(&root.join(path), contents);
    write("package.json", runner)?;
    for (path, contents) in files {
        write(path, contents)?;
    }
    let changed_files = crate::analysis::diff::parse_unified_diff(diff);
    let options = AnalysisOptions {
        root: root.clone(),
        base: None,
        diff_file: None,
        mode: crate::analysis::AnalysisMode::Draft,
        include_unchanged_tests: false,
        resolve_tsconfig_paths: false,
        perl_facts_path: None,
        perl_producer_failure: None,
        git_timeout: None,
        git_candidate: None,
        production_like_targets: Default::default(),
        test_harnesses: Vec::new(),
        resolved_subject_identity: None,
        open_rust_index_paths: Default::default(),
    };
    let result = TypeScriptAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed_files);
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("no finding on line {line}"))
}

const MOCHA: &str = r#"{"name":"pkg","scripts":{"test":"mocha"},"devDependencies":{"mocha":"^12.0.0","chai":"^6.0.0"}}"#;
const NODE_TEST: &str = r#"{"name":"pkg","type":"module","scripts":{"test":"node --import tsx --test test/*.test.ts"}}"#;

fn replace_diff(path: &str, line: usize, old: &str, new: &str) -> String {
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -{line},1 +{line},1 @@\n-{old}\n+{new}\n"
    )
}

// ---- chai `throw(Class[, message])` (rule 10) ----

const RANGE_SRC_AFTER: &str = "export function describeRange(lo: number, hi: number): string {\n  if (lo > hi) {\n    THROW\n  }\n  return `${lo}..${hi}`;\n}\n";

fn chai_test(check: &str) -> String {
    format!(
        "import {{ expect }} from \"chai\";\nimport {{ describeRange }} from \"../src/range\";\n\ndescribe(\"describeRange\", () => {{\n  it(\"refuses an inverted range\", () => {{\n    expect(() => describeRange(4, 1)).to.throw({check});\n  }});\n}});\n"
    )
}

fn chai_class(
    check: &str,
    old_throw: &str,
    new_throw: &str,
    label: &str,
) -> Result<Finding, String> {
    finding_for(
        label,
        MOCHA,
        &[
            ("src/range.ts", &RANGE_SRC_AFTER.replace("THROW", new_throw)),
            ("test/range.test.ts", &chai_test(check)),
        ],
        &replace_diff(
            "src/range.ts",
            3,
            &format!("    {old_throw}"),
            &format!("    {new_throw}"),
        ),
        3,
    )
}

const RANGE: &str = r#"throw new RangeError("lo above hi");"#;
const TYPE: &str = r#"throw new TypeError("lo above hi");"#;
const REWORDED: &str = r#"throw new RangeError("lo is above hi");"#;

#[test]
fn chai_class_and_message_credit_a_class_change() -> Result<(), String> {
    let finding = chai_class(
        r#"RangeError, "lo above hi""#,
        RANGE,
        TYPE,
        "chai-class-change",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn chai_class_and_message_credit_a_message_reword() -> Result<(), String> {
    let finding = chai_class(
        r#"RangeError, "lo above hi""#,
        RANGE,
        REWORDED,
        "chai-message-reword",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn chai_class_only_credits_a_class_change() -> Result<(), String> {
    let finding = chai_class("RangeError", RANGE, TYPE, "chai-class-only")?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// Negative: a message that is a substring of both messages passes on both
/// versions, whatever the class says.
#[test]
fn chai_class_and_shared_substring_stay_weak_on_a_reword() -> Result<(), String> {
    let finding = chai_class(
        r#"RangeError, "above""#,
        RANGE,
        REWORDED,
        "chai-shared-substring",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

/// Negative: `Error` is every thrown class's ancestor, so a class change
/// that keeps the message passes `to.throw(Error, "...")` on both versions.
#[test]
fn chai_error_class_and_message_stay_weak_on_a_class_change() -> Result<(), String> {
    let finding = chai_class(
        r#"Error, "lo above hi""#,
        RANGE,
        TYPE,
        "chai-error-ancestor",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

/// Negative: the class is the same on both sides of a message reword, so
/// `to.throw(RangeError)` alone passes on both.
#[test]
fn chai_class_only_stays_weak_on_a_reword() -> Result<(), String> {
    let finding = chai_class("RangeError", RANGE, REWORDED, "chai-class-reword")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

// ---- `assert.match` with an anchored plain regex (rule 10) ----

const SLUG_AFTER: &str = "export function slugify(title: string): string {\n  return title.trim().toLowerCase().replace(/[^a-z0-9]+/g, \"_\");\n}\n";

fn match_slug(check: &str, label: &str) -> Result<Finding, String> {
    let test = format!(
        "import {{ describe, it }} from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport {{ slugify }} from \"../src/slugify\";\n\ndescribe(\"slugify\", () => {{\n  it(\"joins words\", () => {{\n    assert.match(slugify(\"Hello World\"), {check});\n  }});\n}});\n"
    );
    finding_for(
        label,
        NODE_TEST,
        &[
            ("src/slugify.ts", SLUG_AFTER),
            ("test/slugify.test.ts", &test),
        ],
        &replace_diff(
            "src/slugify.ts",
            2,
            "  return title.trim().toLowerCase().replace(/[^a-z0-9]+/g, \"-\");",
            "  return title.trim().toLowerCase().replace(/[^a-z0-9]+/g, \"_\");",
        ),
        2,
    )
}

#[test]
fn anchored_plain_regex_match_pins_the_value() -> Result<(), String> {
    let finding = match_slug("/^hello-world$/", "match-anchored")?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn unanchored_wildcard_or_flagged_regex_match_stays_weak() -> Result<(), String> {
    for check in [
        "/hello/",
        "/^hello.world$/",
        "/^HELLO-WORLD$/i",
        "/^hello[-_]world$/",
    ] {
        let finding = match_slug(check, "match-wide")?;
        assert_eq!(
            finding.class,
            ExposureClass::WeaklyExposed,
            "{check}: {finding:?}"
        );
    }
    Ok(())
}

// ---- `assert.rejects` with a plain substring regex (rule 17) ----

const RETRY_AFTER: &str = "export async function withRetry(fn: () => Promise<string>, attempts: number): Promise<string> {\n  let lastError: unknown;\n  for (let i = 0; i < attempts; i++) {\n    try {\n      return await fn();\n    } catch (error) {\n      lastError = error;\n    }\n  }\n  THROW\n}\n";

fn rejects_retry(
    check: &str,
    old: &str,
    new: &str,
    extra: &str,
    label: &str,
) -> Result<Finding, String> {
    let src = RETRY_AFTER.replace("THROW", new);
    let src = if extra.is_empty() {
        src
    } else {
        src.replace("  let lastError", &format!("{extra}\n  let lastError"))
    };
    let test = format!(
        "import {{ describe, it, mock }} from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport {{ withRetry }} from \"../src/retry\";\n\ndescribe(\"withRetry\", () => {{\n  it(\"gives up\", async () => {{\n    const fn = mock.fn(async () => {{ throw new Error(\"down\"); }});\n    await assert.rejects(withRetry(fn, 2), {check});\n  }});\n}});\n"
    );
    let line = src
        .lines()
        .position(|text| text.trim() == new.trim())
        .map_or(0, |index| index + 1);
    finding_for(
        label,
        NODE_TEST,
        &[("src/retry.ts", &src), ("test/retry.test.ts", &test)],
        &replace_diff(
            "src/retry.ts",
            line,
            &format!("  {old}"),
            &format!("  {new}"),
        ),
        line,
    )
}

#[test]
fn rejects_substring_regex_credits_a_forwarded_throw_replaced_by_another_error()
-> Result<(), String> {
    let finding = rejects_retry(
        "/down/",
        "throw lastError;",
        r#"throw new Error("other");"#,
        "",
        "rejects-forwarded",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// Negative: a regex the new error still satisfies passes on the edit.
#[test]
fn rejects_substring_regex_the_new_error_satisfies_stays_weak() -> Result<(), String> {
    let finding = rejects_retry(
        "/other/",
        "throw lastError;",
        r#"throw new Error("other");"#,
        "",
        "rejects-satisfied",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

/// Negative: with a second throw site the pinned error may come from there.
#[test]
fn rejects_substring_regex_stays_weak_with_a_second_throw_site() -> Result<(), String> {
    let finding = rejects_retry(
        "/down/",
        "throw lastError;",
        r#"throw new Error("other");"#,
        "  if (attempts < 0) throw new Error(\"down\");",
        "rejects-second-site",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

/// Negative: a regex with a metacharacter is not a plain substring test.
#[test]
fn rejects_wildcard_regex_stays_weak() -> Result<(), String> {
    let finding = rejects_retry(
        "/d.wn/",
        "throw lastError;",
        r#"throw new Error("other");"#,
        "",
        "rejects-wildcard",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

/// Both messages readable: the literal must sit in exactly one of them.
#[test]
fn rejects_substring_regex_in_both_messages_stays_weak() -> Result<(), String> {
    let finding = rejects_retry(
        "/down/",
        r#"throw new Error("down hard");"#,
        r#"throw new Error("down soft");"#,
        "",
        "rejects-both",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    let finding = rejects_retry(
        "/hard/",
        r#"throw new Error("down hard");"#,
        r#"throw new Error("down soft");"#,
        "",
        "rejects-one",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

// ---- a method call replaced by `void <local>;` (rule 16) ----

const AUDIT_AFTER: &str = "export const sink = {\n  write(line: string): void {\n    void line;\n  },\n};\n\nexport function record(event: string): string {\n  const line = `event:${event}`;\n  void line;\n  return line;\n}\n";

fn audit_test(spy: &str, expected: &str) -> String {
    format!(
        "import {{ mock, test }} from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport {{ record, sink }} from \"../src/audit\";\n\ntest(\"record writes once\", () => {{\n  {spy}\n  record(\"login\");\n  assert.equal(write.mock.callCount(), {expected});\n}});\n"
    )
}

fn removed_call(spy: &str, expected: &str, label: &str) -> Result<Finding, String> {
    finding_for(
        label,
        NODE_TEST,
        &[
            ("src/audit.ts", AUDIT_AFTER),
            ("test/audit.test.ts", &audit_test(spy, expected)),
        ],
        &replace_diff("src/audit.ts", 9, "  sink.write(line);", "  void line;"),
        9,
    )
}

#[test]
fn counted_method_call_replaced_by_void_is_exposed() -> Result<(), String> {
    let finding = removed_call(
        "const write = mock.method(sink, \"write\", () => undefined);",
        "1",
        "void-counted",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// Negative: a spy on another method says nothing about the removed call.
#[test]
fn void_replacement_stays_unrecognised_when_another_method_is_spied() -> Result<(), String> {
    let finding = removed_call(
        "const write = mock.method(sink, \"flush\", () => undefined);",
        "1",
        "void-other-method",
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// Negative: a count of zero passes with the call removed.
#[test]
fn void_replacement_stays_unrecognised_when_the_count_is_zero() -> Result<(), String> {
    let finding = removed_call(
        "const write = mock.method(sink, \"write\", () => undefined);",
        "0",
        "void-zero-count",
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}
