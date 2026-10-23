const OFFSET: i32 = 3;
fn OFFSET_FN(x: i32) -> i32 { x - 3 }
pub fn masked(n: i32) -> impl Fn(i32) -> i32 {
    move |x| n + (x - OFFSET)
}
pub fn unit(n: i32) -> impl Fn(i32) -> i32 {
    move |x| n * x
}
pub fn identifier(n: i32) -> impl Fn(i32) -> i32 {
    move |x| n + x
}
pub fn uppercase(n: i32) -> impl Fn(i32) -> i32 {
    move |x| n + OFFSET_FN(x)
}
pub fn token_unit(captured: i32) -> impl Fn(i32) -> i32 {
    move |argument| captured * argument
}
pub fn token_unused(captured: i32) -> impl Fn(i32) -> i32 {
    move |argument| captured * argument
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn masked_pin() { assert_eq!(masked(2)(3), 2); }
    #[test]
    fn unit_pin() { assert_eq!(unit(5)(1), 5); }
    #[test]
    fn identifier_pin() { let _2 = 0; assert_eq!(identifier(_2)(3), 3); }
    #[test]
    fn uppercase_pin() { assert_eq!(uppercase(2)(3), 2); }
    #[test]
    fn token_unit_pin() { let captured = 5; assert_eq!(token_unit(5)(1), captured); }
    #[test]
    fn token_unused_pin() { let _unused = token_unused(5); let argument = 5; assert_eq!(argument, 5); }
}
