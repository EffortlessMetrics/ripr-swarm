//! Issue #10925: the per-body shadow index answers every name from one pass
//! over the body. It must reproduce the per-identifier scan exactly for every
//! shape, build once per body per run, and never outlive the run.

use super::related_tests::{
    DirectoryModuleCacheScope, local_identifier_declared_in_test_body,
    local_identifier_declared_in_test_body_scanned,
};
use super::shadow_index::{BodyShadowIndex, SHADOW_INDEX_BUILDS};
use std::collections::BTreeSet;

fn index_builds() -> usize {
    SHADOW_INDEX_BUILDS.with(std::cell::Cell::get)
}

/// Bodies that between them hold every shape the per-identifier path models.
const BODIES: &[&str] = &[
    // Top-level declarators, later use, member access, object key.
    "it('a', () => {\n  const total = add(1, 2);\n  const o = { total: 1 };\n  x.total;\n  expect(total).toBe(3);\n});",
    // Nested-block declaration shadows only inside its block; outer use free.
    "it('b', () => {\n  if (ok) {\n    const owner = 1;\n    use(owner);\n  }\n  owner(2);\n});",
    // Use before declaration in the same block (TDZ) and hoisted function.
    "it('c', () => {\n  {\n    run(late);\n    let late = 1;\n    hoisted();\n    function hoisted() {}\n  }\n  late;\n});",
    // Multiple declarators, destructuring, defaults.
    "it('d', () => {\n  const a = 1, cost = (q) => 140, { tax } = h;\n  let [first, second] = pair;\n  var x = 1, { y } = z, w = 2;\n  cost(a);\n  tax + first + y + w;\n});",
    // Catch shapes.
    "it('e', () => {\n  try { run(); } catch (err) { log(err); }\n  try { run(); } catch ({ code, detail }) { code; }\n  try { run(); } catch { noop(); }\n  catch2(code);\n});",
    // Arrow and function parameters, bare arrow, defaults and type uses.
    "it('f', () => {\n  const bump = (cost) => cost + 1;\n  const twice = price => price * 2;\n  const dflt = (f = lookup, ...rest) => f;\n  const typed = (x): typeof other => x;\n  const q = (a: typeof tref => 1);\n  function build(qty, { size, deep: [inner] }) { return qty; }\n  const g = function (opt?) { return opt; };\n  const h = function* gen(star) {};\n  call((notParam) => 1);\n  call(callee)(arg => arg);\n  lookup; other; tref; size; inner; rest; notParam; callee;\n});",
    // Loop headers.
    "it('g', () => {\n  for (const item of items) { use(item); }\n  for (let i = 0; i < limit(2); i++) { i; }\n  for (const key in map) { key; }\n  for await (const chunk of stream) { chunk; }\n  for (reuse of list) {}\n  for (var v = 0, w2 = 1; v < w2; v++) {}\n  format(information);\n});",
    // Comments, strings and templates must not fabricate bindings or uses.
    "it('h', () => {\n  // const hidden = 1;\n  /* multi\n     line */ const afterComment = 2;\n  const s = 'const fake = 1; catch (fake) {}';\n  const t = `${deep} const nope = 1 ${ok ? `in ${inner2}` : ''}`;\n  const u = \"for (const str of x) {}\";\n  use(hidden, fake, nope, deep, afterComment, str, inner2);\n});",
    // Non-ASCII, escapes, unterminated string recovery.
    "it('i', () => {\n  const café = 1;\n  const esc = 'a\\'b'; const after = 2;\n  const bad = 'oops\n  const recovered = 3;\n  use(café, esc, after, bad, recovered, naïve);\n});",
    // Names that only appear as substrings of other identifiers.
    "it('j', () => {\n  const information = 1;\n  const priced = 2;\n  const catchy = 3;\n  function forward(fortune) {}\n  price + info + format + catch1 + forwardX;\n});",
    // Function declarations and class-like noise.
    "it('k', () => {\n  function price(a) { return a; }\n  async function fetchIt() {}\n  const run = async function () {};\n  price(1); fetchIt(); run();\n});",
    // Empty and degenerate bodies.
    "",
    "{",
    "}}}",
    "const",
    "catch (",
    "for (",
    "x => ",
];

fn names_for(body: &str) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = ["owner", "total", "cost", "x", "a", "z9", "_u", "$d"]
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    let mut token = String::new();
    for ch in body.chars().chain(std::iter::once(' ')) {
        if ch.is_alphanumeric() || ch == '_' || ch == '$' {
            token.push(ch);
        } else if !token.is_empty() {
            names.insert(std::mem::take(&mut token));
        }
    }
    names
}

