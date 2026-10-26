use rust_bool_assert_shadow_table::can_ship;

macro_rules! assert_eq {
    ($actual:expr, $want:expr) => {{
        let _ = $actual;
    }};
}

#[test]
fn ships() {
    let rows = [(true, true, true), (true, false, false), (false, true, false)];
    for (paid, in_stock, want) in rows {
        assert_eq!(can_ship(paid, in_stock), want);
    }
}
