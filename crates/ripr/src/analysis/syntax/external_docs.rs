//! Whether a Rust source pulls documentation in from outside its own text
//! (`#![doc = include_str!(..)]`, `#[doc = concat!(..)]`), so doctests ripr
//! does not index may call any workspace function (#13754).
//!
//! The file-fact producer answers this from the parse it already holds and
//! stores the answer with the file's facts, so the unseen-reach scan reads one
//! bit per file instead of re-parsing the workspace on every run.

use ra_ap_syntax::{AstNode, SyntaxNode, ast};

/// Whitespace the gate skips between `doc` and `=`: Unicode `White_Space`
/// plus the two marks Rust's lexer also treats as whitespace
/// (`Pattern_White_Space`) that `char::is_whitespace` leaves out.
fn rust_or_unicode_whitespace(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{200E}' | '\u{200F}')
}

/// The cheap text gate in front of the parse: a source that never spells
/// `doc` and `!` cannot include external docs. A source known to parse
/// cleanly (`clean`: the file-fact producer already parsed it without
/// errors) also needs a `doc` followed by `=`, because a key-value `doc`
/// attribute has the `=` token right after its name with only whitespace or
/// a comment between. A following `/` (a possible comment) keeps the source
/// a candidate. An unclean source stays a candidate on the first test alone:
/// the scan answers `true` for a source it cannot parse.
pub(crate) fn may_include_external_docs(source: &str, clean: bool) -> bool {
    source.contains("doc")
        && source.contains('!')
        && (!clean
            || source.match_indices("doc").any(|(at, name)| {
                source[at + name.len()..]
                    .trim_start_matches(rust_or_unicode_whitespace)
                    .starts_with(['=', '/'])
            }))
}

/// Whether any attribute under `root`, read through `cfg_attr`, is a
/// key-value `doc` whose value is not a literal.
pub(crate) fn tree_includes_external_docs(root: &SyntaxNode) -> bool {
    root.descendants()
        .filter_map(ast::Attr::cast)
        .any(|attribute| {
            attribute.meta().is_some_and(|meta| {
                meta.skip_cfg_attrs().into_iter().any(|meta| {
                    meta.simple_name().is_some_and(|name| name == "doc")
                        && matches!(meta, ast::Meta::KeyValueMeta(value)
                        if !matches!(value.expr(), Some(ast::Expr::Literal(_))))
                })
            })
        })
}

/// The answer for `source` given its clean parse `tree`: exactly what the
/// unseen-reach scan computes for a clean source (the text gate, then the
/// attribute scan of that same parse).
pub(crate) fn parsed_source_includes_external_docs(source: &str, tree: &ast::SourceFile) -> bool {
    may_include_external_docs(source, true) && tree_includes_external_docs(tree.syntax())
}
