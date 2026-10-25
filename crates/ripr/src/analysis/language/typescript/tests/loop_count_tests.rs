//! RIPR-SPEC-0246 rule 8: a counted-loop header change that a related test
//! reads only through a mock call count is `static_unknown`, not a gap.

use super::*;

const RETRY: &str = "export async function withRetry<T>(fn: () => Promise<T>, attempts: number): Promise<T> {\n  let lastError: unknown;\n  for (let i = 0; i < attempts; i++) {\n    try {\n      return await fn();\n    } catch (error) {\n      lastError = error;\n    }\n  }\n  throw lastError;\n}\n";
const HEAD: &str = "import { it, mock } from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport { withRetry } from \"../src/retry\";\n";
const OLD: &str = "  for (let i = 0; i < attempts; i++) {";
const NEW: &str = "  for (let i = 0; i < attempts - 1; i++) {";

fn run(
    label: &str,
    source: &str,
    removed: &str,
    added: &str,
    test: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/retry.ts"), source)?;
    ts_write_file(&root.join("test/retry.test.ts"), test)?;
    let line = 3;
    let changed = ChangedFile {
        path: PathBuf::from("src/retry.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: added.to_string(),
            new_side_line: line,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: removed.to_string(),
            new_side_line: line,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

fn retry_test(body: &str) -> String {
    format!("{HEAD}\nit(\"x\", async () => {{\n{body}\n}});\n")
}

const COUNTED: &str = "  const fn = mock.fn(async () => { throw new Error(\"down\"); });\n  await assert.rejects(withRetry(fn, 2), /down/);\n  assert.equal(fn.mock.callCount(), 2);";

#[test]
fn a_call_count_over_a_changed_loop_bound_is_unknown() -> Result<(), String> {
    let finding = run("loop-count", RETRY, OLD, NEW, &retry_test(COUNTED))?;
    assert_eq!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    assert_eq!(
        finding.stop_reasons,
        vec![StopReason::GapEvidenceUnresolved]
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("counted loop header")),
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_value_assertion_alone_is_not_made_unknown() -> Result<(), String> {
    let body = "  const fn = mock.fn(async () => \"ok\");\n  assert.equal(await withRetry(fn, 3), \"ok\");";
    let finding = run("loop-value", RETRY, OLD, NEW, &retry_test(body))?;
    assert_ne!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_changed_condition_that_is_not_a_loop_header_is_not_made_unknown() -> Result<(), String> {
    let source = RETRY.replace(OLD, "  if (attempts > 0) {");
    let finding = run(
        "loop-if",
        &source,
        "  if (attempts > 0) {",
        "  if (attempts > 1) {",
        &retry_test(COUNTED),
    )?;
    assert_ne!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_for_of_loop_header_change_is_not_made_unknown() -> Result<(), String> {
    let source = RETRY.replace(OLD, "  for (const step of steps) {");
    let finding = run(
        "loop-for-of",
        &source,
        "  for (const step of steps) {",
        "  for (const step of steps.slice(1)) {",
        &retry_test(COUNTED),
    )?;
    assert_ne!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_call_count_on_a_mock_the_owner_never_receives_is_not_made_unknown() -> Result<(), String> {
    let body = "  const fn = mock.fn(async () => \"ok\");\n  const log = mock.fn();\n  assert.equal(await withRetry(fn, 3), \"ok\");\n  assert.equal(log.mock.callCount(), 0);";
    let finding = run("loop-unrelated-count", RETRY, OLD, NEW, &retry_test(body))?;
    assert_ne!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}

const REAL_VALUE: &str = "  const real = mock.fn(async () => \"ok\");\n  assert.equal(await withRetry(real, 3), \"ok\");";

fn assert_gap(label: &str, finding: &Finding) {
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{label}: {:?}",
        finding.missing
    );
    assert!(
        !finding
            .stop_reasons
            .contains(&StopReason::GapEvidenceUnresolved),
        "{label}: {:?}",
        finding.stop_reasons
    );
}

fn assert_abstains(label: &str, finding: &Finding) {
    assert_eq!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{label}: {:?}",
        finding.missing
    );
    assert_eq!(
        finding.stop_reasons,
        vec![StopReason::GapEvidenceUnresolved],
        "{label}"
    );
}

fn assert_stays_a_gap(label: &str, body: &str) -> Result<(), String> {
    let finding = run(label, RETRY, OLD, NEW, &retry_test(body))?;
    assert_gap(label, &finding);
    Ok(())
}

#[test]
fn a_member_argument_is_not_the_counted_mock() -> Result<(), String> {
    assert_stays_a_gap(
        "loop-member-arg",
        "  const fn = mock.fn(async () => \"ok\");\n  const other = { fn: mock.fn(async () => \"ok\") };\n  assert.equal(await withRetry(other.fn, 3), \"ok\");\n  assert.equal(fn.mock.callCount(), 0);",
    )
}

#[test]
fn an_unrelated_member_call_or_a_comment_is_not_the_owner_call() -> Result<(), String> {
    let counted =
        "  const fn = mock.fn(async () => \"ok\");\n  assert.equal(fn.mock.callCount(), 2);";
    assert_stays_a_gap(
        "loop-member-call",
        &format!(
            "{REAL_VALUE}\n  const other = {{ withRetry: (f: unknown, n: number) => n }};\n  other.withRetry(fn, 2);\n{counted}"
        ),
    )?;
    assert_stays_a_gap(
        "loop-comment",
        &format!("{REAL_VALUE}\n  // withRetry(fn, 2)\n{counted}"),
    )
}

#[test]
fn an_optional_call_passing_the_mock_is_unknown() -> Result<(), String> {
    let body = COUNTED.replace("withRetry(fn, 2)", "withRetry?.(fn, 2)");
    let finding = run("loop-optional-call", RETRY, OLD, NEW, &retry_test(&body))?;
    assert_eq!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_renamed_import_passing_the_mock_is_unknown() -> Result<(), String> {
    let test = format!(
        "{}\nit(\"x\", async () => {{\n{}\n}});\n",
        HEAD.replace("{ withRetry }", "{ withRetry as retry }"),
        COUNTED.replace("withRetry(fn, 2)", "retry(fn, 2)")
    );
    let finding = run("loop-alias", RETRY, OLD, NEW, &test)?;
    assert_eq!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn trivia_after_a_member_dot_is_still_a_member_call() -> Result<(), String> {
    let declared = "  const fn = mock.fn(async () => \"ok\");";
    let counted = "  assert.equal(fn.mock.callCount(), 2);";
    for (label, call) in [
        ("loop-dot-space", "other. withRetry(fn, 2);"),
        ("loop-dot-comment", "other./* member */withRetry(fn, 2);"),
    ] {
        assert_stays_a_gap(
            label,
            &format!(
                "{REAL_VALUE}\n{declared}\n  const other = {{ withRetry: (f: unknown, n: number) => n }};\n  {call}\n{counted}"
            ),
        )?;
    }
    Ok(())
}

#[test]
fn a_default_import_passing_the_mock_is_unknown() -> Result<(), String> {
    let source = RETRY.replace("export async function", "export default async function");
    let head = HEAD.replace("import { withRetry }", "import retry");
    let test = format!(
        "{head}\nit(\"x\", async () => {{\n{}\n}});\n",
        COUNTED.replace("withRetry(fn, 2)", "retry(fn, 2)")
    );
    let finding = run("loop-default-import", &source, OLD, NEW, &test)?;
    assert_eq!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    assert_eq!(
        finding.stop_reasons,
        vec![StopReason::GapEvidenceUnresolved]
    );
    Ok(())
}

#[test]
fn a_local_canonical_name_beside_a_renamed_import_is_not_the_owner_call() -> Result<(), String> {
    let head = HEAD.replace("{ withRetry }", "{ withRetry as retry }");
    let wrap = |body: &str| format!("{head}\nit(\"x\", async () => {{\n{body}\n}});\n");
    let shadowed = wrap(
        "  const real = mock.fn(async () => \"ok\");\n  const fn = mock.fn();\n  const withRetry = (f: unknown, n: number) => n;\n  assert.equal(await retry(real, 3), \"ok\");\n  withRetry(fn, 2);\n  assert.equal(fn.mock.callCount(), 0);",
    );
    let finding = run("loop-alias-local-canonical", RETRY, OLD, NEW, &shadowed)?;
    assert_ne!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    // The same test with the lawful alias call counted still abstains.
    let lawful = wrap(
        "  const fn = mock.fn(async () => { throw new Error(\"down\"); });\n  const withRetry = (f: unknown, n: number) => n;\n  await assert.rejects(retry(fn, 2), /down/);\n  assert.equal(fn.mock.callCount(), 2);",
    );
    let finding = run("loop-alias-local-lawful", RETRY, OLD, NEW, &lawful)?;
    assert_eq!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn every_way_of_handing_the_counted_mock_to_the_owner_abstains() -> Result<(), String> {
    for (label, call) in [
        ("loop-arrow", "withRetry(() => fn(), 2)"),
        ("loop-bind", "withRetry(fn.bind(null), 2)"),
        ("loop-cast", "withRetry(fn as any, 2)"),
        ("loop-paren", "withRetry((fn), 2)"),
        ("loop-spread", "withRetry(...[fn, 2])"),
    ] {
        let body = format!(
            "  const fn = mock.fn(async () => {{ throw new Error(\"down\"); }});\n  await assert.rejects({call}, /down/);\n  assert.equal(fn.mock.callCount(), 2);"
        );
        let finding = run(label, RETRY, OLD, NEW, &retry_test(&body))?;
        assert_abstains(label, &finding);
    }
    for (label, setup, call, count) in [
        (
            "loop-helper-arrow",
            "const go = (f: any) => withRetry(f, 2);",
            "go(fn)",
            "fn.mock.callCount()",
        ),
        (
            "loop-helper-function",
            "function go(f: any) {\n    return withRetry(f, 2);\n  }",
            "go(fn)",
            "fn.mock.callCount()",
        ),
        (
            "loop-args-carrier",
            "const args = [fn, 2] as const;",
            "withRetry(...args)",
            "fn.mock.callCount()",
        ),
        (
            "loop-container-carrier",
            "const h = { fn };",
            "withRetry(h.fn, 2)",
            "fn.mock.callCount()",
        ),
        (
            "loop-count-alias",
            "",
            "withRetry(fn, 2)",
            "(() => { const calls = fn.mock.calls; return calls.length; })()",
        ),
        (
            "loop-count-cast",
            "",
            "withRetry(fn, 2)",
            "(fn as any).mock.callCount()",
        ),
    ] {
        let body = format!(
            "  const fn = mock.fn(async () => {{ throw new Error(\"down\"); }});\n  {setup}\n  await assert.rejects({call}, /down/);\n  assert.equal({count}, 2);"
        );
        let finding = run(label, RETRY, OLD, NEW, &retry_test(&body))?;
        assert_abstains(label, &finding);
    }
    let labelled = RETRY.replace(OLD, "  outer: for (let i = 0; i < attempts; i++) {");
    let labelled_new = "  outer: for (let i = 0; i < attempts - 1; i++) {";
    let finding = run(
        "loop-label",
        &labelled,
        "  outer: for (let i = 0; i < attempts; i++) {",
        labelled_new,
        &retry_test(COUNTED),
    )?;
    assert_abstains("loop-label", &finding);
    let alias = "  const fn = mock.fn(async () => { throw new Error(\"down\"); });\n  const task = fn;\n  await assert.rejects(withRetry(task, 2), /down/);\n  assert.equal(fn.mock.callCount(), 2);";
    let finding = run("loop-carrier", RETRY, OLD, NEW, &retry_test(alias))?;
    assert_eq!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}

/// A spelling the adapter does not link to the owner as a call has only a
/// heuristic relation, so its finding is advisory and never a repair target;
/// the guard still reads these spellings as handoffs once a relation exists.
#[test]
fn owner_spellings_without_a_direct_relation_are_never_actionable() -> Result<(), String> {
    for (label, call) in [
        ("loop-generic", "withRetry<number>(fn, 2)"),
        ("loop-nonnull", "withRetry!(fn, 2)"),
        ("loop-comma", "(0, withRetry)(fn, 2)"),
        ("loop-call", "withRetry.call(null, fn, 2)"),
        ("loop-apply", "withRetry.apply(null, [fn, 2])"),
        ("loop-owner-alias", "(withRetry as typeof withRetry)(fn, 2)"),
    ] {
        let body = format!(
            "  const fn = mock.fn(async () => {{ throw new Error(\"down\"); }});\n  await assert.rejects({call}, /down/);\n  assert.equal(fn.mock.callCount(), 2);"
        );
        let finding = run(label, RETRY, OLD, NEW, &retry_test(&body))?;
        // Only a heuristic-only link is `ambiguous_related_test`: the finding
        // never routes a repair packet.
        let heuristic_only = finding
            .missing
            .iter()
            .any(|line| line.contains("ambiguous_related_test"));
        assert!(
            finding.class == ExposureClass::StaticUnknown || heuristic_only,
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

#[test]
fn a_method_owner_called_through_a_receiver_abstains() -> Result<(), String> {
    let source = "export class Svc {\n  async retry<T>(fn: () => Promise<T>, attempts: number): Promise<T> {\n  for (let i = 0; i < attempts; i++) {\n    try {\n      return await fn();\n    } catch (error) {}\n  }\n  throw new Error(\"down\");\n  }\n}\n";
    let test = "import { it, mock } from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport { Svc } from \"../src/retry\";\n\nit(\"x\", async () => {\n  const svc = new Svc();\n  const fn = mock.fn(async () => { throw new Error(\"down\"); });\n  await assert.rejects(svc.retry(fn, 2), /down/);\n  assert.equal(fn.mock.callCount(), 2);\n});\n";
    let finding = run("loop-method", source, OLD, NEW, test)?;
    assert_eq!(
        finding.class,
        ExposureClass::StaticUnknown,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn assignment_and_annotated_carriers_abstain() -> Result<(), String> {
    for (label, setup) in [
        ("loop-reassigned", "let task = real;\n  task = fn;"),
        ("loop-annotated", "const task: () => Promise<string> = fn;"),
    ] {
        let body = format!(
            "  const real = mock.fn(async () => \"ok\");\n  const fn = mock.fn(async () => {{ throw new Error(\"down\"); }});\n  {setup}\n  await assert.rejects(withRetry(task, 2), /down/);\n  assert.equal(fn.mock.callCount(), 2);"
        );
        let finding = run(label, RETRY, OLD, NEW, &retry_test(&body))?;
        assert_abstains(label, &finding);
    }
    Ok(())
}

#[test]
fn a_callback_parameter_named_like_the_mock_is_not_the_mock() -> Result<(), String> {
    assert_stays_a_gap(
        "loop-param-shadow",
        "  const fn = mock.fn();\n  assert.equal(await withRetry(async (fn = async () => \"ok\") => fn(), 3), \"ok\");\n  assert.equal(fn.mock.callCount(), 0);",
    )?;
    // A local declared inside the callback shadows the outer mock.
    assert_stays_a_gap(
        "loop-callback-local",
        "  const fn = mock.fn();\n  assert.equal(await withRetry(async () => { const fn = async () => \"ok\"; return fn(); }, 3), \"ok\");\n  assert.equal(fn.mock.callCount(), 0);",
    )?;
    // An outer call before a nested block that declares the same name is
    // still the outer mock.
    let label = "loop-local-outer-call";
    let callback = "() => { fn(); { const fn = () => 0; } throw new Error(\"down\"); }";
    let body = format!(
        "  const fn = mock.fn();\n  await assert.rejects(withRetry({callback}, 2), /down/);\n  assert.equal(fn.mock.callCount(), 2);"
    );
    let finding = run(label, RETRY, OLD, NEW, &retry_test(&body))?;
    assert_abstains(label, &finding);
    // A destructuring default reads the outer mock; it is not a binding.
    let label = "loop-local-destructure-default";
    let body = "  const fn = mock.fn(async () => { throw new Error(\"down\"); });\n  await assert.rejects(withRetry(async () => { const { a = fn } = {}; return a(); }, 2), /down/);\n  assert.equal(fn.mock.callCount(), 2);";
    let finding = run(label, RETRY, OLD, NEW, &retry_test(body))?;
    assert_abstains(label, &finding);
    // A second declarator shadows the mock like the first.
    let label = "loop-local-multi-declarator";
    let callback = "async () => { const a = 1, fn = async () => \"ok\"; return fn(); }";
    let body = format!(
        "  const fn = mock.fn();\n  assert.equal(await withRetry({callback}, 3), \"ok\");\n  assert.equal(fn.mock.callCount(), 0);"
    );
    let finding = run(label, RETRY, OLD, NEW, &retry_test(&body))?;
    assert_gap(label, &finding);
    // A block inside an expression-bodied arrow is the nested function's own.
    let label = "loop-local-expression-arrow";
    let callback = "(x: boolean) => x ? fn : async () => { const fn = 1; return fn; }";
    let body = format!(
        "  const fn = mock.fn(async () => {{ throw new Error(\"down\"); }});\n  await assert.rejects(withRetry({callback}, 2), /down/);\n  assert.equal(fn.mock.callCount(), 2);"
    );
    let finding = run(label, RETRY, OLD, NEW, &retry_test(&body))?;
    assert_abstains(label, &finding);
    // A declaration in a nested block or function, a type annotation or a
    // loop head does not rebind the callback's own `fn`.
    for (label, callback) in [
        (
            "loop-local-block",
            "async () => { if (false) { const fn = 1; } return fn(); }",
        ),
        (
            "loop-local-nested-fn",
            "async () => { const g = () => { const fn = 1; return fn; }; g(); return fn(); }",
        ),
        (
            "loop-local-annotation",
            "async () => { const r: ReturnType<typeof fn> = fn(); return r; }",
        ),
        (
            "loop-local-loop-head",
            "async () => { for (const c of fn.mock.calls) {} return fn(); }",
        ),
    ] {
        let body = format!(
            "  const fn = mock.fn(async () => {{ throw new Error(\"down\"); }});\n  await assert.rejects(withRetry({callback}, 2), /down/);\n  assert.equal(fn.mock.callCount(), 2);"
        );
        let finding = run(label, RETRY, OLD, NEW, &retry_test(&body))?;
        assert_abstains(label, &finding);
    }
    assert_stays_a_gap(
        "loop-function-shadow",
        "  const fn = mock.fn();\n  assert.equal(await withRetry(async function (fn: any) { return fn(); }, 3), \"ok\");\n  assert.equal(fn.mock.callCount(), 0);",
    )
}

const SVC: &str = "export class Svc {\n  async retry<T>(fn: () => Promise<T>, attempts: number): Promise<T> {\n  for (let i = 0; i < attempts; i++) {\n    try {\n      return await fn();\n    } catch (error) {}\n  }\n  throw new Error(\"down\");\n  }\n}\n";

fn svc_test(body: &str) -> String {
    format!(
        "import {{ it, mock }} from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport {{ Svc }} from \"../src/retry\";\n\nit(\"x\", async () => {{\n  const svc = new Svc();\n{body}\n}});\n"
    )
}

#[test]
fn a_method_owner_keeps_its_receiver_identity() -> Result<(), String> {
    let counted = "  const fn = mock.fn(async () => { throw new Error(\"down\"); });\n  await assert.rejects(svc.retry(fn, 2), /down/);\n  assert.equal(fn.mock.callCount(), 2);";
    // A same-named local cannot shadow the method property.
    let beside_local = format!("  const retry = () => \"unrelated\";\n  retry();\n{counted}");
    let finding = run("loop-method-local", SVC, OLD, NEW, &svc_test(&beside_local))?;
    assert_abstains("loop-method-local", &finding);
    // Another receiver's same-named member is another binding.
    let other = "  const real = mock.fn(async () => \"ok\");\n  const fn = mock.fn();\n  const other = { retry: (f: unknown, n: number) => n };\n  assert.equal(await svc.retry(real, 3), \"ok\");\n  other.retry(fn, 2);\n  assert.equal(fn.mock.callCount(), 0);";
    let finding = run("loop-method-other", SVC, OLD, NEW, &svc_test(other))?;
    assert_gap("loop-method-other", &finding);
    // A bare local of the method's name is not the method.
    let bare = "  const real = mock.fn(async () => \"ok\");\n  const fn = mock.fn();\n  const retry = (f: () => unknown) => f();\n  assert.equal(await svc.retry(real, 3), \"ok\");\n  retry(fn);\n  assert.equal(fn.mock.callCount(), 1);";
    let finding = run("loop-method-bare-local", SVC, OLD, NEW, &svc_test(bare))?;
    assert_gap("loop-method-bare-local", &finding);
    // A foreign inline constructor does not authenticate the receiver.
    let foreign = "  class Other { retry(_task: unknown, attempts: number) { return attempts; } }\n  const real = mock.fn(async () => \"ok\");\n  const fn = mock.fn();\n  assert.equal(await svc.retry(real, 3), \"ok\");\n  (new Other()).retry(fn, 2);\n  assert.equal(fn.mock.callCount(), 0);";
    let finding = run("loop-inline-foreign", SVC, OLD, NEW, &svc_test(foreign))?;
    assert_gap("loop-inline-foreign", &finding);
    // A `new` inside the constructor arguments is not the receiver's class.
    for (label, receiver) in [
        ("loop-inline-nested-date", "new Svc(1, new Date())"),
        ("loop-inline-nested-map", "new Svc(new Map())"),
    ] {
        let body = format!(
            "  const fn = mock.fn(async () => {{ throw new Error(\"down\"); }});\n  await assert.rejects({receiver}.retry(fn, 2), /down/);\n  assert.equal(fn.mock.callCount(), 2);"
        );
        let finding = run(label, SVC, OLD, NEW, &svc_test(&body))?;
        assert_abstains(label, &finding);
    }
    // The owner's own inline constructor still hands the mock over.
    let genuine = "  const fn = mock.fn(async () => { throw new Error(\"down\"); });\n  await assert.rejects(new Svc().retry(fn, 2), /down/);\n  assert.equal(fn.mock.callCount(), 2);";
    let finding = run("loop-inline-genuine", SVC, OLD, NEW, &svc_test(genuine))?;
    assert_abstains("loop-inline-genuine", &finding);
    Ok(())
}

#[test]
fn every_asserted_count_spelling_abstains_and_an_unasserted_read_does_not() -> Result<(), String> {
    let thrower = "  const fn = mock.fn(async () => { throw new Error(\"down\"); });\n  await assert.rejects(withRetry(fn, 2), /down/);";
    for (label, count) in [
        (
            "loop-mock-alias",
            "const m = fn.mock;\n  assert.equal(m.callCount(), 2);",
        ),
        (
            "loop-calls-destructure",
            "const { calls } = fn.mock;\n  assert.equal(calls.length, 2);",
        ),
        (
            "loop-count-local",
            "const n = fn.mock.callCount();\n  assert.equal(n, 2);",
        ),
        ("loop-sinon-count", "assert.equal(fn.callCount, 2);"),
        ("loop-sinon-called", "assert.equal(fn.calledTwice, true);"),
        ("loop-reversed", "assert.equal(2, fn.mock.callCount());"),
    ] {
        let body = format!("{thrower}\n  {count}");
        let finding = run(label, RETRY, OLD, NEW, &retry_test(&body))?;
        assert_abstains(label, &finding);
    }
    // A count that no assertion reads cannot fail the test.
    let unasserted = "  const fn = mock.fn(async () => \"ok\");\n  assert.equal(await withRetry(fn, 3), \"ok\");\n  fn.mock.callCount();\n  console.log(fn.mock.callCount());";
    assert_stays_a_gap("loop-unasserted", unasserted)?;
    // A property mock is not the local handed to the owner.
    let property = "  const fn = mock.fn(async () => \"ok\");\n  const other = { fn: mock.fn(async () => \"ok\") };\n  assert.equal(await withRetry(fn, 3), \"ok\");\n  assert.equal(other.fn.mock.callCount(), 0);";
    assert_stays_a_gap("loop-property-count", property)
}

#[test]
fn a_count_read_through_an_alias_or_container_of_the_mock_abstains() -> Result<(), String> {
    for (label, setup, count) in [
        (
            "loop-count-alias",
            "const spy = fn;",
            "spy.mock.callCount()",
        ),
        (
            "loop-count-paren-alias",
            "const spy = (fn);",
            "spy.mock.callCount()",
        ),
        (
            "loop-count-cast-alias",
            "const spy = (fn as any);",
            "spy.mock.callCount()",
        ),
        (
            "loop-count-array",
            "const fns = [fn];",
            "fns[0].mock.callCount()",
        ),
        (
            "loop-count-object",
            "const o = { fn };",
            "o.fn.mock.callCount()",
        ),
    ] {
        let body = format!(
            "  const fn = mock.fn(async () => {{ throw new Error(\"down\"); }});\n  {setup}\n  await assert.rejects(withRetry(fn, 2), /down/);\n  assert.equal({count}, 2);"
        );
        let finding = run(label, RETRY, OLD, NEW, &retry_test(&body))?;
        assert_abstains(label, &finding);
    }
    Ok(())
}

#[test]
fn storing_the_mock_on_a_callee_or_in_a_callback_abstains() -> Result<(), String> {
    for (label, flow) in [
        (
            "loop-push-binding",
            "const h: any[] = [];\n  const n = h.push(fn);\n  await assert.rejects(withRetry(h[0], 2), /down/);",
        ),
        (
            "loop-mock-impl",
            "const outer = mock.fn();\n  outer.mock.mockImplementation(fn);\n  await assert.rejects(withRetry(outer, 2), /down/);",
        ),
        (
            "loop-callback-body",
            "await Promise.all([fn].map(async (f) => {\n    await assert.rejects(withRetry(f, 2), /down/);\n  }));",
        ),
        (
            "loop-closure-install",
            "mock.method(deps, \"load\", () => fn());\n  await assert.rejects(withRetry(() => deps.load(), 2), /down/);",
        ),
        (
            "loop-closure-stash",
            "const stash: any[] = [];\n  stash.push(() => fn());\n  await assert.rejects(withRetry(stash[0], 2), /down/);",
        ),
        (
            "loop-t-mock-method",
            "t.mock.method(deps, \"load\", fn);\n  await assert.rejects(withRetry(() => deps.load(), 2), /down/);",
        ),
    ] {
        let body = format!(
            "  const fn = mock.fn(async () => {{ throw new Error(\"down\"); }});\n  {flow}\n  assert.equal(fn.mock.callCount(), 2);"
        );
        let finding = run(label, RETRY, OLD, NEW, &retry_test(&body))?;
        assert_abstains(label, &finding);
    }
    Ok(())
}

#[test]
fn calling_or_configuring_an_unrelated_mock_stays_a_gap() -> Result<(), String> {
    for (label, uses) in [
        ("loop-lg-call", "lg('x');\n  await lg();"),
        (
            "loop-lg-config",
            "lg.mock.mockImplementation(() => 1);\n  lg.mock.resetCalls();",
        ),
        ("loop-lg-timer", "setTimeout(lg, 0);"),
    ] {
        let body = format!(
            "{REAL_VALUE}\n  const lg = mock.fn();\n  {uses}\n  assert.equal(lg.mock.callCount(), 0);"
        );
        assert_stays_a_gap(label, &body)?;
    }
    Ok(())
}
