use shop::shipping;

#[test]
fn big_order_ships_free() {
    assert_eq!(shipping(20), 0);
}

#[test]
fn small_order_pays_shipping() {
    assert_eq!(shipping(2), 800);
}
