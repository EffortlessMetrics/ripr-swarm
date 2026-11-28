pub fn band(n: u8) -> u8 {
    if n > 9 { 2 } else { 1 }
}
#[test]
fn observed() { assert_eq!(band(band(9)), 1); assert_eq!(band(9), 1); }
