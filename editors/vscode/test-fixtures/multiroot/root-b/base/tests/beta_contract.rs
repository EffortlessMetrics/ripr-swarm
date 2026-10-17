use root_beta::beta_shipping;

#[test]
fn beta_light_parcel() {
    assert_eq!(beta_shipping(5, 10), 7);
}

#[test]
fn beta_heavy_parcel() {
    assert_eq!(beta_shipping(100, 10), 300);
}
