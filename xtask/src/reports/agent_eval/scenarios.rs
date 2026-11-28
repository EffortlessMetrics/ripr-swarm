//! Answer-keyed scenarios for `cargo xtask agent-eval`.
//!
//! Each scenario is a tiny project with a committed base, a committed head
//! (the intended behavior change to one source file), the tests the
//! repository already had, and two test files an agent might add: a `weak`
//! one that runs the changed code without telling the old and new behavior
//! apart, and a `reference` one that does. The answer key names, per gap, the
//! owning function, the changed line and the facts a test must pin. Keys are
//! hand-written facts about the scenario, not ripr output, so a route that
//! drops the fact is visible.
//!
//! Scenarios cover Rust, Python (pytest) and TypeScript (`node:test`). The
//! `py_*`, `ts_*` and `rs_*` ones keep the layout and code shape of the
//! open-source project their summary names (nested modules, package markers,
//! re-exports, several source files) instead of a single `src/lib.rs`. Their
//! code is written for the scenario, not copied from the project.
//!
//! Keep scenarios small and realistic. Changing a scenario changes the
//! baseline: rerun with `--write-baseline` in the same change.

/// The project kind a scenario materializes as. It decides the manifest, the
/// test file layout and the test command the key checks and trials run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Language {
    /// A Cargo package; tests under `tests/`, run with `cargo test`.
    Rust,
    /// A `pyproject.toml` package with `src/` on the pytest path; tests
    /// under `tests/test_*.py`, run with `python3 -m pytest`.
    Python,
    /// An ES module package; tests under `test/*.test.ts`, run with Node's
    /// built-in runner (`node --experimental-strip-types --test`).
    TypeScript,
}

impl Language {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Python => "python",
            Language::TypeScript => "typescript",
        }
    }

    /// The manifest file and its text for a package called `name`.
    pub(crate) fn manifest(self, name: &str) -> (&'static str, String) {
        match self {
            Language::Rust => (
                "Cargo.toml",
                format!(
                    "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n"
                ),
            ),
            Language::Python => (
                "pyproject.toml",
                format!(
                    "[project]\nname = \"{name}\"\nversion = \"0.1.0\"\nrequires-python = \">=3.10\"\n\n[tool.pytest.ini_options]\npythonpath = [\"src\"]\n"
                ),
            ),
            Language::TypeScript => (
                "package.json",
                format!(
                    "{{\n  \"name\": \"{name}\",\n  \"version\": \"0.1.0\",\n  \"type\": \"module\",\n  \"scripts\": {{ \"test\": \"node --experimental-strip-types --test\" }}\n}}\n"
                ),
            ),
        }
    }

    pub(crate) fn gitignore(self) -> &'static str {
        match self {
            Language::Rust => "/target/\n",
            Language::Python => "__pycache__/\n.pytest_cache/\n",
            Language::TypeScript => "node_modules/\n",
        }
    }

    /// Where a test file called `stem` lives (`existing`, `eval_weak`,
    /// `eval_reference`).
    pub(crate) fn test_path(self, stem: &str) -> String {
        match self {
            Language::Rust => format!("tests/{stem}.rs"),
            Language::Python => format!("tests/test_{stem}.py"),
            Language::TypeScript => format!("test/{stem}.test.ts"),
        }
    }

    /// How the owner is spelled where it is defined, for the key sanity test.
    #[cfg(test)]
    pub(crate) fn defines(self, owner: &str) -> String {
        match self {
            Language::Rust => format!("fn {owner}("),
            Language::Python => format!("def {owner}("),
            Language::TypeScript => format!("function {owner}("),
        }
    }
}

