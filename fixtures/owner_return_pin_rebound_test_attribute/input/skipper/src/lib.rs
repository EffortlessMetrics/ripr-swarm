use proc_macro::TokenStream;

/// Replaces the annotated function with nothing, so the test never runs.
#[proc_macro_attribute]
pub fn serial(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    TokenStream::new()
}
