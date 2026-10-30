use rust_twin_collection_read_literal_control::*;

#[test]
fn bag_holds_the_added_item() {
    let mut c = Bag::new();
    c.add(20);
    assert_eq!(c.items, vec![20]);
}
