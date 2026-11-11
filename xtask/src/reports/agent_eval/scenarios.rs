//! Answer-keyed scenarios for `cargo xtask agent-eval`.
//!
//! Each scenario is a tiny crate with a committed base, a committed head
//! (the intended behavior change), the tests the repository already had, and
//! two test files an agent might add: a `weak` one that runs the changed code
//! without telling the old and new behavior apart, and a `reference` one that
//! does. The answer key names, per gap, the owning function, the changed line
//! and the facts a test must pin. Keys are hand-written facts about the
//! scenario, not ripr output, so a route that drops the fact is visible.
//!
//! Keep scenarios small and realistic. Changing a scenario changes the
//! baseline: rerun with `--write-baseline` in the same change.

pub(crate) struct GapKey {
    /// Function that owns the changed behavior.
    pub(crate) owner: &'static str,
    /// 1-based changed line in `src/lib.rs` at head.
    pub(crate) line: u32,
    /// Each entry is one fact; any of its spellings counts as present.
    pub(crate) facts: &'static [&'static [&'static str]],
}

pub(crate) struct Scenario {
    pub(crate) id: &'static str,
    pub(crate) summary: &'static str,
    pub(crate) crate_name: &'static str,
    pub(crate) base_lib: &'static str,
    pub(crate) head_lib: &'static str,
    /// Existing integration tests (`tests/existing.rs`), committed at base.
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
            facts: &[&["verbose"], &["--verbose"]],
        }],
    },
    Scenario {
        id: "tested_control",
        summary: "a boundary change whose existing tests already pin below, equal and above",
        crate_name: "ages",
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
                facts: &[&["100"]],
            },
            GapKey {
                owner: "late_fee",
                line: 6,
                facts: &[&["days * 3", "* 3"]],
            },
        ],
    },
];
