pub fn call_through_pointer(x: u32) -> u32 {
    3 * x
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_shadow() {
        let f: fn(u32) -> u32 = crate::call_through_pointer;
        {
            fn r#f(_: u32) -> u32 { 6 }
            assert_eq!(f(2), 6);
        }
    }
}
