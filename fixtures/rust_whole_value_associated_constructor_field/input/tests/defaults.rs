use whole_value_associated_constructor_field::Cfg;

#[test]
fn defaults() {
    assert_eq!(
        Cfg::new(),
        Cfg {
            retries: 3,
            verbose: false
        }
    );
}
