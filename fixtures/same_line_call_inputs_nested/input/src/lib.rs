pub fn band(n: u8) -> u8 {
    if n > 9 { 2 } else { 1 }
}
#[test]
fn nested() { assert_eq!(band(band(9)), 1); }
#[test]
fn commented() { assert_eq!(band(10), 2); assert_eq!(band /* outer */ (band(9)), 1); }
