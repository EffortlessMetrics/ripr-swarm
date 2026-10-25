//! RIPR-SPEC-0243 rules 10, 19 and 20 end to end: assertion forms of chai
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

// ---- `assert.rejects` with a plain substring regex (rule 20) ----

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
    rejects_retry_source(check, old, new, &src, label)
}

fn rejects_retry_source(
    check: &str,
    old: &str,
    new: &str,
    src: &str,
    label: &str,
) -> Result<Finding, String> {
    let test = format!(
        "import {{ describe, it, mock }} from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport {{ withRetry }} from \"../src/retry\";\n\ndescribe(\"withRetry\", () => {{\n  it(\"gives up\", async () => {{\n    const fn = mock.fn(async () => {{ throw new Error(\"down\"); }});\n    await assert.rejects(withRetry(fn, 2), {check});\n  }});\n}});\n"
    );
    let line = src
        .lines()
        .position(|text| text.trim() == new.trim())
        .map(|index| index + 1)
        .ok_or_else(|| format!("retry fixture lacks changed throw: {new}"))?;
    finding_for(
        label,
        NODE_TEST,
        &[("src/retry.ts", src), ("test/retry.test.ts", &test)],
        &replace_diff(
            "src/retry.ts",
            line,
            &format!("  {old}"),
            &format!("  {new}"),
        ),
        line,
    )
}

/// Confirm the error-path assertion was extracted and related before checking
/// the guard's verdict; missing fixture evidence must not explain a weak result.
fn assert_retry_class(finding: &Finding, expected: ExposureClass) {
    assert_eq!(finding.probe.family, ProbeFamily::ErrorPath, "{finding:?}");
    assert_eq!(finding.related_tests.len(), 1, "{finding:?}");
    assert_eq!(
        finding.related_tests[0].oracle_kind,
        OracleKind::ExactErrorVariant,
        "{finding:?}"
    );
    assert_eq!(
        finding.related_tests[0].oracle_strength,
        OracleStrength::Strong,
        "{finding:?}"
    );
    assert_eq!(finding.class, expected, "{finding:?}");
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
    assert_retry_class(&finding, ExposureClass::Exposed);
    Ok(())
}

