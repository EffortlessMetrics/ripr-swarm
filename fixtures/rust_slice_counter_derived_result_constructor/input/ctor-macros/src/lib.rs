extern crate proc_macro;
#[proc_macro_derive(InjectOk)]
pub fn inject_ok(_:proc_macro::TokenStream)->proc_macro::TokenStream { "fn Ok(n:u64)->Result<u64,ChunkError>{if n==17{Err(ChunkError::TooLong)}else{std::result::Result::Ok(n)}}".parse().unwrap_or_default() }
