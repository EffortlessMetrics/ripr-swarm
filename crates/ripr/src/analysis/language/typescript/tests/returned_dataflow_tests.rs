//! RIPR-SPEC-0243 rule 15: changed lines whose value reaches the owner's
//! return. Each positive pins the credit; the negatives reach the rule and
//! show the fallback it must keep.

use super::*;

/// Analyzes one edited line of `src/<module>.ts` against `test/<module>.test.ts`.
/// The edit replaces `old` with the line `new_source` carries at `line`.
fn finding_for_edit(
    label: &str,
    module: &str,
    new_source: &str,
    line: usize,
    old: &str,
    test_source: &str,
) -> Result<Finding, String> {
    let new = new_source
        .lines()
        .nth(line - 1)
        .ok_or_else(|| format!("{label}: no line {line}"))?
        .to_string();
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join(format!("src/{module}.ts")), new_source)?;
    ts_write_file(&root.join(format!("test/{module}.test.ts")), test_source)?;
    let changed = ChangedFile {
        path: PathBuf::from(format!("src/{module}.ts")),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: new,
            new_side_line: line,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: old.to_string(),
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
        .ok_or_else(|| format!("{label}: expected a finding on line {line}"))
}

fn vitest(imports: &str, body: &str) -> String {
    format!("import {{ expect, it }} from \"vitest\";\n{imports}\n\n{body}\n")
}

fn assert_credited(finding: &Finding, label: &str) {
    assert_eq!(finding.class, ExposureClass::Exposed, "{label}");
}

fn assert_not_credited(finding: &Finding, label: &str) {
    assert_ne!(finding.class, ExposureClass::Exposed, "{label}");
}

// ---------------------------------------------------------------- arrow body

const MATHX: &str = "export const double = (n: number): number => n * 3;\n\nexport const sum = (xs: readonly number[]): number => xs.reduce((a, b) => a + b, 1);\n";

fn mathx_test() -> String {
    vitest(
        "import { double, sum } from \"../src/mathx\";",
        "it(\"doubles\", () => {\n  expect(double(4)).toBe(8);\n});\n\nit(\"sums\", () => {\n  expect(sum([1, 2])).toBe(3);\n});",
    )
}

#[test]
fn declared_arrow_body_is_the_returned_value() -> Result<(), String> {
    let finding = finding_for_edit(
        "arrow-body",
        "mathx",
        MATHX,
        1,
        "export const double = (n: number): number => n * 2;",
        &mathx_test(),
    )?;
    assert_credited(&finding, "an exact assertion observes the arrow body");
    // A callback inside the body is a value, not a curried function.
    let finding = finding_for_edit(
        "arrow-reduce-seed",
        "mathx",
        MATHX,
        3,
        "export const sum = (xs: readonly number[]): number => xs.reduce((a, b) => a + b, 0);",
        &mathx_test(),
    )?;
    assert_credited(&finding, "the reduce seed reaches the exact assertion");
    Ok(())
}

#[test]
fn declared_arrow_body_keeps_its_fallbacks() -> Result<(), String> {
    // A weak oracle credits nothing.
    let weak = vitest(
        "import { double } from \"../src/mathx\";",
        "it(\"doubles\", () => {\n  expect(double(4)).toBeDefined();\n});",
    );
    let finding = finding_for_edit(
        "arrow-weak",
        "mathx",
        MATHX,
        1,
        "export const double = (n: number): number => n * 2;",
        &weak,
    )?;
    assert_not_credited(&finding, "a smoke assertion does not observe the value");
    // A curried arrow's body is another function.
    let curried = "export const scale = (a: number) => (b: number) => a * 3 * b;\n";
    let test = vitest(
        "import { scale } from \"../src/curried\";",
        "it(\"scales\", () => {\n  expect(scale(2)(3)).toBe(12);\n});",
    );
    let finding = finding_for_edit(
        "arrow-curried",
        "curried",
        curried,
        1,
        "export const scale = (a: number) => (b: number) => a * 2 * b;",
        &test,
    )?;
    assert_not_credited(&finding, "a curried body is not the returned value");
    // A relational body is a predicate boundary, not a plain value.
    let predicate = "export const big = (n: number): boolean => n > 3;\n";
    let test = vitest(
        "import { big } from \"../src/predicate\";",
        "it(\"is big\", () => {\n  expect(big(10)).toBe(true);\n});",
    );
    let finding = finding_for_edit(
        "arrow-predicate",
        "predicate",
        predicate,
        1,
        "export const big = (n: number): boolean => n > 2;",
        &test,
    )?;
    assert_not_credited(&finding, "an input off the boundary does not witness it");
    Ok(())
}

