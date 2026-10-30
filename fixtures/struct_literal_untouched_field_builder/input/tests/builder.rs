use struct_literal_untouched_field_builder::RequestBuilder;

#[test]
fn url_is_kept() {
    assert_eq!(RequestBuilder::new("http://x").build().url, "http://x");
}

#[test]
fn explicit_timeout_is_kept() {
    let request = RequestBuilder::new("u").timeout_ms(500).build();
    assert_eq!(request.timeout_ms, 500);
}
