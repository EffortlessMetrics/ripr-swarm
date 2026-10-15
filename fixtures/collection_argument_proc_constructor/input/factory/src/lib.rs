extern crate proc_macro;
#[proc_macro_derive(Factory)]
pub fn factory(_: proc_macro::TokenStream) -> proc_macro::TokenStream {
    "pub fn Bag(_: std::vec::Vec<u32>) -> Payload { Payload(Numbers::new()) }".parse().unwrap_or_default()
}