// ----------------------------------------------------------- compound assign

const ACCOUNT: &str = "export class Account {\n  constructor(public balance: number) {}\n\n  deposit(amount: number): number {\n    this.balance -= amount;\n    return this.balance;\n  }\n}\n";

fn account_test() -> String {
    vitest(
        "import { Account } from \"../src/account\";",
        "it(\"deposit returns the new balance\", () => {\n  expect(new Account(10).deposit(5)).toBe(15);\n});",
    )
}

#[test]
fn compound_assignment_to_the_returned_state_is_a_return_change() -> Result<(), String> {
    let finding = finding_for_edit(
        "state-field",
        "account",
        ACCOUNT,
        5,
        "    this.balance += amount;",
        &account_test(),
    )?;
    assert_credited(&finding, "the field is the returned expression");
    // A local accumulated in a loop, then returned.
    let total = "export function total(n: number): number {\n  let t = 0;\n  for (let i = 0; i < n; i++) {\n    t += i * 3;\n  }\n  return t;\n}\n";
    let test = vitest(
        "import { total } from \"../src/total\";",
        "it(\"totals\", () => {\n  expect(total(3)).toBe(6);\n});",
    );
    let finding = finding_for_edit("state-loop", "total", total, 4, "    t += i * 2;", &test)?;
    assert_credited(&finding, "a loop body updates the returned local");
    // The same statement inside one minified line: the changed span is the
    // statement, not the whole line.
    let minified = "export const total=(n:number)=>{let t=0;for(let i=0;i<n;i++){t+=i*3}return t};export const other=(n:number)=>n+1;\n";
    let old = "export const total=(n:number)=>{let t=0;for(let i=0;i<n;i++){t+=i*2}return t};export const other=(n:number)=>n+1;";
    let finding = finding_for_edit("state-minified", "total", minified, 1, old, &test)?;
    assert_credited(
        &finding,
        "a minified compound assignment is read by statement",
    );
    Ok(())
}

#[test]
fn compound_assignment_keeps_its_fallbacks() -> Result<(), String> {
    let refuse = |label: &str, source: &str, old: &str, line: usize| -> Result<(), String> {
        let test = vitest(
            "import { run } from \"../src/run\";",
            "it(\"runs\", () => {\n  expect(run(3)).toBe(6);\n});",
        );
        let finding = finding_for_edit(label, "run", source, line, old, &test)?;
        assert_not_credited(&finding, label);
        Ok(())
    };
    // The returned expression is not the assigned target.
    refuse(
        "state-other-return",
        "export function run(n: number): number {\n  let t = 0;\n  t += n * 3;\n  return t + 1;\n}\n",
        "  t += n * 2;",
        3,
    )?;
    // A branch may skip the assignment.
    refuse(
        "state-branch",
        "export function run(n: number): number {\n  let t = 0;\n  if (n > 0) {\n    t += n * 3;\n  }\n  return t;\n}\n",
        "    t += n * 2;",
        4,
    )?;
    // Another write replaces the value after the assignment.
    refuse(
        "state-overwritten",
        "export function run(n: number): number {\n  let t = 0;\n  t += n * 3;\n  t = 5;\n  return t;\n}\n",
        "  t += n * 2;",
        3,
    )?;
    // Two returns: the assignment may not be the one returned.
    refuse(
        "state-two-returns",
        "export function run(n: number): number {\n  let t = 0;\n  t += n * 3;\n  if (n > 9) {\n    return 0;\n  }\n  return t;\n}\n",
        "  t += n * 2;",
        3,
    )?;
    // The old statement wrote another target.
    refuse(
        "state-swapped-target",
        "export function run(n: number): number {\n  let t = 0;\n  t += n * 3;\n  return t;\n}\n",
        "  u += n * 2;",
        3,
    )?;
    // A nested function may run it anywhere.
    refuse(
        "state-nested-function",
        "export function run(n: number): number {\n  let t = 0;\n  const add = () => {\n    t += n * 3;\n  };\n  add();\n  return t;\n}\n",
        "    t += n * 2;",
        4,
    )?;
    Ok(())
}

