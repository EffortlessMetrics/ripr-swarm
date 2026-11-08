use ab_invoice::{invoice_total, Line, Promo};

#[test]
fn invoice_total_is_positive() {
    let lines = [Line { unit_cents: 250, qty: 4 }];
    assert!(invoice_total(&lines, 800, Promo::None) > 0);
}

#[test]
fn promo_does_not_increase_total() {
    let lines = [Line { unit_cents: 250, qty: 4 }];
    assert!(invoice_total(&lines, 800, Promo::PercentOff(10)) <= invoice_total(&lines, 800, Promo::None));
}
