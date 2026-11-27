#[proc_macro_derive(Ext)]
pub fn ext(_input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    "pub trait Ext { fn limit(&self) -> u32; } impl Ext for crate::Guard { fn limit(&self) -> u32 { 7 } }"
        .parse()
        .unwrap_or_default()
}
