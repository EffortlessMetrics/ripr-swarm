//! Metered usage overage billing.
const MAX_BILLABLE_UNITS: u64 = 10_000;

/// Cents owed for usage beyond `included` units at `rate_cents` per unit.
pub fn overage_cents(usage: &[u32], included: u32, rate_cents: u64) -> u64 {
    let total: u64 = usage.iter().map(|&u| u as u64).sum();
    let over = total.saturating_sub(included as u64);
    over.min(MAX_BILLABLE_UNITS) * rate_cents
}
