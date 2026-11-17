//! Owners reached only through a closure edge (#13592, unjs/pathe, unjs/destr,
//! unjs/ohash): a function passed as a value (`JSON.parse(text, reviver)`,
//! `keys.sort(compare)`), a constant built by a call whose callbacks run when
//! the value is invoked (`const Rule = match(re, (s) => upper(s))`), or the
//! combinator that built it. The bare-call graph has no edge for any of
//! these, so the tests that run them read `no_static_path` (pathe's glob
//! matcher alone: 250 mutants Stryker caught).
//!
//! A closure edge records reach only. When, or whether, the value runs is
//! not modeled, so the finding reads `static_unknown` whatever the oracle,
//! never `exposed`, `weakly_exposed` or a repair start. Every negative
//! asserts its positive twin first, so the refusal is the single changed
//! element and not a parse accident.

use super::*;

type Files<'a> = Vec<(&'a str, String)>;

/// `reviver` is passed to `JSON.parse` as a value, never called by name.
const REVIVER: &str = "function reviver(key: string, value: any): any {\n  return key === \"__proto__\" ? undefined : value;\n}\n\nexport function parse(text: string): any {\n  return JSON.parse(text, reviver);\n}\n";
const REVIVER_LINE: &str = "  return key === \"__proto__\" ? undefined : value;";
const REVIVER_TEST: &str = "import { expect, it } from \"vitest\";\nimport { parse } from \"../src/parse\";\n\nit(\"drops proto keys\", () => {\n  expect(parse('{\"a\":1}')).toEqual({ a: 1 });\n});\n";

/// A parser-combinator module shaped like pathe's glob grammar: `Rule` is a
/// constant `match` builds from a callback, and `apply` hands it to `run`.
const GRAMMAR: &str = "const match = (re: RegExp, handler: (s: string) => string) => {\n  return (s: string): string => (re.test(s) ? handler(s) : s);\n};\n\nconst upper = (s: string): string => {\n  return s.toUpperCase();\n};\n\nconst Rule = match(/x/, (s) => upper(s) + \"!\");\n\nfunction run(input: string, rule: (s: string) => string): string {\n  return rule(input);\n}\n\nexport function apply(input: string): string {\n  return run(input, Rule);\n}\n";
const GRAMMAR_TEST: &str = "import { expect, it } from \"vitest\";\nimport { apply } from \"../src/grammar\";\n\nit(\"applies the rule\", () => {\n  expect(apply(\"x\")).toBe(\"X!\");\n});\n";

fn finding_at(
    label: &str,
    files: &Files<'_>,
    changed_path: &str,
    changed_line: &str,
    owner: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    let mut changed_source = None;
    for (path, source) in files {
        ts_write_file(&root.join(path), source)?;
        if *path == changed_path {
            changed_source = Some(source.as_str());
        }
    }
    let line = changed_source
        .and_then(|source| source.lines().position(|line| line == changed_line))
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from {changed_path}"))?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines(changed_path, &[(line, changed_line)])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    let suffix = format!("::{owner}");
    result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|candidate| candidate.0.ends_with(&suffix))
        })
        .ok_or_else(|| format!("{label}: expected a finding for `{owner}`"))
}

fn reviver_files(owner_source: &str, test_source: &str) -> Files<'static> {
    vec![
        ("src/parse.ts", owner_source.to_string()),
        ("tests/parse.test.ts", test_source.to_string()),
    ]
}

fn reviver_finding(label: &str, owner_source: &str, test_source: &str) -> Result<Finding, String> {
    finding_at(
        label,
        &reviver_files(owner_source, test_source),
        "src/parse.ts",
        REVIVER_LINE,
        "reviver",
    )
}

/// Entries of `owner` in `source` as `(name, closure)` pairs.
fn entries(source: &str, owner: &str) -> Vec<(String, bool, u8)> {
    extract_owners(Path::new("src/m.ts"), source)
        .into_iter()
        .find(|candidate| candidate.name == owner)
        .map(|candidate| {
            candidate
                .module_entries
                .into_iter()
                .map(|entry| (entry.name, entry.closure, entry.hops))
                .collect()
        })
        .unwrap_or_default()
}

