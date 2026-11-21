pub fn discount(n: u32) -> u32 { if n > 5 { 3 } else { 2 } }

#[test]
fn checks_threshold_only() {
    assert_eq!(discount(5), 2);
}