/// Negative: differing from the forwarded error is insufficient. The pinned
/// literal must be absent from all of String(err), including the class prefix.
#[test]
fn rejects_substring_regex_the_new_error_satisfies_stays_weak() -> Result<(), String> {
    for (check, new) in [
        ("/other/", r#"throw new Error("other");"#),
        ("/down/", r#"throw new Error("down again");"#),
        ("/Error/", r#"throw new Error("other");"#),
    ] {
        let finding = rejects_retry(check, "throw lastError;", new, "", "rejects-satisfied")?;
        assert_retry_class(&finding, ExposureClass::WeaklyExposed);
    }
    Ok(())
}

/// The scan is deliberately lexical: an uncalled callback's unknown call
/// withholds credit, while the same callback with no call preserves it.
#[test]
fn rejects_substring_regex_sole_throw_scans_nested_callback_calls() -> Result<(), String> {
    for (body, expected) in [
        ("void attempts;", ExposureClass::Exposed),
        ("helper();", ExposureClass::WeaklyExposed),
    ] {
        let extra = format!("  const nested = () => {{ {body} }};");
        let finding = rejects_retry(
            "/down/",
            "throw lastError;",
            r#"throw new Error("other");"#,
            &extra,
            "rejects-nested-call",
        )?;
        assert_retry_class(&finding, expected);
    }
    Ok(())
}

/// Even a callback defined after the changed throw contributes a second
/// lexical throw site; it must not be mistaken for another top-level owner.
#[test]
fn rejects_substring_regex_sole_throw_scans_later_nested_throws() -> Result<(), String> {
    let new = r#"throw new Error("other");"#;
    let src = RETRY_AFTER.replace(
        "THROW",
        &format!("{new}\n  const nested = () => {{ throw new Error(\"down\"); }};"),
    );
    let finding = rejects_retry_source(
        "/down/",
        "throw lastError;",
        new,
        &src,
        "rejects-nested-throw",
    )?;
    assert_retry_class(&finding, ExposureClass::WeaklyExposed);
    Ok(())
}

/// The same parameter is safe inside the retry try/catch, but an additional
/// call outside it can reject before reaching the changed throw.
#[test]
fn rejects_substring_regex_sole_throw_refuses_uncaught_parameter_callbacks() -> Result<(), String> {
    for (extra, expected) in [
        ("", ExposureClass::Exposed),
        ("  await fn();", ExposureClass::WeaklyExposed),
    ] {
        let finding = rejects_retry(
            "/down/",
            "throw lastError;",
            r#"throw new Error("other");"#,
            extra,
            "rejects-uncaught-parameter",
        )?;
        assert_retry_class(&finding, expected);
    }
    Ok(())
}

/// A neighbor's calls and throws cannot contaminate the changed owner's
/// source span, and that neighbor cannot erase the changed owner's call.
#[test]
fn rejects_substring_regex_sole_throw_uses_only_the_changed_owner() -> Result<(), String> {
    let neighbor = "export function unrelated() {\n  helper();\n  throw new Error(\"down\");\n}\n";
    let new = r#"throw new Error("other");"#;
    for neighbor_first in [true, false] {
        for (extra, expected) in [
            ("", ExposureClass::Exposed),
            ("  await fn();\n", ExposureClass::WeaklyExposed),
        ] {
            let owner = RETRY_AFTER
                .replace("THROW", new)
                .replace("  let lastError", &format!("{extra}  let lastError"));
            let src = if neighbor_first {
                format!("{neighbor}{owner}")
            } else {
                format!("{owner}{neighbor}")
            };
            let finding = rejects_retry_source(
                "/down/",
                "throw lastError;",
                new,
                &src,
                "rejects-owner-span",
            )?;
            assert_retry_class(&finding, expected);
        }
    }
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

/// Negative: chai tests `instanceof`, so a subclass or an alias of the
/// pinned class still passes; only a swap of built-in siblings separates.
#[test]
fn chai_class_stays_weak_unless_built_in_siblings_swap() -> Result<(), String> {
    let validation = r#"throw new ValidationError("lo above hi");"#;
    let field = r#"throw new FieldValidationError("lo above hi");"#;
    for (check, old, new, label) in [
        ("ValidationError", validation, field, "chai-subclass"),
        ("Error", RANGE, TYPE, "chai-bare-error"),
        ("RangeError", RANGE, validation, "chai-user-class"),
        (
            r#"ValidationError, "lo above hi""#,
            validation,
            field,
            "chai-subclass-msg",
        ),
    ] {
        let finding = chai_class(check, old, new, label)?;
        assert_eq!(
            finding.class,
            ExposureClass::WeaklyExposed,
            "{label}: {finding:?}"
        );
    }
    let finding = chai_class("TypeError", RANGE, TYPE, "chai-sibling-swap")?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// Negative: a negated class check and a regex argument are not pins.
#[test]
fn chai_negated_class_and_regex_stay_weak() -> Result<(), String> {
    for (check, label) in [
        ("/lo above/", "chai-regex"),
        ("RangeError, /lo/", "chai-class-regex"),
    ] {
        let finding = chai_class(check, RANGE, TYPE, label)?;
        assert_eq!(
            finding.class,
            ExposureClass::WeaklyExposed,
            "{label}: {finding:?}"
        );
    }
    let test = chai_test("RangeError").replace(".to.throw", ".to.not.throw");
    let finding = finding_for(
        "chai-not-class",
        MOCHA,
        &[
            ("src/range.ts", &RANGE_SRC_AFTER.replace("THROW", TYPE)),
            ("test/range.test.ts", &test),
        ],
        &replace_diff(
            "src/range.ts",
            3,
            &format!("    {RANGE}"),
            &format!("    {TYPE}"),
        ),
        3,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

/// Negative: with another path that can throw the pinned text, a readable
/// message change is not shown to separate the versions.
#[test]
fn rejects_regex_stays_weak_with_another_throwing_path() -> Result<(), String> {
    let boom = r#"throw new Error("boom");"#;
    let bang = r#"throw new Error("bang");"#;
    let second = "  if (attempts < 0) throw new Error(\"boom\");";
    let finding = rejects_retry("/boom/", boom, bang, second, "rejects-readable-second")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    let call = "  helper();";
    let finding = rejects_retry("/boom/", boom, bang, call, "rejects-readable-call")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    let finding = rejects_retry("/boom/", boom, bang, "", "rejects-readable-sole")?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}