fn closure_entry_names(source: &str, owner: &str) -> Vec<String> {
    entries(source, owner)
        .into_iter()
        .filter_map(|(name, closure, _)| closure.then_some(name))
        .collect()
}

fn relates_through_entry(finding: &Finding) -> bool {
    finding
        .related_tests
        .iter()
        .any(|test| test.relation_reason == Some(crate::domain::RelationReason::HelperOwnerCall))
}

/// Closure reach reads unknown, names its path, and never starts a repair.
fn assert_closure_related(label: &str, finding: &Finding) {
    assert!(
        relates_through_entry(finding),
        "{label}: the test reaches the owner through a closure edge, related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.ripr.reach.state, StageState::Yes, "{label}");
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{label}");
    assert_eq!(
        finding.stop_reasons,
        [StopReason::PropagationEvidenceUnknown],
        "{label}"
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("only through callback or value-passing callers")),
        "{label}: missing: {:?}",
        finding.missing
    );
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("repair_packet_ready: true")),
        "{label}: closure reach never starts a repair: {:?}",
        finding.evidence
    );
}

#[test]
fn a_function_passed_as_a_value_is_a_closure_entry() {
    assert_eq!(
        entries(REVIVER, "reviver"),
        [("parse".to_string(), true, 1)]
    );
    // A sort comparator, an array callback and a stored handler alike.
    let comparator = "function compare(a: string, b: string): number {\n  return a < b ? -1 : 1;\n}\nexport function sortKeys(o: object): string[] {\n  return Object.keys(o).sort(compare);\n}\n";
    assert_eq!(closure_entry_names(comparator, "compare"), ["sortKeys"]);
    let stored = "function onKey(k: string): string {\n  return k;\n}\nconst handlers = { key: onKey };\nexport function dispatch(k: string): string {\n  return handlers.key(k);\n}\n";
    // `handlers` is an object literal, not a call-built constant: it is no
    // node of the graph, so nothing reaches `onKey` through it.
    assert!(closure_entry_names(stored, "onKey").is_empty());
}

#[test]
fn a_test_of_the_caller_relates_to_a_passed_function_as_unknown() -> Result<(), String> {
    let finding = reviver_finding("reviver", REVIVER, REVIVER_TEST)?;
    assert_closure_related("reviver", &finding);
    Ok(())
}

#[test]
fn a_weak_oracle_through_a_closure_edge_is_still_unknown() -> Result<(), String> {
    // A same-module call entry keeps a weak oracle `weakly_exposed`; a
    // closure edge does not, because the value may never run.
    let weak = REVIVER_TEST.replace(
        "expect(parse('{\"a\":1}')).toEqual({ a: 1 });",
        "expect(parse('{\"a\":1}')).toBeTruthy();",
    );
    let finding = reviver_finding("reviver-weak", REVIVER, &weak)?;
    assert_closure_related("reviver-weak", &finding);
    Ok(())
}

#[test]
fn combinator_built_constants_reach_their_callbacks_and_combinator() -> Result<(), String> {
    // `apply` passes `Rule` (a value) to `run`; `Rule` was built by `match`
    // with a callback that calls `upper`.
    assert_eq!(closure_entry_names(GRAMMAR, "Rule"), ["apply"]);
    assert_eq!(closure_entry_names(GRAMMAR, "match"), ["apply"]);
    assert_eq!(closure_entry_names(GRAMMAR, "upper"), ["apply"]);
    // `run` is a bare call of `apply`: a call entry, not a closure one.
    assert_eq!(entries(GRAMMAR, "run"), [("apply".to_string(), false, 1)]);
    let files: Files<'_> = vec![
        ("src/grammar.ts", GRAMMAR.to_string()),
        ("tests/grammar.test.ts", GRAMMAR_TEST.to_string()),
    ];
    for (owner, line) in [
        ("upper", "  return s.toUpperCase();"),
        (
            "match",
            "  return (s: string): string => (re.test(s) ? handler(s) : s);",
        ),
        ("Rule", "const Rule = match(/x/, (s) => upper(s) + \"!\");"),
    ] {
        let label = format!("grammar-{owner}");
        let finding = finding_at(&label, &files, "src/grammar.ts", line, owner)?;
        assert_closure_related(&label, &finding);
    }
    Ok(())
}

