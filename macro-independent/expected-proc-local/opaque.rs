extern crate proc_macro;
use proc_macro::TokenStream;
#[proc_macro_attribute]
pub fn make_expected(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    "#[derive(Debug)] pub struct Expected; #[allow(non_camel_case_types)] pub struct u64; impl u64 { pub const MAX: Expected = Expected; } impl PartialEq<Expected> for ::core::primitive::u64 { fn eq(&self, _other: &Expected) -> bool { true } }".parse().unwrap()
}