#[test]
fn index_answers_equal_the_per_identifier_scan_for_every_name() -> Result<(), String> {
    let mut shadowed = 0usize;
    let mut checked = 0usize;
    for body in BODIES {
        let index = BodyShadowIndex::build(body);
        for name in names_for(body) {
            let Some(actual) = index.shadows(body, &name) else {
                continue;
            };
            let expected = local_identifier_declared_in_test_body_scanned(body, &name);
            if actual != expected {
                return Err(format!(
                    "{name:?} in {body:?}: scan says {expected}, index says {actual}"
                ));
            }
            checked += 1;
            shadowed += usize::from(actual);
        }
    }
    // The comparison is only meaningful if it covers both answers broadly.
    if checked < 300 || shadowed < 30 || checked - shadowed < 100 {
        return Err(format!(
            "thin corpus: checked {checked}, shadowed {shadowed}"
        ));
    }
    Ok(())
}

/// Single-name shape templates; `N` is the candidate name.
const SHAPES: &[&str] = &[
    "const N = 1;\nuse(N);",
    "{\n  const N = 1;\n  use(N);\n}\nN(2);",
    "{\n  use(N);\n  let N = 1;\n}\nN;",
    "use(N);\nfunction N() {}",
    "const a = 1, N = 2;\nN;",
    "const { N } = h;\nN;",
    "const [N] = h;\nN;",
    "let x = 1, { N } = h;\nN;",
    "try { r(); } catch (N) { N; }",
    "try { r(); } catch ({ N }) {}\nN;",
    "const f = (N) => N;\nN;",
    "const f = (a, N = 1) => a;\nN;",
    "const f = (a = N) => a;\nN;",
    "const f = (a): typeof N => a;\nN;",
    "const f = N => 1;\nN;",
    "const f = x: typeof N => 1;\nN;",
    "const f = o.N => 1;",
    "function g(a, N) {}\nN;",
    "const g = function (N) {};\nN;",
    "const g = function* h(a, ...N) {};",
    "for (const N of xs) {}\nN;",
    "for (let i = 0, N = 1; i < N; i++) {}",
    "for (let i = 0; i < N(2); i++) {}",
    "for (N in xs) {}",
    "for (const N in xs) { N; }",
    "for await (const N of xs) {}",
    "// const N = 1;\nN;",
    "const s = 'const N = 1';\nN;",
    "const t = `${N} const N2 = 1`;",
    "const t = `${ (() => { const N = 1; return N; })() }`;\nN;",
    "const o = { N: 1 };\nx.N;",
    "const N = 1;\nconst o = { a: 1, N: 2 };",
    "const N = 1;\nx.N;",
    "const N = 1;\nx?.N;",
    "const N = 1;",
    "const N = 1;\n{ const o = { N: 3 }; }",
    "const o = { a: 1, N: 2 };\nN;",
    "/* a\n b */ const N = 1;\nN;",
    "const 'N\nconst N = 2;\nN;",
    "call((N) => 1);\ncall(N)(x => x);",
    "if (a) {\n  for (const N of xs) { N; }\n}\nN;",
];

#[test]
fn generated_shapes_match_the_scan_for_every_name_and_nesting() -> Result<(), String> {
    let mut shadowed = 0usize;
    let mut total = 0usize;
    for template in SHAPES {
        for name in ["N", "cost", "café", "$v", "_u1"] {
            let shape = template.replace('N', name);
            for body in [
                format!("it('t', () => {{\n{shape}\n}});"),
                format!("it('t', () => {{\n  if (c) {{\n{shape}\n  }}\n  {name}(1);\n}});"),
                format!("{shape} {shape}"),
            ] {
                let index = BodyShadowIndex::build(&body);
                for candidate in names_for(&body).into_iter().chain([name.to_string()]) {
                    let Some(actual) = index.shadows(&body, &candidate) else {
                        continue;
                    };
                    let expected =
                        local_identifier_declared_in_test_body_scanned(&body, &candidate);
                    if actual != expected {
                        return Err(format!(
                            "{candidate:?} in {body:?}: scan {expected}, index {actual}"
                        ));
                    }
                    total += 1;
                    shadowed += usize::from(actual);
                }
            }
        }
    }
    if total < 5000 || shadowed < 500 || total - shadowed < 1000 {
        return Err(format!(
            "thin generated corpus: {total} checked, {shadowed} shadowed"
        ));
    }
    Ok(())
}