#[test]
fn a_function_both_called_and_passed_is_a_closure_entry() {
    // The bare-call graph refuses a name with any non-call mention (it may be
    // an alias); the closure edge still records the reach, as unknown.
    let both = "function reviver(key: string, value: any): any {\n  return value;\n}\nexport function parse(text: string): any {\n  reviver(\"\", text);\n  return JSON.parse(text, reviver);\n}\n";
    assert_eq!(entries(both, "reviver"), [("parse".to_string(), true, 1)]);
}

#[test]
fn closure_edges_are_not_invented_for_rebound_names() {
    assert_eq!(closure_entry_names(REVIVER, "reviver"), ["parse"]);
    // A parameter of the same name is what `JSON.parse` receives.
    let parameter = REVIVER.replace(
        "export function parse(text: string): any {",
        "export function parse(text: string, reviver: any): any {",
    );
    assert!(entries(&parameter, "reviver").is_empty());
    // So is a body-local declaration, even after the reference.
    let local = REVIVER.replace(
        "  return JSON.parse(text, reviver);\n}",
        "  return JSON.parse(text, reviver);\n  function reviver() {}\n}",
    );
    assert!(entries(&local, "reviver").is_empty());
    // A nested callback's parameter binds it too.
    let callback = REVIVER.replace(
        "  return JSON.parse(text, reviver);",
        "  return [text].map((reviver) => JSON.parse(text, reviver));",
    );
    assert!(entries(&callback, "reviver").is_empty());
    // A module that reassigns the name may hold another function.
    let reassigned = REVIVER.replace("function reviver", "let reviver = function")
        + "reviver = (key: string, value: any) => value;\n";
    assert!(entries(&reassigned, "reviver").is_empty());
    // A type position runs nothing.
    let typed = REVIVER.replace(
        "  return JSON.parse(text, reviver);",
        "  const r: typeof reviver | undefined = undefined;\n  return JSON.parse(text, r);",
    );
    assert!(entries(&typed, "reviver").is_empty());
}

#[test]
fn closure_paths_stop_at_their_hop_bound() {
    // `reviver` passed by `p0`, which `p1`..`p7` call in turn: the export
    // `p7` is 8 edges away, inside the total bound, but its path starts with
    // seven bare calls, past the three a path may take before its first
    // closure edge.
    let mut chain = String::from(
        "function reviver(key: string, value: any): any {\n  return value;\n}\nfunction p0(text: string): any {\n  return JSON.parse(text, reviver);\n}\n",
    );
    for hop in 1..=7 {
        chain.push_str(&format!(
            "function p{hop}(text: string): any {{\n  return p{}(text);\n}}\n",
            hop - 1
        ));
    }
    let within = format!("{chain}export {{ p7 }};\n");
    // The call prefix `p7 -> p6 -> p5 -> p4` passes the bare-call bound of
    // three before the closure edge, which is not followed.
    assert!(entries(&within, "reviver").is_empty());
    let mut short = String::from(
        "function reviver(key: string, value: any): any {\n  return value;\n}\nconst r1 = (text: string): any => JSON.parse(text, reviver);\n",
    );
    for hop in 2..=8 {
        short.push_str(&format!(
            "const r{hop} = (text: string): any => wrap(text, r{});\n",
            hop - 1
        ));
    }
    short.push_str(
        "function wrap(text: string, f: (t: string) => any): any {\n  return f(text);\n}\n",
    );
    // `r8 -> r7 -> .. -> r1 -> reviver` is eight closure edges.
    let eight = format!("{short}export {{ r8 }};\n");
    assert_eq!(closure_entry_names(&eight, "reviver"), ["r8"]);
    let nine =
        format!("{short}const r9 = (text: string): any => wrap(text, r8);\nexport {{ r9 }};\n");
    assert!(closure_entry_names(&nine, "reviver").is_empty());
}