// ------------------------------------------------------------- ternary arm

const LEVEL: &str = "export enum Level {\n  Low = 1,\n  High = 2,\n}\n\nexport function weight(level: Level): number {\n  return level === Level.High ? 11 : 1;\n}\n";

fn level_test(calls: &str) -> String {
    vitest("import { Level, weight } from \"../src/level\";", calls)
}

#[test]
fn an_edited_arm_is_credited_by_an_assertion_that_selects_it() -> Result<(), String> {
    let both = level_test(
        "it(\"high\", () => {\n  expect(weight(Level.High)).toBe(10);\n});\n\nit(\"low\", () => {\n  expect(weight(Level.Low)).toBe(1);\n});",
    );
    let finding = finding_for_edit(
        "arm-enum-true",
        "level",
        LEVEL,
        7,
        "  return level === Level.High ? 10 : 1;",
        &both,
    )?;
    assert_credited(&finding, "weight(Level.High) selects the edited true arm");
    // Numeric relational conditions evaluate on literal arguments.
    let price = "export function price(amount: number): number {\n  return amount > 100 ? 140 : amount;\n}\n";
    let test = vitest(
        "import { price } from \"../src/price\";",
        "it(\"discounts\", () => {\n  expect(price(150)).toBe(130);\n});",
    );
    let finding = finding_for_edit(
        "arm-number-true",
        "price",
        price,
        2,
        "  return amount > 100 ? 130 : amount;",
        &test,
    )?;
    assert_credited(&finding, "price(150) takes the edited true arm");
    // The false arm is selected by an argument that fails the condition.
    let price = "export function price(amount: number): number {\n  return amount > 100 ? amount - 10 : amount + 1;\n}\n";
    let test = vitest(
        "import { price } from \"../src/price\";",
        "it(\"keeps small\", () => {\n  expect(price(50)).toBe(51);\n});",
    );
    let finding = finding_for_edit(
        "arm-number-false",
        "price",
        price,
        2,
        "  return amount > 100 ? amount - 10 : amount;",
        &test,
    )?;
    assert_credited(&finding, "price(50) takes the edited false arm");
    Ok(())
}

#[test]
fn an_edited_arm_keeps_its_fallbacks() -> Result<(), String> {
    // Only the other arm is pinned: the edit passes on both versions.
    let low_only = level_test("it(\"low\", () => {\n  expect(weight(Level.Low)).toBe(1);\n});");
    let finding = finding_for_edit(
        "arm-other-arm",
        "level",
        LEVEL,
        7,
        "  return level === Level.High ? 10 : 1;",
        &low_only,
    )?;
    assert_not_credited(&finding, "weight(Level.Low) never runs the edited arm");
    // An argument that is not a literal selects nothing provable.
    let variable = level_test(
        "it(\"high\", () => {\n  const level = Level.High;\n  expect(weight(level)).toBe(10);\n});",
    );
    let finding = finding_for_edit(
        "arm-variable-argument",
        "level",
        LEVEL,
        7,
        "  return level === Level.High ? 10 : 1;",
        &variable,
    )?;
    assert_not_credited(&finding, "a variable argument is not evaluated");
    // A weak assertion on the selecting call proves nothing.
    let weak = level_test("it(\"high\", () => {\n  expect(weight(Level.High)).toBeDefined();\n});");
    let finding = finding_for_edit(
        "arm-weak",
        "level",
        LEVEL,
        7,
        "  return level === Level.High ? 10 : 1;",
        &weak,
    )?;
    assert_not_credited(&finding, "a smoke assertion does not observe the arm");
    // A changed condition is a boundary change, not an arm change.
    let moved = "export function price(amount: number): number {\n  return amount > 100 ? 130 : amount;\n}\n";
    let old_moved = "  return amount >= 100 ? 120 : amount;";
    let test = vitest(
        "import { price } from \"../src/price\";",
        "it(\"discounts\", () => {\n  expect(price(150)).toBe(130);\n});",
    );
    let finding = finding_for_edit("arm-condition-moved", "price", moved, 2, old_moved, &test)?;
    assert_not_credited(
        &finding,
        "a moved condition is a boundary change, not an arm change",
    );
    // Different member paths may share a value, so they decide nothing.
    let finding = finding_for_edit(
        "arm-other-member-false",
        "level",
        "export enum Level {\n  Low = 1,\n  High = 1,\n}\n\nexport function weight(level: Level): number {\n  return level === Level.High ? 10 : 2;\n}\n",
        7,
        "  return level === Level.High ? 10 : 1;",
        &level_test("it(\"low\", () => {\n  expect(weight(Level.Low)).toBe(1);\n});"),
    )?;
    assert_not_credited(
        &finding,
        "an aliased enum member is not provably the other arm",
    );
    Ok(())
}

