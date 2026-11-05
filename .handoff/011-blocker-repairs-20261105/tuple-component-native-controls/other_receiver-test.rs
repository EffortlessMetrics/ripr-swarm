use rust_same_method_owner_type_positive_fixture::{WhileSome,Combinations};

#[test]
fn while_some_size_hint_upper_bound() {
    let it = WhileSome { remaining: 2 };
    let _=it.size_hint();
    let other=Combinations {remaining:2};
    assert_eq!(other.size_hint().1, Some(other.remaining));
}
