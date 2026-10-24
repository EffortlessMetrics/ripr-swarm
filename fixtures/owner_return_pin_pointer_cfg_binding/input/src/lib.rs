pub fn call_through_pointer(x: u32) -> u32 {
    3 * x
}
#[cfg(test)]
mod tests {
    fn f(_: u32) -> u32 { 6 }
    #[test]
    fn cfg_binding() {
        #[cfg(any())]
        let f: fn(u32) -> u32 = crate::call_through_pointer;
        assert_eq!(f(2), 6);
    }
}
