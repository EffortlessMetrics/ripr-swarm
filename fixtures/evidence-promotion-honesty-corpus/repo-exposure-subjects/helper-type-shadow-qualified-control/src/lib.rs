fn seed_value() {}
fn leaf_value() -> Option<u32> { Some(7) }
mod helpers { pub fn read_leaf_value() -> Option<u32> { crate::leaf_value() } }
trait Foreign { fn read_leaf_value() -> Option<u32> { Some(7) } }
struct Fake;
impl Foreign for Fake {}
fn route_read_leaf_value_seed_value_leaf_value<Helpers: Foreign>() -> Option<u32> {
    seed_value();
    crate::helpers::read_leaf_value()
}
#[test] fn decoy() { assert_eq!(route_read_leaf_value_seed_value_leaf_value::<Fake>(), Some(7)); }
