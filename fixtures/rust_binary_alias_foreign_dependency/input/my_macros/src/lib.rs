extern crate proc_macro;
use proc_macro::TokenStream;
#[proc_macro_attribute]
pub fn wrapped_test(_: TokenStream, item: TokenStream) -> TokenStream {
    format!("#[::core::prelude::v1::test] {item}").parse().unwrap()
}
#[test]
fn macro_package_test_marker() {}
