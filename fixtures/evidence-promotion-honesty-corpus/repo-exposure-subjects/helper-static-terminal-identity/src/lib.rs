struct Real;
impl Real { fn leaf_value() -> Option<u32> { Some(7) } }
struct Decoy;
impl Decoy { fn leaf_value() -> Option<u32> { Some(9) } }
fn read_leaf_value() -> Option<u32> { Real::leaf_value() }
#[test] fn observes_real() { assert_eq!(read_leaf_value(), Some(7)); }
