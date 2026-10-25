//! RIPR-SPEC-0243 rule 21: a decorated-method reach credits only the tests of
//! the decorated method. Each case runs the whole adapter on a temporary
//! workspace so the relation, the entry-only cap and the observation guard are
//! exercised together, not as helper output.

use super::*;

const AUDITED: &str = "export function audited(_target: object, key: string, descriptor: PropertyDescriptor): void {\n  const original = descriptor.value as (...args: number[]) => number;\n  descriptor.value = function (this: unknown, ...args: number[]): number {\n    calls.push(key);\n    return original.apply(this, args) + 1;\n  };\n}\n\nexport function register(target: object, args: number[]): number[] {\n  audited(target, 'add', { value: () => 1 });\n  return args;\n}\n\nexport class Ledger {\n  @audited\n  add(a: number, b: number): number {\n    return a + b;\n  }\n}\n\nconst calls: string[] = [];\n";
const RETURN_LINE: (usize, &str) = (5, "    return original.apply(this, args) + 1;");

const HEADER: &str =
    "import { it, expect } from 'vitest';\nimport { Ledger, register } from '../src/audited';\n";
const DECORATED_ASSERTED: &str =
    "it('adds', () => {\n  expect(new Ledger().add(2, 3)).toBe(6);\n});\n";
const DECORATED_UNASSERTED: &str = "it('runs', () => {\n  new Ledger().add(2, 3);\n});\n";
const ORDINARY_ARGS: &str = "it('registers', () => {\n  const args = [1, 2];\n  register({}, args);\n  expect(args).toEqual([1, 2]);\n});\n";

fn audited_class(label: &str, tests: &str) -> Result<ExposureClass, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/audited.ts"), AUDITED)?;
    ts_write_file(
        &root.join("tests/audited.test.ts"),
        &format!("{HEADER}{tests}"),
    )?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/audited.ts", &[RETURN_LINE])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("audited"))
        })
        .map(|finding| finding.class)
        .ok_or_else(|| format!("{label}: expected a finding for `audited`"))
}

fn expect_class(label: &str, tests: &str, expected: ExposureClass) -> Result<(), String> {
    let class = audited_class(label, tests)?;
    if class == expected {
        Ok(())
    } else {
        Err(format!("{label}: expected {expected:?}, got {class:?}"))
    }
}

#[test]
fn asserted_decorated_call_credits_the_return_edit() -> Result<(), String> {
    expect_class("decorated-only", DECORATED_ASSERTED, ExposureClass::Exposed)
}

#[test]
fn ordinary_caller_assertion_does_not_credit_the_return_edit() -> Result<(), String> {
    // The `args` assertion never sees the wrapper's return; the decorated
    // test asserts nothing. The entry-only cap must still hold.
    let class = audited_class(
        "mixed-unasserted",
        &format!("{ORDINARY_ARGS}{DECORATED_UNASSERTED}"),
    )?;
    if class == ExposureClass::Exposed {
        return Err("an ordinary entry assertion credited the decorated return".to_string());
    }
    Ok(())
}

#[test]
fn ordinary_caller_does_not_hide_a_real_decorated_assertion() -> Result<(), String> {
    expect_class(
        "mixed-asserted",
        &format!("{ORDINARY_ARGS}{DECORATED_ASSERTED}"),
        ExposureClass::Exposed,
    )
}

#[test]
fn shadowed_or_replaced_receiver_gets_no_credit() -> Result<(), String> {
    for (label, body) in [
        (
            "typed-shadow",
            "it('s', () => {\n  const l = new Ledger();\n  [{ add: () => 99 }].forEach((l: { add(): number }) => expect(l.add()).toBe(99));\n});\n",
        ),
        (
            "replaced",
            "it('r', () => {\n  const l = new Ledger();\n  const u = \"http://x\"; l.add = () => 99;\n  expect(l.add(2, 3)).toBe(99);\n});\n",
        ),
    ] {
        let class = audited_class(label, body)?;
        if class == ExposureClass::Exposed {
            return Err(format!("{label}: credited a bypassed decorator"));
        }
    }
    Ok(())
}
