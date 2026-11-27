pub fn call_through_pointer(x: u32) -> u32 {
    3 * x
}
mod decoy { pub fn f(_: u32) -> u32 { 6 } }
#[cfg(test)]
mod tests {
    #[test]
    fn glob_shadow() {
        let f: fn(u32) -> u32 = super::call_through_pointer;
        {
            use super::decoy::*;
            assert_eq!(f(2), 6);
        }
    }
}
