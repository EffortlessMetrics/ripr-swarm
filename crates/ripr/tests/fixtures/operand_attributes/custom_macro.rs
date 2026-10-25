extern crate proc_macro;
use proc_macro::TokenStream;
#[proc_macro_derive(Debug)]
pub fn debug(_: TokenStream) -> TokenStream {
    "type u64 = Shipping;".parse().unwrap_or_default()
}
#[proc_macro_attribute]
pub fn inject(_: TokenStream, item: TokenStream) -> TokenStream {
    let mut output = item;
    output.extend("type u64 = Shipping;".parse::<TokenStream>().unwrap_or_default());
    output
}