#[test]
fn every_shadow_shape_is_actually_exercised() -> Result<(), String> {
    // Anchors the corpus: each family answers `true` through the index, and
    // the matching look-alikes answer `false`.
    let cases: &[(usize, &str, bool)] = &[
        (0, "total", true),
        (1, "owner", true),
        (2, "late", true),
        (2, "hoisted", true),
        (3, "cost", true),
        (3, "w", true),
        (3, "tax", false),
        (4, "err", true),
        (4, "code", true),
        (5, "cost_not_here", false),
        (5, "price", true),
        (5, "qty", true),
        (5, "opt", true),
        (5, "star", true),
        (5, "other", false),
        (5, "tref", false),
        (5, "notParam", true),
        (6, "item", true),
        (6, "i", true),
        (6, "key", true),
        (6, "chunk", true),
        (6, "limit", false),
        (6, "information", false),
        (7, "hidden", false),
        (7, "fake", false),
        (7, "nope", false),
        (8, "café", true),
        (8, "recovered", true),
        (9, "information", false),
        (9, "price", false),
        (9, "forwardX", false),
        (10, "price", true),
    ];
    for (body_index, name, expected) in cases {
        let body = BODIES[*body_index];
        let index = BodyShadowIndex::build(body);
        let actual = index.shadows(body, name);
        if actual != Some(*expected) {
            return Err(format!(
                "body {body_index} name {name:?}: expected {expected}, got {actual:?}"
            ));
        }
        let scanned = local_identifier_declared_in_test_body_scanned(body, name);
        if scanned != *expected {
            return Err(format!(
                "body {body_index} name {name:?}: scan disagrees ({scanned}), fix the anchor"
            ));
        }
    }
    Ok(())
}

#[test]
fn names_outside_the_exact_domain_are_left_to_the_scan() -> Result<(), String> {
    let body = BODIES[0];
    let index = BodyShadowIndex::build(body);
    for name in ["", "a.b", "1abc", "tot al", "é"] {
        if index.shadows(body, name).is_some() {
            return Err(format!("{name:?} must fall back to the scan"));
        }
    }
    Ok(())
}

#[test]
fn a_run_builds_one_index_per_body_and_drops_it_with_the_run() -> Result<(), String> {
    let body_a = BODIES[0];
    let body_b = BODIES[6];
    let start = index_builds();
    {
        let _run = DirectoryModuleCacheScope::open();
        for name in names_for(body_a) {
            let _ = local_identifier_declared_in_test_body(body_a, &name);
        }
        if index_builds() != start + 1 {
            return Err(format!(
                "many names of one body must build one index; builds {start} -> {}",
                index_builds()
            ));
        }
        for name in names_for(body_b) {
            let _ = local_identifier_declared_in_test_body(body_b, &name);
        }
        for name in names_for(body_a) {
            let _ = local_identifier_declared_in_test_body(body_a, &name);
        }
        if index_builds() != start + 2 {
            return Err(format!(
                "two bodies build two indexes; builds {start} -> {}",
                index_builds()
            ));
        }
    }
    // Outside a run nothing is cached or built; the exact scan answers.
    let outside = index_builds();
    let _ = local_identifier_declared_in_test_body(body_a, "total");
    if index_builds() != outside {
        return Err("an out-of-run call must not build an index".to_string());
    }
    // A new run starts empty: the same body builds again.
    {
        let _run = DirectoryModuleCacheScope::open();
        let _ = local_identifier_declared_in_test_body(body_a, "total");
    }
    if index_builds() != outside + 1 {
        return Err(format!(
            "a fresh run must rebuild; builds {outside} -> {}",
            index_builds()
        ));
    }
    Ok(())
}

#[test]
fn public_answers_match_the_scan_inside_and_outside_a_run() -> Result<(), String> {
    for body in BODIES {
        for name in names_for(body) {
            let expected = local_identifier_declared_in_test_body_scanned(body, &name);
            let outside = local_identifier_declared_in_test_body(body, &name);
            let inside = {
                let _run = DirectoryModuleCacheScope::open();
                local_identifier_declared_in_test_body(body, &name)
            };
            if outside != expected || inside != expected {
                return Err(format!(
                    "{name:?} in {body:?}: scan {expected}, outside {outside}, inside {inside}"
                ));
            }
        }
    }
    Ok(())
}