// -------------------------------------------------------------- loop bound

const RANGE: &str = "export function* range(n: number): Generator<number> {\n  for (let i = 0; i <= n; i++) {\n    yield i;\n  }\n}\n";

fn range_test(expression: &str, expected: &str) -> String {
    vitest(
        "import { range } from \"../src/range\";",
        &format!(
            "it(\"drains the range\", () => {{\n  expect({expression}).toEqual({expected});\n}});"
        ),
    )
}

#[test]
fn a_generator_loop_bound_is_credited_by_the_drained_sequence() -> Result<(), String> {
    let old = "  for (let i = 0; i < n; i++) {";
    let finding = finding_for_edit(
        "loop-bound-spread",
        "range",
        RANGE,
        2,
        old,
        &range_test("[...range(3)]", "[0, 1, 2]"),
    )?;
    assert_credited(&finding, "[...range(3)] observes the extra element");
    let finding = finding_for_edit(
        "loop-bound-array-from",
        "range",
        RANGE,
        2,
        old,
        &range_test("Array.from(range(3))", "[0, 1, 2]"),
    )?;
    assert_credited(&finding, "Array.from(range(3)) observes it too");
    // Counting down.
    let down = "export function* countdown(n: number): Generator<number> {\n  for (let i = 5; i >= n; i--) {\n    yield i;\n  }\n}\n";
    let test = vitest(
        "import { countdown } from \"../src/down\";",
        "it(\"counts down\", () => {\n  expect([...countdown(3)]).toEqual([5, 4]);\n});",
    );
    let finding = finding_for_edit(
        "loop-bound-down",
        "down",
        down,
        2,
        "  for (let i = 5; i > n; i--) {",
        &test,
    )?;
    assert_credited(&finding, "a descending loop reaches its bound too");
    Ok(())
}

#[test]
fn a_generator_loop_bound_keeps_its_fallbacks() -> Result<(), String> {
    let old = "  for (let i = 0; i < n; i++) {";
    // Only the first element is observed.
    let finding = finding_for_edit(
        "loop-bound-first-only",
        "range",
        RANGE,
        2,
        old,
        &range_test("range(3).next().value", "0"),
    )?;
    assert_not_credited(&finding, "the first element never sees the last iteration");
    // The counter never reaches a negative bound.
    let finding = finding_for_edit(
        "loop-bound-unreached",
        "range",
        RANGE,
        2,
        old,
        &range_test("[...range(-1)]", "[]"),
    )?;
    assert_not_credited(&finding, "range(-1) runs no iteration on either version");
    // Not a generator: the extra iteration need not change the result.
    let sum = "export function sum(n: number): number {\n  let t = 0;\n  for (let i = 0; i <= n; i++) {\n    t += 1;\n  }\n  return t;\n}\n";
    let test = vitest(
        "import { sum } from \"../src/sum\";",
        "it(\"sums\", () => {\n  expect(sum(3)).toEqual(4);\n});",
    );
    let finding = finding_for_edit(
        "loop-bound-not-generator",
        "sum",
        sum,
        3,
        "  for (let i = 0; i < n; i++) {",
        &test,
    )?;
    assert_not_credited(
        &finding,
        "only a generator's yield proves the extra element",
    );
    // A conditional yield may skip the extra element.
    let conditional = "export function* evens(n: number): Generator<number> {\n  for (let i = 0; i <= n; i++) {\n    if (i % 2 === 0) {\n      yield i;\n    }\n  }\n}\n";
    let test = vitest(
        "import { evens } from \"../src/evens\";",
        "it(\"drains\", () => {\n  expect([...evens(4)]).toEqual([0, 2, 4]);\n});",
    );
    let finding = finding_for_edit(
        "loop-bound-conditional-yield",
        "evens",
        conditional,
        2,
        "  for (let i = 0; i < n; i++) {",
        &test,
    )?;
    assert_not_credited(&finding, "a yield behind a branch is not every iteration");
    Ok(())
}

