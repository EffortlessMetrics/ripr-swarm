//! Invoice totals in integer cents.
//!
//! Evaluator input domain: at most 100 lines, unit_cents <= 1_000_000,
//! qty <= 10_000, tax_bps <= 10_000, and BuyNGetOneFree parameter n <= 1_000_000.
//! PercentOff and FlatOff accept any u32 (capped and saturating respectively).
//! Inputs outside this domain are not evaluated; this fixture does not validate
//! them. These bounds keep intermediate arithmetic within u32/u64 limits.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line {
    pub unit_cents: u32,
    pub qty: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Promo {
    None,
    PercentOff(u32),
    FlatOff(u32),
    BuyNGetOneFree(u32),
}

/// Multiply by basis points, rounding half up.
fn bps_of(cents: u64, bps: u32) -> u64 {
    (cents * bps as u64 + 5_000) / 10_000
}

fn line_subtotal(line: &Line, promo: Promo) -> u64 {
    let gross = line.unit_cents as u64 * line.qty as u64;
    match promo {
        Promo::BuyNGetOneFree(n) if n > 0 => {
            let free = (line.qty / (n + 1)) as u64;
            gross - free * line.unit_cents as u64
        }
        _ => gross,
    }
}

fn apply_order_promo(subtotal: u64, promo: Promo) -> u64 {
    match promo {
        Promo::PercentOff(p) => subtotal - bps_of(subtotal, p.min(100) * 100),
        Promo::FlatOff(c) => subtotal.saturating_sub(c as u64),
        _ => subtotal,
    }
}

/// Total = promo-adjusted subtotal plus tax. Tax is charged on the discounted
/// subtotal, never on a negative amount. Empty invoices cost 0.
pub fn invoice_total(lines: &[Line], tax_bps: u32, promo: Promo) -> u64 {
    if lines.is_empty() {
        return 0;
    }
    let subtotal: u64 = lines.iter().map(|l| line_subtotal(l, promo)).sum();
    let discounted = apply_order_promo(subtotal, promo);
    discounted + bps_of(discounted, tax_bps)
}