pub(crate) struct GapKey {
    /// Function that owns the changed behavior.
    pub(crate) owner: &'static str,
    /// 1-based changed line in the scenario's `source_path` at head.
    pub(crate) line: u32,
    /// Each entry is one fact; any of its spellings counts as present.
    pub(crate) facts: &'static [&'static [&'static str]],
}

pub(crate) struct Scenario {
    pub(crate) id: &'static str,
    pub(crate) summary: &'static str,
    /// Package name written into the manifest.
    pub(crate) crate_name: &'static str,
    pub(crate) language: Language,
    /// The one source file the change touches, relative to the root.
    pub(crate) source_path: &'static str,
    /// Unchanged files committed at base besides the manifest (module
    /// declarations, package markers, sibling helpers), as `(path, text)`.
    pub(crate) support_files: &'static [(&'static str, &'static str)],
    /// `source_path` at base.
    pub(crate) base_lib: &'static str,
    /// `source_path` at head.
    pub(crate) head_lib: &'static str,
    /// Existing tests (`Language::test_path("existing")`), committed at base.
    pub(crate) existing_tests: &'static str,
    /// A test that runs the change but does not discriminate it.
    pub(crate) weak_test: Option<&'static str>,
    /// A test that passes at head and fails at base.
    pub(crate) reference_test: Option<&'static str>,
    /// Gaps a useful route must surface. Every other reported gap is noise;
    /// an empty list is a control scenario where the useful answer is silence.
    pub(crate) gaps: &'static [GapKey],
}

pub(crate) const SCENARIOS: &[Scenario] = &[
    Scenario {
        id: "boundary_threshold",
        summary: "`>` becomes `>=` on a named threshold; tests use inputs far from it",
        crate_name: "shop",
        language: Language::Rust,
        source_path: "src/lib.rs",
        support_files: &[],
        base_lib: "/// Shipping is free for orders of at least this many cents.
pub const FREE_SHIPPING_CENTS: u64 = 5_000;

pub fn shipping_cents(order_cents: u64) -> u64 {
    if order_cents > FREE_SHIPPING_CENTS { 0 } else { 499 }
}
",
        head_lib: "/// Shipping is free for orders of at least this many cents.
pub const FREE_SHIPPING_CENTS: u64 = 5_000;

pub fn shipping_cents(order_cents: u64) -> u64 {
    if order_cents >= FREE_SHIPPING_CENTS { 0 } else { 499 }
}
",
        existing_tests: "use shop::*;

#[test]
fn big_orders_ship_free() {
    assert_eq!(shipping_cents(10_000), 0);
}

#[test]
fn small_orders_pay() {
    assert!(shipping_cents(100) > 0);
}
",
        weak_test: Some(
            "use shop::*;

#[test]
fn eval_weak() {
    assert!(shipping_cents(6_000) == 0);
}
",
        ),
        reference_test: Some(
            "use shop::*;

#[test]
fn eval_reference() {
    assert_eq!(shipping_cents(5_000), 0);
    assert_eq!(shipping_cents(4_999), 499);
}
",
        ),
        gaps: &[GapKey {
            owner: "shipping_cents",
            line: 5,
            facts: &[&["FREE_SHIPPING_CENTS", "5_000", "5000"]],
        }],
    },
    Scenario {
        id: "error_variant",
        summary: "a too-long input now returns a new error variant; tests only check is_err",
        crate_name: "names",
        language: Language::Rust,
        source_path: "src/lib.rs",
        support_files: &[],
        base_lib: "#[derive(Debug, PartialEq)]
pub enum NameError {
    Empty,
    TooLong,
}

pub fn parse_name(text: &str) -> Result<String, NameError> {
    if text.is_empty() {
        return Err(NameError::Empty);
    }
    if text.len() > 8 {
        return Err(NameError::Empty);
    }
    Ok(text.to_string())
}
",
        head_lib: "#[derive(Debug, PartialEq)]
pub enum NameError {
    Empty,
    TooLong,
}

pub fn parse_name(text: &str) -> Result<String, NameError> {
    if text.is_empty() {
        return Err(NameError::Empty);
    }
    if text.len() > 8 {
        return Err(NameError::TooLong);
    }
    Ok(text.to_string())
}
",
        existing_tests: "use names::*;

#[test]
fn rejects_long_names() {
    assert!(parse_name(\"abcdefghij\").is_err());
}

#[test]
fn accepts_short_names() {
    assert_eq!(parse_name(\"ada\"), Ok(\"ada\".to_string()));
}
",
        weak_test: Some(
            "use names::*;

#[test]
fn eval_weak() {
    assert!(matches!(parse_name(\"abcdefghijk\"), Err(_)));
}
",
        ),
        reference_test: Some(
            "use names::*;

#[test]
fn eval_reference() {
    assert_eq!(parse_name(\"abcdefghij\"), Err(NameError::TooLong));
}
",
        ),
        gaps: &[GapKey {
            owner: "parse_name",
            line: 12,
            facts: &[&["TooLong"]],
        }],
    },
    Scenario {
        id: "changed_constant_return",
        summary: "a default timeout changes value; the only test checks it is positive",
        crate_name: "netcfg",
        language: Language::Rust,
        source_path: "src/lib.rs",
        support_files: &[],
        base_lib: "pub fn default_timeout_ms() -> u64 {
    1200
}

pub fn retry_delay_ms(attempt: u32) -> u64 {
    100 * u64::from(attempt)
}
",
        head_lib: "pub fn default_timeout_ms() -> u64 {
    1500
}

pub fn retry_delay_ms(attempt: u32) -> u64 {
    100 * u64::from(attempt)
}
",
        existing_tests: "use netcfg::*;

#[test]
fn timeout_is_positive() {
    assert!(default_timeout_ms() > 0);
}

#[test]
fn delay_grows() {
    assert_eq!(retry_delay_ms(3), 300);
}
",
        weak_test: Some(
            "use netcfg::*;

#[test]
fn eval_weak() {
    assert!(default_timeout_ms() >= 1000);
}
",
        ),
        reference_test: Some(
            "use netcfg::*;

#[test]
fn eval_reference() {
    assert_eq!(default_timeout_ms(), 1500);
}
",
        ),
        gaps: &[GapKey {
            owner: "default_timeout_ms",
            line: 2,
            facts: &[&["1500"]],
        }],
    },
    Scenario {
        id: "struct_field",
        summary: "a constructor gains a long-flag spelling; tests assert another field",
        crate_name: "cliopts",
        language: Language::Rust,
        source_path: "src/lib.rs",
        support_files: &[],
        base_lib: "#[derive(Debug, PartialEq)]
pub struct Options {
    pub retries: u32,
    pub verbose: bool,
}

pub fn options_from_flag(flag: &str) -> Options {
    Options {
        retries: 3,
        verbose: flag == \"-v\",
    }
}
",
        head_lib: "#[derive(Debug, PartialEq)]
pub struct Options {
    pub retries: u32,
    pub verbose: bool,
}

pub fn options_from_flag(flag: &str) -> Options {
    Options {
        retries: 3,
        verbose: flag == \"-v\" || flag == \"--verbose\",
    }
}
",
        existing_tests: "use cliopts::*;

#[test]
fn keeps_default_retries() {
    assert_eq!(options_from_flag(\"--verbose\").retries, 3);
}
",
        weak_test: Some(
            "use cliopts::*;

#[test]
fn eval_weak() {
    assert_eq!(options_from_flag(\"-v\").retries, 3);
}
",
        ),
        reference_test: Some(
            "use cliopts::*;

#[test]
fn eval_reference() {
    assert!(options_from_flag(\"--verbose\").verbose);
    assert!(!options_from_flag(\"-q\").verbose);
}
",
        ),
        gaps: &[GapKey {
            owner: "options_from_flag",
            line: 10,
            facts: &[&["--verbose"]],
        }],
    },
    Scenario {
        id: "tested_control",
        summary: "a boundary change whose existing tests already pin below, equal and above",
        crate_name: "ages",
        language: Language::Rust,
        source_path: "src/lib.rs",
        support_files: &[],
        base_lib: "pub fn is_adult(age: u32) -> bool {
    age > 18
}
",
        head_lib: "pub fn is_adult(age: u32) -> bool {
    age >= 18
}
",
        existing_tests: "use ages::*;

#[test]
fn adult_boundary() {
    assert!(!is_adult(17));
    assert!(is_adult(18));
    assert!(is_adult(19));
}
",
        weak_test: None,
        reference_test: None,
        gaps: &[],
    },
    Scenario {
        id: "mixed_three_changes",
        summary: "three changed functions: two gaps and one change the tests already pin",
        crate_name: "billing",
        language: Language::Rust,
        source_path: "src/lib.rs",
        support_files: &[],
        base_lib: "pub fn discount_percent(total: u32) -> u32 {
    if total > 100 { 10 } else { 0 }
}

pub fn late_fee(days: u32) -> u32 {
    days * 2
}

pub fn tier(points: u32) -> &'static str {
    if points > 500 { \"gold\" } else { \"basic\" }
}
",
        head_lib: "pub fn discount_percent(total: u32) -> u32 {
    if total >= 100 { 10 } else { 0 }
}

pub fn late_fee(days: u32) -> u32 {
    days * 3
}

pub fn tier(points: u32) -> &'static str {
    if points >= 500 { \"gold\" } else { \"basic\" }
}
",
        existing_tests: "use billing::*;

#[test]
fn big_totals_get_a_discount() {
    assert_eq!(discount_percent(500), 10);
}

#[test]
fn late_fee_is_charged() {
    assert!(late_fee(4) > 0);
}

#[test]
fn tier_boundary() {
    assert_eq!(tier(499), \"basic\");
    assert_eq!(tier(500), \"gold\");
    assert_eq!(tier(501), \"gold\");
}
",
        weak_test: Some(
            "use billing::*;

#[test]
fn eval_weak() {
    assert!(discount_percent(200) > 0);
    assert!(late_fee(1) >= 2);
}
",
        ),
        reference_test: Some(
            "use billing::*;

#[test]
fn eval_reference() {
    assert_eq!(discount_percent(100), 10);
    assert_eq!(discount_percent(99), 0);
    assert_eq!(late_fee(4), 12);
}
",
        ),
        gaps: &[
            GapKey {
                owner: "discount_percent",
                line: 2,
                facts: &[&["== 100", ">= 100"]],
            },
            GapKey {
                owner: "late_fee",
                line: 6,
                facts: &[&["days * 3", "* 3"]],
            },
        ],
    },
    Scenario {
        id: "match_arm_value",
        summary: "one match arm's value changes; tests only cover the other arms",
        crate_name: "sizes",
        language: Language::Rust,
        source_path: "src/lib.rs",
        support_files: &[],
        base_lib: "pub enum Size {
    Small,
    Medium,
    Large,
}

pub fn price_cents(size: Size) -> u32 {
    match size {
        Size::Small => 300,
        Size::Medium => 400,
        Size::Large => 500,
    }
}
",
        head_lib: "pub enum Size {
    Small,
    Medium,
    Large,
}

pub fn price_cents(size: Size) -> u32 {
    match size {
        Size::Small => 300,
        Size::Medium => 450,
        Size::Large => 500,
    }
}
",
        existing_tests: "use sizes::*;

#[test]
fn small_and_large_prices() {
    assert_eq!(price_cents(Size::Small), 300);
    assert_eq!(price_cents(Size::Large), 500);
}
",
        weak_test: Some(
            "use sizes::*;

#[test]
fn eval_weak() {
    assert!(price_cents(Size::Medium) > price_cents(Size::Small));
}
",
        ),
        reference_test: Some(
            "use sizes::*;

#[test]
fn eval_reference() {
    assert_eq!(price_cents(Size::Medium), 450);
}
",
        ),
        gaps: &[GapKey {
            owner: "price_cents",
            line: 10,
            facts: &[&["450"]],
        }],
    },
    Scenario {
        id: "private_helper",
        summary: "a private helper's rounding changes; only the public caller is tested, far from the edge",
        crate_name: "money",
        language: Language::Rust,
        source_path: "src/lib.rs",
        support_files: &[],
        base_lib: "fn round_to_nickel(cents: u32) -> u32 {
    cents / 5 * 5
}

pub fn cash_total(cents: u32) -> u32 {
    round_to_nickel(cents)
}
",
        head_lib: "fn round_to_nickel(cents: u32) -> u32 {
    (cents + 2) / 5 * 5
}

pub fn cash_total(cents: u32) -> u32 {
    round_to_nickel(cents)
}
",
        existing_tests: "use money::*;

#[test]
fn exact_nickels_stay() {
    assert_eq!(cash_total(100), 100);
}
",
        weak_test: Some(
            "use money::*;

#[test]
fn eval_weak() {
    assert_eq!(cash_total(105), 105);
}
",
        ),
        reference_test: Some(
            "use money::*;

#[test]
fn eval_reference() {
    assert_eq!(cash_total(103), 105);
    assert_eq!(cash_total(102), 100);
}
",
        ),
        gaps: &[GapKey {
            owner: "round_to_nickel",
            line: 2,
            facts: &[&["cents + 2", "+ 2"]],
        }],
    },
    Scenario {
        id: "inclusive_range",
        summary: "an exclusive range becomes inclusive in a sum; tests only use n = 0",
        crate_name: "sums",
        language: Language::Rust,
        source_path: "src/lib.rs",
        support_files: &[],
        base_lib: "pub fn sum_to(n: u32) -> u32 {
    (0..n).sum()
}
",
        head_lib: "pub fn sum_to(n: u32) -> u32 {
    (0..=n).sum()
}
",
        existing_tests: "use sums::*;

#[test]
fn empty_sum() {
    assert_eq!(sum_to(0), 0);
}
",
        weak_test: Some(
            "use sums::*;

#[test]
fn eval_weak() {
    assert!(sum_to(4) > 0);
}
",
        ),
        reference_test: Some(
            "use sums::*;

#[test]
fn eval_reference() {
    assert_eq!(sum_to(4), 10);
}
",
        ),
        gaps: &[GapKey {
            owner: "sum_to",
            line: 2,
            facts: &[&["0..=n", "..="]],
        }],
    },
    Scenario {
        id: "refactor_control",
        summary: "a behavior-preserving refactor (extracted local); the useful answer is silence",
        crate_name: "temps",
        language: Language::Rust,
        source_path: "src/lib.rs",
        support_files: &[],
        base_lib: "pub fn to_fahrenheit(celsius: i32) -> i32 {
    celsius * 9 / 5 + 32
}
",
        head_lib: "pub fn to_fahrenheit(celsius: i32) -> i32 {
    let scaled = celsius * 9 / 5;
    scaled + 32
}
",
        existing_tests: "use temps::*;

#[test]
fn freezing_and_boiling() {
    assert_eq!(to_fahrenheit(0), 32);
    assert_eq!(to_fahrenheit(100), 212);
}
",
        weak_test: None,
        reference_test: None,
        gaps: &[],
    },
    Scenario {
        id: "py_retry_backoff",
        summary: "Python, urllib3 `Retry` backoff shape: the first retry now waits; tests use attempts far from the boundary",
        crate_name: "retrykit",
        language: Language::Python,
        source_path: "src/retrykit/backoff.py",
        support_files: &[("src/retrykit/__init__.py", "\"\"\"Retry helpers.\"\"\"\n")],
        base_lib: r#""""Retry backoff, in the style of urllib3's Retry.get_backoff_time."""

BACKOFF_MAX = 120.0


def backoff_time(attempt: int, factor: float) -> float:
    if attempt <= 1:
        return 0.0
    delay = factor * (2 ** (attempt - 1))
    return min(BACKOFF_MAX, delay)
"#,
        head_lib: r#""""Retry backoff, in the style of urllib3's Retry.get_backoff_time."""

BACKOFF_MAX = 120.0


def backoff_time(attempt: int, factor: float) -> float:
    if attempt < 1:
        return 0.0
    delay = factor * (2 ** (attempt - 1))
    return min(BACKOFF_MAX, delay)
"#,
        existing_tests: r#"from retrykit.backoff import backoff_time


def test_backoff_doubles():
    assert backoff_time(5, 0.5) == 8.0


def test_backoff_is_capped():
    assert backoff_time(20, 1.0) == 120.0
"#,
        weak_test: Some(
            r#"from retrykit.backoff import backoff_time


def test_eval_weak():
    assert backoff_time(3, 1.0) > 0
"#,
        ),
        reference_test: Some(
            r#"from retrykit.backoff import backoff_time


def test_eval_reference():
    assert backoff_time(1, 0.5) == 0.5
    assert backoff_time(0, 0.5) == 0.0
"#,
        ),
        gaps: &[GapKey {
            owner: "backoff_time",
            line: 7,
            facts: &[&["attempt == 1", "backoff_time(1"]],
        }],
    },
    Scenario {
        id: "py_invalid_version",
        summary: "Python, `packaging` InvalidVersion shape: a bad version now raises a ValueError subclass; tests catch ValueError",
        crate_name: "verlib",
        language: Language::Python,
        source_path: "src/verlib/version.py",
        support_files: &[
            ("src/verlib/__init__.py", "\"\"\"Version parsing.\"\"\"\n"),
            (
                "src/verlib/errors.py",
                "class InvalidVersion(ValueError):\n    \"\"\"A version string does not match the release pattern.\"\"\"\n",
            ),
        ],
        base_lib: r#""""Release-number parsing, in the style of packaging.version."""

from verlib.errors import InvalidVersion


def parse_release(text: str) -> tuple[int, ...]:
    parts = text.strip().split(".")
    if not all(part.isdigit() for part in parts):
        raise ValueError(f"invalid version: {text!r}")
    return tuple(int(part) for part in parts)
"#,
        head_lib: r#""""Release-number parsing, in the style of packaging.version."""

from verlib.errors import InvalidVersion


def parse_release(text: str) -> tuple[int, ...]:
    parts = text.strip().split(".")
    if not all(part.isdigit() for part in parts):
        raise InvalidVersion(f"invalid version: {text!r}")
    return tuple(int(part) for part in parts)
"#,
        existing_tests: r#"import pytest

from verlib.version import parse_release


def test_parses_release():
    assert parse_release("1.2.3") == (1, 2, 3)


def test_rejects_letters():
    with pytest.raises(ValueError):
        parse_release("1.x")
"#,
        weak_test: Some(
            r#"import pytest

from verlib.version import parse_release


def test_eval_weak():
    with pytest.raises(ValueError):
        parse_release("v1")
"#,
        ),
        reference_test: Some(
            r#"import pytest

from verlib.errors import InvalidVersion
from verlib.version import parse_release


def test_eval_reference():
    with pytest.raises(InvalidVersion):
        parse_release("1.x")
"#,
        ),
        gaps: &[GapKey {
            owner: "parse_release",
            line: 9,
            facts: &[&["InvalidVersion"]],
        }],
    },
    Scenario {
        id: "py_default_headers",
        summary: "Python, requests `default_headers` shape: one dict value changes; tests check only other keys",
        crate_name: "httpkit",
        language: Language::Python,
        source_path: "src/httpkit/utils.py",
        support_files: &[("src/httpkit/__init__.py", "\"\"\"HTTP helpers.\"\"\"\n")],
        base_lib: r#""""Default request headers, in the style of requests.utils.default_headers."""

USER_AGENT = "httpkit/0.1"


def default_headers() -> dict[str, str]:
    return {
        "User-Agent": USER_AGENT,
        "Accept-Encoding": "gzip, deflate",
        "Accept": "*/*",
        "Connection": "keep-alive",
    }
"#,
        head_lib: r#""""Default request headers, in the style of requests.utils.default_headers."""

USER_AGENT = "httpkit/0.1"


def default_headers() -> dict[str, str]:
    return {
        "User-Agent": USER_AGENT,
        "Accept-Encoding": "gzip, deflate, br",
        "Accept": "*/*",
        "Connection": "keep-alive",
    }
"#,
        existing_tests: r#"from httpkit.utils import default_headers


def test_user_agent():
    assert default_headers()["User-Agent"] == "httpkit/0.1"


def test_accepts_anything():
    assert default_headers()["Accept"] == "*/*"
"#,
        weak_test: Some(
            r#"from httpkit.utils import default_headers


def test_eval_weak():
    assert "Accept-Encoding" in default_headers()
"#,
        ),
        reference_test: Some(
            r#"from httpkit.utils import default_headers


def test_eval_reference():
    assert default_headers()["Accept-Encoding"] == "gzip, deflate, br"
"#,
        ),
        gaps: &[GapKey {
            owner: "default_headers",
            line: 9,
            facts: &[&["gzip, deflate, br"]],
        }],
    },
    Scenario {
        id: "py_refactor_control",
        summary: "Python control: a behavior-preserving refactor (extracted local); the useful answer is silence",
        crate_name: "units",
        language: Language::Python,
        source_path: "src/units/temperature.py",
        support_files: &[("src/units/__init__.py", "\"\"\"Unit conversions.\"\"\"\n")],
        base_lib: r#"def to_fahrenheit(celsius: float) -> float:
    return celsius * 9 / 5 + 32
"#,
        head_lib: r#"def to_fahrenheit(celsius: float) -> float:
    scaled = celsius * 9 / 5
    return scaled + 32
"#,
        existing_tests: r#"from units.temperature import to_fahrenheit


def test_freezing_and_boiling():
    assert to_fahrenheit(0) == 32
    assert to_fahrenheit(100) == 212
"#,
        weak_test: None,
        reference_test: None,
        gaps: &[],
    },
    Scenario {
        id: "ts_duration_hour",
        summary: "TypeScript, vercel/ms `fmtShort` shape: exactly one hour now formats as 1h; tests import through an index re-export and use other durations",
        crate_name: "duration",
        language: Language::TypeScript,
        source_path: "src/format.ts",
        support_files: &[(
            "src/index.ts",
            "export { formatShort } from \"./format.ts\";\n",
        )],
        base_lib: r#"// Short duration formatting, in the style of vercel/ms.
const s = 1000;
const m = s * 60;
const h = m * 60;
const d = h * 24;

export function formatShort(ms: number): string {
  const msAbs = Math.abs(ms);
  if (msAbs >= d) {
    return `${Math.round(ms / d)}d`;
  }
  if (msAbs > h) {
    return `${Math.round(ms / h)}h`;
  }
  if (msAbs >= m) {
    return `${Math.round(ms / m)}m`;
  }
  if (msAbs >= s) {
    return `${Math.round(ms / s)}s`;
  }
  return `${ms}ms`;
}
"#,
        head_lib: r#"// Short duration formatting, in the style of vercel/ms.
const s = 1000;
const m = s * 60;
const h = m * 60;
const d = h * 24;

export function formatShort(ms: number): string {
  const msAbs = Math.abs(ms);
  if (msAbs >= d) {
    return `${Math.round(ms / d)}d`;
  }
  if (msAbs >= h) {
    return `${Math.round(ms / h)}h`;
  }
  if (msAbs >= m) {
    return `${Math.round(ms / m)}m`;
  }
  if (msAbs >= s) {
    return `${Math.round(ms / s)}s`;
  }
  return `${ms}ms`;
}
"#,
        existing_tests: r#"import { test } from "node:test";
import assert from "node:assert/strict";
import { formatShort } from "../src/index.ts";

test("formats hours", () => {
  assert.equal(formatShort(7_200_000), "2h");
});

test("formats minutes", () => {
  assert.equal(formatShort(1_800_000), "30m");
});
"#,
        weak_test: Some(
            r#"import { test } from "node:test";
import assert from "node:assert/strict";
import { formatShort } from "../src/index.ts";

test("eval weak", () => {
  assert.ok(formatShort(36_000_000).endsWith("h"));
});
"#,
        ),
        reference_test: Some(
            r#"import { test } from "node:test";
import assert from "node:assert/strict";
import { formatShort } from "../src/index.ts";

test("eval reference", () => {
  assert.equal(formatShort(3_600_000), "1h");
  assert.equal(formatShort(3_599_999), "60m");
});
"#,
        ),
        gaps: &[GapKey {
            owner: "formatShort",
            line: 12,
            facts: &[&["msAbs == h", "3600000", "3_600_000"]],
        }],
    },
    Scenario {
        id: "ts_slugify_default",
        summary: "TypeScript, sindresorhus/slugify options shape: an option default flips; tests always pass the option",
        crate_name: "slugkit",
        language: Language::TypeScript,
        source_path: "src/slugify.ts",
        support_files: &[],
        base_lib: r#"export interface SlugifyOptions {
  separator?: string;
  lowercase?: boolean;
}

export function slugify(text: string, options: SlugifyOptions = {}): string {
  const separator = options.separator ?? "-";
  const lowercase = options.lowercase ?? false;
  const words = text.trim().split(/[^A-Za-z0-9]+/).filter(Boolean);
  const slug = words.join(separator);
  return lowercase ? slug.toLowerCase() : slug;
}
"#,
        head_lib: r#"export interface SlugifyOptions {
  separator?: string;
  lowercase?: boolean;
}

export function slugify(text: string, options: SlugifyOptions = {}): string {
  const separator = options.separator ?? "-";
  const lowercase = options.lowercase ?? true;
  const words = text.trim().split(/[^A-Za-z0-9]+/).filter(Boolean);
  const slug = words.join(separator);
  return lowercase ? slug.toLowerCase() : slug;
}
"#,
        existing_tests: r#"import { test } from "node:test";
import assert from "node:assert/strict";
import { slugify } from "../src/slugify.ts";

test("lowercases when asked", () => {
  assert.equal(slugify("Hello World", { lowercase: true }), "hello-world");
});

test("custom separator", () => {
  assert.equal(slugify("a b", { separator: "_", lowercase: false }), "a_b");
});
"#,
        weak_test: Some(
            r#"import { test } from "node:test";
import assert from "node:assert/strict";
import { slugify } from "../src/slugify.ts";

test("eval weak", () => {
  assert.equal(slugify("Hello World").length, 11);
});
"#,
        ),
        reference_test: Some(
            r#"import { test } from "node:test";
import assert from "node:assert/strict";
import { slugify } from "../src/slugify.ts";

test("eval reference", () => {
  assert.equal(slugify("Hello World"), "hello-world");
});
"#,
        ),
        gaps: &[GapKey {
            owner: "slugify",
            line: 8,
            facts: &[&["lowercase"], &["true"]],
        }],
    },
    Scenario {
        id: "ts_tested_control",
        summary: "TypeScript control: a boundary change whose existing tests already pin below, equal and above",
        crate_name: "ages",
        language: Language::TypeScript,
        source_path: "src/age.ts",
        support_files: &[],
        base_lib: r#"export function isAdult(age: number): boolean {
  return age > 18;
}
"#,
        head_lib: r#"export function isAdult(age: number): boolean {
  return age >= 18;
}
"#,
        existing_tests: r#"import { test } from "node:test";
import assert from "node:assert/strict";
import { isAdult } from "../src/age.ts";

test("adult boundary", () => {
  assert.equal(isAdult(17), false);
  assert.equal(isAdult(18), true);
  assert.equal(isAdult(19), true);
});
"#,
        weak_test: None,
        reference_test: None,
        gaps: &[],
    },
    Scenario {
        id: "rs_semver_prerelease",
        summary: "Rust, semver `Prerelease` ordering shape in a nested module: numeric identifiers now compare as numbers; tests use single digits",
        crate_name: "semverish",
        language: Language::Rust,
        source_path: "src/version/compare.rs",
        support_files: &[
            ("src/lib.rs", "//! Version handling.\n\npub mod version;\n"),
            (
                "src/version/mod.rs",
                "//! Version ordering.\n\npub mod compare;\n",
            ),
        ],
        base_lib: r#"//! Pre-release precedence, in the style of semver's `Prerelease` ordering.

use std::cmp::Ordering;

/// Compare two pre-release tags. A release (empty tag) outranks any
/// pre-release; dot-separated identifiers compare left to right.
pub fn cmp_pre(left: &str, right: &str) -> Ordering {
    match (left.is_empty(), right.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    for (a, b) in left.split('.').zip(right.split('.')) {
        let order = match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(_), Ok(_)) => a.cmp(b),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => a.cmp(b),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    left.split('.').count().cmp(&right.split('.').count())
}
"#,
        head_lib: r#"//! Pre-release precedence, in the style of semver's `Prerelease` ordering.

use std::cmp::Ordering;

/// Compare two pre-release tags. A release (empty tag) outranks any
/// pre-release; dot-separated identifiers compare left to right.
pub fn cmp_pre(left: &str, right: &str) -> Ordering {
    match (left.is_empty(), right.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    for (a, b) in left.split('.').zip(right.split('.')) {
        let order = match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => a.cmp(b),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    left.split('.').count().cmp(&right.split('.').count())
}
"#,
        existing_tests: r#"use semverish::version::compare::cmp_pre;
use std::cmp::Ordering;

#[test]
fn alphabetic_identifiers_sort() {
    assert_eq!(cmp_pre("alpha", "beta"), Ordering::Less);
}

#[test]
fn release_outranks_prerelease() {
    assert_eq!(cmp_pre("", "rc.1"), Ordering::Greater);
}
"#,
        weak_test: Some(
            r#"use semverish::version::compare::cmp_pre;
use std::cmp::Ordering;

#[test]
fn eval_weak() {
    assert_eq!(cmp_pre("rc.1", "rc.2"), Ordering::Less);
}
"#,
        ),
        reference_test: Some(
            r#"use semverish::version::compare::cmp_pre;
use std::cmp::Ordering;

#[test]
fn eval_reference() {
    assert_eq!(cmp_pre("rc.2", "rc.10"), Ordering::Less);
}
"#,
        ),
        gaps: &[GapKey {
            owner: "cmp_pre",
            line: 16,
            facts: &[&["x.cmp(&y)", "cmp(&y)"]],
        }],
    },
];