#[test]
fn declared_arrow_body_reads_as_a_return_value_unless_it_compares() {
    for line in [
        "export const double = (n: number): number => n * 3;",
        "const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 1);",
        "export const label = async (n: number) => `n=${n}`;",
    ] {
        assert_eq!(
            classify_probe_shape(line),
            (ProbeFamily::ReturnValue, DeltaKind::Value),
            "{line}"
        );
    }
    for line in [
        "export const big = (n: number): boolean => n > 3;",
        "export const scale = (a: number) => (b: number) => a * b;",
        "export const run = (n: number) => { return n * 3; };",
    ] {
        assert_ne!(
            classify_probe_shape(line).0,
            ProbeFamily::ReturnValue,
            "{line}"
        );
    }
}

#[test]
fn a_generator_loop_that_may_not_run_or_yield_is_not_credited() -> Result<(), String> {
    let old = "  for (let i = 0; i < n; i++) {";
    let refuse = |label: &str, source: &str, test: &str| -> Result<(), String> {
        let finding = finding_for_edit(label, "gen", source, 2, old, test)?;
        assert_not_credited(&finding, label);
        Ok(())
    };
    let test = |call: &str, expected: &str| {
        vitest(
            "import { gen } from \"../src/gen\";",
            &format!("it(\"drains\", () => {{\n  expect([...{call}]).toEqual({expected});\n}});"),
        )
    };
    // The loop sits inside a branch the test never enters.
    let branch = "export function* gen(n: number, on: boolean): Generator<number> {\n  if (on) {\n    for (let i = 0; i <= n; i++) {\n      yield i;\n    }\n  }\n}\n";
    let finding = finding_for_edit(
        "loop-in-branch",
        "gen",
        branch,
        3,
        "    for (let i = 0; i < n; i++) {",
        &test("gen(3, false)", "[]"),
    )?;
    assert_not_credited(&finding, "the branch never runs the loop");
    // An early return can leave before the loop.
    let early = "export function* gen(n: number): Generator<number> {\n  if (n > 2) return;\n  for (let i = 0; i <= n; i++) {\n    yield i;\n  }\n}\n";
    let finding = finding_for_edit(
        "loop-early-return",
        "gen",
        early,
        3,
        old,
        &test("gen(3)", "[]"),
    )?;
    assert_not_credited(&finding, "an early return precedes the loop");
    // A braceless guarded yield may skip the extra element.
    let guarded = "export function* gen(n: number): Generator<number> {\n  for (let i = 0; i <= n; i++) {\n    if (i % 2 === 0) yield i;\n  }\n}\n";
    refuse(
        "loop-braceless-yield",
        guarded,
        &test("gen(4)", "[0, 2, 4]"),
    )?;
    Ok(())
}

#[test]
fn a_local_arrow_nested_in_the_owner_is_not_its_return() -> Result<(), String> {
    let source = "export function outer(n: number): number {\n  const inner = (k: number): number => k * 3;\n  return n + 1;\n}\n";
    let test = vitest(
        "import { outer } from \"../src/outer\";",
        "it(\"adds\", () => {\n  expect(outer(1)).toBe(2);\n});",
    );
    let finding = finding_for_edit(
        "nested-arrow",
        "outer",
        source,
        2,
        "  const inner = (k: number): number => k * 2;",
        &test,
    )?;
    assert_not_credited(&finding, "the nested arrow binds another name");
    Ok(())
}

