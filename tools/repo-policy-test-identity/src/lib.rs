//! Compiler-time full resource custody without package-global volatile env.

use proc_macro::TokenStream;
use std::path::Path;

#[path = "../../repo-policy/source_identity.rs"]
mod source_identity;

#[proc_macro]
pub fn identities(input: TokenStream) -> TokenStream {
    let result = if input.is_empty() {
        std::env::var_os("CARGO_MANIFEST_DIR")
            .ok_or_else(|| "policy identity macro requires CARGO_MANIFEST_DIR".to_string())
            .and_then(|manifest| expand(&std::path::PathBuf::from(manifest).join("../..")))
    } else {
        Err("policy identities macro accepts no arguments".into())
    };
    match result.and_then(|source| {
        source
            .parse()
            .map_err(|error| format!("policy identity expansion: {error}"))
    }) {
        Ok(tokens) => tokens,
        Err(error) => {
            use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenTree};
            TokenStream::from_iter([
                TokenTree::Ident(Ident::new("compile_error", Span::call_site())),
                TokenTree::Punct(Punct::new('!', Spacing::Alone)),
                TokenTree::Group(Group::new(
                    Delimiter::Parenthesis,
                    TokenStream::from(TokenTree::Literal(Literal::string(&error))),
                )),
            ])
        }
    }
}

fn expand(root: &Path) -> Result<String, String> {
    let implementation = source_identity::implementation_identity(root)?;
    let resources = source_identity::source_identity(root)?;
    let mut output = String::from("{");
    for path in source_identity::source_inputs(root)? {
        let path = path
            .to_str()
            .ok_or("policy identity input path is not UTF-8")?;
        // Unused constants register exact rustc dependencies but retain no
        // raw source bytes in the linked test executable.
        output.push_str(&format!(
            "const _: &[u8] = ::core::include_bytes!({path:?});"
        ));
    }
    output.push_str(&format!("({implementation:?}, {resources:?}) }}"));
    Ok(output)
}
