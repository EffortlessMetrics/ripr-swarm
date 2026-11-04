mod real {
    pub fn leaf_value() -> Option<u32> {
        Some(7)
    }
}

mod decoy {
    pub fn leaf_value() -> Option<u32> {
        Some(9)
    }
}

fn read_leaf_value() -> Option<u32> {
    real::leaf_value()
}

#[test]
fn observes_real() {
    assert_eq!(read_leaf_value(), Some(7));
}