#[test]
fn a_test_that_calls_nothing_reaching_the_value_does_not_relate() -> Result<(), String> {
    let twin = reviver_finding("unrelated-twin", REVIVER, REVIVER_TEST)?;
    assert_closure_related("unrelated-twin", &twin);
    let other = REVIVER.to_string()
        + "export function size(text: string): number {\n  return text.length;\n}\n";
    let test = "import { expect, it } from \"vitest\";\nimport { size } from \"../src/parse\";\n\nit(\"sizes\", () => {\n  expect(size(\"ab\")).toBe(2);\n});\n";
    let finding = reviver_finding("unrelated", &other, test)?;
    assert!(
        !relates_through_entry(&finding),
        "a test of an export that never reaches `reviver` does not relate: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    Ok(())
}

#[test]
fn a_mocked_caller_module_voids_closure_reach() -> Result<(), String> {
    let twin = reviver_finding("mocked-twin", REVIVER, REVIVER_TEST)?;
    assert_closure_related("mocked-twin", &twin);
    let mocked = REVIVER_TEST
        .replace(
            "import { parse } from \"../src/parse\";\n",
            "import { parse } from \"../src/parse\";\nvi.mock(\"../src/parse\");\n",
        )
        .replace(
            "import { expect, it } from \"vitest\";",
            "import { expect, it, vi } from \"vitest\";",
        );
    let finding = reviver_finding("mocked", REVIVER, &mocked)?;
    assert!(
        !relates_through_entry(&finding),
        "a mocked module runs no closure: {:?}",
        finding.related_tests
    );
    Ok(())
}

#[test]
fn closure_reach_crosses_into_an_importing_module() -> Result<(), String> {
    // pathe: `matchesGlob` (in `_path.ts`) calls the imported `matchGlob`,
    // which reaches the grammar callbacks only through closure edges.
    let barrel = "import { apply } from \"./grammar\";\nexport function applyAll(input: string): string {\n  return apply(input);\n}\n";
    let test = "import { expect, it } from \"vitest\";\nimport { applyAll } from \"../src/all\";\n\nit(\"applies\", () => {\n  expect(applyAll(\"x\")).toBe(\"X!\");\n});\n";
    let files: Files<'_> = vec![
        ("src/grammar.ts", GRAMMAR.to_string()),
        ("src/all.ts", barrel.to_string()),
        ("tests/all.test.ts", test.to_string()),
    ];
    let finding = finding_at(
        "cross-closure",
        &files,
        "src/grammar.ts",
        "  return s.toUpperCase();",
        "upper",
    )?;
    assert_closure_related("cross-closure", &finding);
    Ok(())
}

#[test]
fn member_names_object_keys_and_labels_are_not_references() {
    let twin = REVIVER.to_string();
    assert_eq!(closure_entry_names(&twin, "reviver"), ["parse"]);
    let named = REVIVER.replace(
        "  return JSON.parse(text, reviver);",
        "  const o: any = { reviver: 1 };\n  reviver: for (const c of text) {\n    break reviver;\n  }\n  return JSON.parse(text, o.reviver);",
    );
    assert!(entries(&named, "reviver").is_empty());
}

#[test]
fn a_bare_called_combinator_constant_reads_as_a_closure_entry() {
    // `Rule(s)` is a bare call, but what runs is the callback `match` was
    // given: the entry reads as closure reach, never a call entry.
    let direct =
        GRAMMAR.to_string() + "export function direct(s: string): string {\n  return Rule(s);\n}\n";
    assert!(
        entries(&direct, "Rule").contains(&("direct".to_string(), true, 1)),
        "{:?}",
        entries(&direct, "Rule")
    );
}