#[test]
fn the_remaining_rule_15_guards_each_keep_their_fallback() -> Result<(), String> {
    let gen_test = |call: &str, expected: &str| {
        vitest(
            "import { gen } from \"../src/gen\";",
            &format!("it(\"drains\", () => {{\n  expect([...{call}]).toEqual({expected});\n}});"),
        )
    };
    let header_old = "  for (let i = 0; i < n; i++) {";
    // A `break`, a `yield*`, or a write to the counter in the loop body.
    for (label, body) in [
        ("loop-break", "    yield i;\n    if (i > 1) break;"),
        ("loop-delegate", "    yield* [i];"),
        ("loop-counter-write", "    yield i;\n    i += 1;"),
    ] {
        let source = format!(
            "export function* gen(n: number): Generator<number> {{\n  for (let i = 0; i <= n; i++) {{\n{body}\n  }}\n}}\n"
        );
        let finding = finding_for_edit(
            label,
            "gen",
            &source,
            2,
            header_old,
            &gen_test("gen(3)", "[0, 1, 2, 3]"),
        )?;
        assert_not_credited(&finding, label);
    }
    // Dropping the final iteration (`<=` to `<`) is the same observable edit.
    let source = "export function* gen(n: number): Generator<number> {\n  for (let i = 0; i < n; i++) {\n    yield i;\n  }\n}\n";
    let finding = finding_for_edit(
        "loop-dropped",
        "gen",
        source,
        2,
        "  for (let i = 0; i <= n; i++) {",
        &gen_test("gen(3)", "[0, 1, 2, 3]"),
    )?;
    assert_credited(
        &finding,
        "a dropped last iteration shows in the drained sequence",
    );
    // A literal bound needs no argument; a descending call that cannot reach it is refused.
    let literal = "export function* gen(): Generator<number> {\n  for (let i = 0; i <= 2; i++) {\n    yield i;\n  }\n}\n";
    let finding = finding_for_edit(
        "loop-literal-bound",
        "gen",
        literal,
        2,
        "  for (let i = 0; i < 2; i++) {",
        &gen_test("gen()", "[0, 1]"),
    )?;
    assert_credited(&finding, "a literal bound is reached by every call");
    let down = "export function* gen(n: number): Generator<number> {\n  for (let i = 5; i >= n; i--) {\n    yield i;\n  }\n}\n";
    let finding = finding_for_edit(
        "loop-down-unreached",
        "gen",
        down,
        2,
        "  for (let i = 5; i > n; i--) {",
        &gen_test("gen(9)", "[]"),
    )?;
    assert_not_credited(&finding, "5 never counts down to 9");

    // Arm rule: text, boolean and null operands; a written parameter; a self-comparing assertion.
    let arm_test = |call: &str, expected: &str| {
        vitest(
            "import { pick } from \"../src/pick\";",
            &format!("it(\"picks\", () => {{\n  expect({call}).toBe({expected});\n}});"),
        )
    };
    for (label, param_type, condition, argument) in [
        ("arm-text", "string", "kind === \"gold\"", "\"gold\""),
        ("arm-boolean", "boolean", "on === true", "true"),
        ("arm-null", "string | null", "value === null", "null"),
    ] {
        let name = condition.split(' ').next().unwrap_or("kind");
        let source = format!(
            "export function pick({name}: {param_type}): number {{\n  return {condition} ? 11 : 2;\n}}\n"
        );
        let old = format!("  return {condition} ? 10 : 2;");
        let finding = finding_for_edit(
            label,
            "pick",
            &source,
            2,
            &old,
            &arm_test(&format!("pick({argument})"), "10"),
        )?;
        assert_credited(&finding, label);
    }
    let written =
        "export function pick(n: number): number {\n  n = n + 1;\n  return n > 5 ? 11 : 2;\n}\n";
    let finding = finding_for_edit(
        "arm-written-parameter",
        "pick",
        written,
        3,
        "  return n > 5 ? 10 : 2;",
        &arm_test("pick(9)", "10"),
    )?;
    assert_not_credited(&finding, "a rewritten parameter is not the argument");
    let source = "export function pick(n: number): number {\n  return n > 5 ? 11 : 2;\n}\n";
    let finding = finding_for_edit(
        "arm-self-comparing",
        "pick",
        source,
        2,
        "  return n > 5 ? 10 : 2;",
        &arm_test("pick(9)", "pick(9)"),
    )?;
    assert_not_credited(
        &finding,
        "an assertion comparing the call with itself proves nothing",
    );
    Ok(())
}

