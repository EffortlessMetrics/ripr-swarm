#[test]
fn foreign_size_hint_is_observed() {
    let it = foreign::WhileSome { remaining: 3 };
    assert_eq!(it.size_hint().1, Some(3));
    assert_eq!(it.size_hint().0, 3);
    assert_eq!(it.size_hint(), (3, Some(3)));
}

#[test]
fn unicode_module_receiver_is_not_the_local_type() {
    use foreign as é;
    assert_eq!(é::WhileSome { remaining: 3 }.size_hint().1, Some(3));
}