#[test]
fn mixed_call_and_closure_reach_keeps_the_same_module_reading() -> Result<(), String> {
    // `helper` is called by `a` and passed by `b`: a weak oracle on the call
    // path stays `weakly_exposed`, and the text does not claim the reach is
    // only through callbacks.
    let owner = "function helper(key: string, value: any): any {\n  return key === \"x\" ? null : value;\n}\nexport function a(key: string): any {\n  return helper(key, 1);\n}\nexport function b(text: string): any {\n  return JSON.parse(text, helper);\n}\n";
    let test = "import { expect, it } from \"vitest\";\nimport { a, b } from \"../src/parse\";\n\nit(\"a\", () => {\n  expect(a(\"y\")).toBeTruthy();\n});\n\nit(\"b\", () => {\n  expect(b(\"1\")).toBeTruthy();\n});\n";
    assert_eq!(
        entries(owner, "helper"),
        [("a".to_string(), false, 1), ("b".to_string(), true, 1)]
    );
    let finding = finding_at(
        "mixed",
        &reviver_files(owner, test),
        "src/parse.ts",
        "  return key === \"x\" ? null : value;",
        "helper",
    )?;
    assert!(
        relates_through_entry(&finding),
        "{:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("only through same-module or callback callers")),
        "missing: {:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_cross_module_closure_chain_takes_the_closure_bound() -> Result<(), String> {
    // `shout` is three edges from `apply` (Rule, upper, shout) and the
    // import makes four: past the call bound, inside the closure bound.
    let grammar = GRAMMAR.replace(
        "const upper = (s: string): string => {\n  return s.toUpperCase();\n};",
        "const shout = (s: string): string => {\n  return s + \"?\";\n};\n\nconst upper = (s: string): string => {\n  return shout(s.toUpperCase());\n};",
    );
    assert_eq!(entries(&grammar, "shout"), [("apply".to_string(), true, 3)]);
    let importer = "import { apply } from \"./grammar\";\nexport function applyAll(input: string): string {\n  return apply(input);\n}\n";
    let test = "import { expect, it } from \"vitest\";\nimport { applyAll } from \"../src/all\";\n\nit(\"applies\", () => {\n  expect(applyAll(\"x\")).toBe(\"X?!\");\n});\n";
    let files: Files<'_> = vec![
        ("src/grammar.ts", grammar),
        ("src/all.ts", importer.to_string()),
        ("tests/all.test.ts", test.to_string()),
    ];
    let finding = finding_at(
        "cross-closure-bound",
        &files,
        "src/grammar.ts",
        "  return s + \"?\";",
        "shout",
    )?;
    assert_closure_related("cross-closure-bound", &finding);
    Ok(())
}

#[test]
fn member_writes_do_not_count_as_rebinding() {
    // `cache.reviver = ...` mutates `cache`, it does not rebind `reviver`;
    // `({ reviver } = other)` does.
    let member = REVIVER.to_string()
        + "const cache: any = {};\nexport function remember(): void {\n  cache.reviver = 1;\n  cache[reviver.name] = 2;\n}\n";
    assert_eq!(
        closure_entry_names(&member, "reviver"),
        ["parse", "remember"]
    );
    let destructured = REVIVER.replace("function reviver", "let reviver = function")
        + "export function reset(other: any): void {\n  ({ reviver } = other);\n}\n";
    assert!(entries(&destructured, "reviver").is_empty());
}

#[test]
fn an_owner_past_the_closure_entry_cap_keeps_none() {
    let exports = |count: usize| {
        let mut source =
            String::from("function reviver(key: string, value: any): any {\n  return value;\n}\n");
        for index in 0..count {
            source.push_str(&format!(
                "export function parse{index}(text: string): any {{\n  return JSON.parse(text, reviver);\n}}\n"
            ));
        }
        source
    };
    assert_eq!(
        closure_entry_names(&exports(MAX_CLOSURE_ENTRIES_PER_OWNER), "reviver").len(),
        MAX_CLOSURE_ENTRIES_PER_OWNER
    );
    assert!(entries(&exports(MAX_CLOSURE_ENTRIES_PER_OWNER + 1), "reviver").is_empty());
}
