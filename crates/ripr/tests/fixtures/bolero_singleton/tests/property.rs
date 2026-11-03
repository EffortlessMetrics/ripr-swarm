use lib_bolero_fixture::increment;

#[test]
fn increment_matches_model() {
    bolero::check!()
        .with_type::<u8>()
        .for_each(|n| {
            assert_eq!(increment(*n), u16::from(*n) + 1);
        });
}
