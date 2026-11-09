pub fn band(n: u8) -> u8 {
    if n > 9 { 2 } else { 1 }
}
fn ignore(_: u8) -> u8 { 0 }
#[test]
fn arithmetic() { assert_eq!(band(10), 2); assert_eq!(band(9) * 0, 0); }
#[test]
fn ignored() { assert_eq!(band(10), 2); assert_eq!(ignore(band(9)), 0); }
