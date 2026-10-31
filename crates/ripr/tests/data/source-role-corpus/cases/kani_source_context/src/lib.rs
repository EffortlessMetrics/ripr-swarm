pub fn boundary(value: u8) -> bool {
    value < 10
}

#[test]
fn ordinary() {
    assert!(boundary(1));
}

#[cfg(kani)]
mod proofs {
    #[kani::proof]
    fn exact() {
        let value: u8 = kani::any();
        assert_eq!(super::boundary(value), value < 10);
        fn local() -> bool { 1 < 2 }
        let _ = local();
    }

    #[cfg(not(kani))]
    #[kani::proof]
    fn disabled() {}

    #[cfg(test)]
    #[kani::proof]
    fn native_test_mode() {}

    #[cfg(feature = "larger-domain")]
    #[kani::proof]
    fn feature_unknown() {}

    mod left {
        #[kani::proof]
        fn same() { kani::cover!(true); }
    }

    mod right {
        #[kani::proof]
        fn same() { let _ = super::super::boundary(1); }
    }
}
