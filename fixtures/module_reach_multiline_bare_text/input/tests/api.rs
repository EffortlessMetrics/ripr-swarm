use qual_reach::inner::order_discount;
#[test]
fn discount_boundary() {
    let _text = r#"a quoted bare entry
    order_discount(10)"#;
    assert_eq!(foreign::order_discount(10), 5);
}
