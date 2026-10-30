pub fn call_through_pointer(x: u32) -> u32 {
    3 * x
}
#[cfg(test)]
mod tests {
    use super::*;
    fn f(_: u32) -> u32 { 6 }
    #[test]
    fn outside_alias_scope() {
        if false {
            let f: fn(u32) -> u32 = call_through_pointer;
            let _ = f(0);
        }
        assert_eq!(f(2), 6);
    }
}
