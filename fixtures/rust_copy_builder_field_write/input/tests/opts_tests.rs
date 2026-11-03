use rust_copy_builder_field_write_fixture::Opts;

#[test]
fn builder_leaves_the_callers_copy_alone() {
    let o = Opts { retries: 3 };
    o.retries(5);
    assert_eq!(o.retries, 3);
}
