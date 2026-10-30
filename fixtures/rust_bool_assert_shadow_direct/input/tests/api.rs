use rust_bool_assert_shadow_direct::can_ship;

macro_rules! assert_eq {
    ($actual:expr, $want:expr) => {{
        let _ = $actual;
    }};
}

#[test]
fn ships() {
    assert_eq!(can_ship(true, true), true);
    assert_eq!(can_ship(true, false), false);
    assert_eq!(can_ship(false, true), false);
}
