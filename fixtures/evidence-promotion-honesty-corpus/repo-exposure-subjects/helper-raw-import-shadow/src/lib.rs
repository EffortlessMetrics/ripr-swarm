fn leaf_value() -> Option<u32> { Some(9) }
mod other { pub fn leaf_value() -> Option<u32> { Some(7) } }
fn read_leaf_value() -> Option<u32> {
    use crate::other::leaf_value as r#leaf_value;
    leaf_value()
}
#[test] fn observes_binding() { assert_eq!(read_leaf_value(), Some(7)); }
