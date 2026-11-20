use whole_value_associated_constructor_unpinned_field::Cfg;
#[test]
fn fresh_config() {
    assert_eq!(Cfg::fresh(3), Cfg { retries: 7, verbose: false });
}