// ------------------------------------------------- review of record: #9493

#[test]
fn an_edited_arm_is_credited_through_an_import_alias() -> Result<(), String> {
    let test = vitest(
        "import { Level, weight as w } from \"../src/level\";",
        "it(\"high\", () => {\n  expect(w(Level.High)).toBe(10);\n});",
    );
    let finding = finding_for_edit(
        "arm-alias",
        "level",
        LEVEL,
        7,
        "  return level === Level.High ? 10 : 1;",
        &test,
    )?;
    assert_credited(&finding, "a renamed import still calls the owner");
    Ok(())
}

#[test]
fn an_edited_arm_refuses_wrappers_dead_expectations_and_shadowed_roots() -> Result<(), String> {
    let price = "export function price(amount: number): number {\n  return amount > 100 ? 140 : amount;\n}\n";
    let old = "  return amount > 100 ? 130 : amount;";
    let wrapped = vitest(
        "import { price } from \"../src/price\";",
        "it(\"is positive\", () => {\n  expect(price(150) > 0).toBe(true);\n});",
    );
    let finding = finding_for_edit("arm-wrapper", "price", price, 2, old, &wrapped)?;
    assert_not_credited(&finding, "a comparison on the call hides the changed value");
    let dead = vitest(
        "import { price } from \"../src/price\";",
        "it(\"never true\", () => {\n  expect(price(150)).toBe(999);\n});",
    );
    let finding = finding_for_edit("arm-dead", "price", price, 2, old, &dead)?;
    assert_not_credited(&finding, "999 equals neither arm value");
    let shadowed = vitest(
        "import { weight } from \"../src/level\";",
        "it(\"high\", () => {\n  const Level = { High: 1 };\n  expect(weight(Level.High)).toBe(1);\n});",
    );
    let finding = finding_for_edit(
        "arm-shadowed-root",
        "level",
        LEVEL,
        7,
        "  return level === Level.High ? 10 : 1;",
        &shadowed,
    )?;
    assert_not_credited(&finding, "a test-local Level is not the owner's enum");
    let foreign = vitest(
        "import { weight } from \"../src/level\";\nimport { Level } from \"../src/other-level\";",
        "it(\"high\", () => {\n  expect(weight(Level.High)).toBe(10);\n});",
    );
    let finding = finding_for_edit(
        "arm-foreign-root",
        "level",
        LEVEL,
        7,
        "  return level === Level.High ? 10 : 1;",
        &foreign,
    )?;
    assert_not_credited(
        &finding,
        "a Level imported from another module is not the owner's enum",
    );
    Ok(())
}

#[test]
fn a_loop_bound_is_credited_through_an_import_alias() -> Result<(), String> {
    let source = "export function* range(n: number): Generator<number> {\n  for (let i = 0; i <= n; i++) {\n    yield i;\n  }\n}\n";
    let test = vitest(
        "import { range as r } from \"../src/range\";",
        "it(\"counts\", () => {\n  expect([...r(3)]).toEqual([0, 1, 2, 3]);\n});",
    );
    let finding = finding_for_edit(
        "loop-alias",
        "range",
        source,
        2,
        "  for (let i = 0; i < n; i++) {",
        &test,
    )?;
    assert_credited(&finding, "a renamed generator import is the owner");
    Ok(())
}

#[test]
fn a_method_call_before_the_return_hides_the_changed_field() -> Result<(), String> {
    let source = "export class Account {\n  constructor(public balance: number) {}\n\n  reset(): void {\n    this.balance = 0;\n  }\n\n  deposit(amount: number): number {\n    this.balance += amount;\n    this.reset();\n    return this.balance;\n  }\n}\n";
    let test = vitest(
        "import { Account } from \"../src/account\";",
        "it(\"resets\", () => {\n  expect(new Account(10).deposit(5)).toBe(0);\n});",
    );
    let finding = finding_for_edit(
        "state-intervening-call",
        "account",
        source,
        9,
        "    this.balance -= amount;",
        &test,
    )?;
    assert_not_credited(&finding, "this.reset() can rewrite the field");
    Ok(())
}
