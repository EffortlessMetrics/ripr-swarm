//! Committed Rust fixtures for no-path repair-start tests.

pub(super) const LOYALTY_BASE: &str = "pub fn doubled(x: u64) -> u64 {\n    x * 2\n}\n";
pub(super) const LOYALTY_NEW: &str = "pub fn doubled(x: u64) -> u64 {\n    x * 2\n}\n\npub fn loyalty_price(amount: u64, member_years: u32) -> u64 {\n    if member_years >= 5 {\n        amount - amount * 5 / 100\n    } else {\n        amount\n    }\n}\n";
pub(super) const LOYALTY_TEST: &str =
    "use fx::doubled;\n\n#[test]\nfn doubles() {\n    assert_eq!(doubled(2), 4);\n}\n";
pub(super) const CALLED_BASE: &str = "pub fn doubled(x: u64) -> u64 {\n    x * 2\n}\n\npub fn checkout(amount: u64) -> u64 {\n    amount\n}\n";
pub(super) const CALLED_NEW: &str = "pub fn doubled(x: u64) -> u64 {\n    x * 2\n}\n\npub fn checkout(amount: u64) -> u64 {\n    loyalty_price(amount, 6)\n}\n\npub fn loyalty_price(amount: u64, member_years: u32) -> u64 {\n    if member_years >= 5 {\n        amount - amount * 5 / 100\n    } else {\n        amount\n    }\n}\n";
