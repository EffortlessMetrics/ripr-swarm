use owner_pin_control::weight;

#[test]
#[cfg(feature = "std")]
fn checks_weight() {
    assert_eq!(weight(4), 12);
}
