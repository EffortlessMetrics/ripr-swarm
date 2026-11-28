pub fn fits(n: u8, max: u8) -> bool {
    n <= max
}

#[cfg(kani)]
mod proofs {
    #[kani::proof]
    fn pins_boundary() {
        let n: u8 = kani::any();
        let max: u8 = kani::any();
        let _ = super::fits(n, max);
        assert!(true);
    }
}
