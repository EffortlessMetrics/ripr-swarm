use rust_twin_collection_read_self_computed::*;

#[test]
fn twin_bags_match() {
    let mut c = Bag::new();
    c.add(20);
    let mut d = Bag::new();
    d.add(20);
    assert_eq!(c.items, d.items);
}
