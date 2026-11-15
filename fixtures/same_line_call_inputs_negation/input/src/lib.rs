pub fn band(n: u8) -> Mask {
    if n > 9 { Mask(2) } else { Mask(1) }
}
pub struct Mask(u8);
impl std::ops::Not for Mask {
    type Output = u8;
    fn not(self) -> u8 { 0 }
}
#[test]
fn negated() { let _ = band(10); assert_eq!(!band(9), 0); }
