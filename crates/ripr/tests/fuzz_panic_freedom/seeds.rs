#[cfg(feature = "lang-python")]
use super::{PYTHON_EDITED, PYTHON_SEED};
use super::{RUST_EDITED, RUST_SEED};
#[cfg(feature = "lang-typescript")]
use super::{TS_EDITED, TS_SEED};

// Keep the default mutation budget per language fixed while sampling all
// three behaviors. The unmutated canary below exercises every seed even if
// random selection misses one in a short default run.
pub(super) const RUST_CASES: [(&str, &str); 3] = [
    (RUST_SEED, RUST_EDITED),
    (
        r#"pub fn validate(n: u64) -> Result<u64, &'static str> { if n == 0 { Err("zero") } else { Ok(n) } }
#[test]
fn error_path() { assert_eq!(validate(0), Err("zero")); }
"#,
        r#"pub fn validate(n: u64) -> Result<u64, &'static str> { if n == 0 { Err("invalid amount") } else { Ok(n) } }
#[test]
fn error_path() { assert_eq!(validate(0), Err("zero")); }
"#,
    ),
    (
        r#"pub fn answer() -> u64 { 7 }
#[test]
fn return_value() { assert_eq!(answer(), 7); }
"#,
        r#"pub fn answer() -> u64 { 9 }
#[test]
fn return_value() { assert_eq!(answer(), 7); }
"#,
    ),
];

#[cfg(feature = "lang-python")]
pub(super) const PYTHON_CASES: [(&str, &str); 3] = [
    (PYTHON_SEED, PYTHON_EDITED),
    (
        r#"import pytest
def validate(n):
    if n == 0:
        raise ValueError("zero")
    return n
def test_error_path():
    with pytest.raises(ValueError, match="zero"):
        validate(0)
"#,
        r#"import pytest
def validate(n):
    if n == 0:
        raise ValueError("invalid amount")
    return n
def test_error_path():
    with pytest.raises(ValueError, match="zero"):
        validate(0)
"#,
    ),
    (
        r#"def answer():
    return 7
def test_return_value():
    assert answer() == 7
"#,
        r#"def answer():
    return 9
def test_return_value():
    assert answer() == 7
"#,
    ),
];

#[cfg(feature = "lang-typescript")]
pub(super) const TS_CASES: [(&str, &str); 3] = [
    (TS_SEED, TS_EDITED),
    (
        r#"import { expect, test } from 'vitest';
export function validate(n: number): number { if (n === 0) { throw new Error('zero'); } return n; }
test('error path', () => { expect(() => validate(0)).toThrow('zero'); });
"#,
        r#"import { expect, test } from 'vitest';
export function validate(n: number): number { if (n === 0) { throw new Error('invalid amount'); } return n; }
test('error path', () => { expect(() => validate(0)).toThrow('zero'); });
"#,
    ),
    (
        r#"import { expect, test } from 'vitest';
export function answer(): number { return 7; }
test('return value', () => { expect(answer()).toBe(7); });
"#,
        r#"import { expect, test } from 'vitest';
export function answer(): number { return 9; }
test('return value', () => { expect(answer()).toBe(7); });
"#,
    ),
];
