pub mod production {
    pub fn as_object(n: u32) -> u32 {
        if n == 0 { 7 } else { as_object(n - 1) }
    }
}
pub mod foreign {
    pub fn as_object(_: u32) -> u32 { 9 }
}
#[cfg(test)] mod tests {
    #[test] fn only_foreign() { assert_eq!(super::foreign::as_object(2), 9); }
}
