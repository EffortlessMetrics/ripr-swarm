//! Where a Rust source declares `const` and `static` items, read from the
//! file-fact producer's clean parse and stored with the file's facts
//! (#13754).
//!
//! The owner-pin constant table needs every workspace `const`/`static` by
//! name. Building it used to re-parse every file of every index a run
//! builds, although the producer had already parsed each file cleanly. The
//! producer now records the byte ranges the table reads (item name, body
//! and type, and the token trees of macro calls that may declare items), so
//! the table slices the stored text instead of parsing it again. The table
//! still interprets that text itself; this module only locates it.

use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, TextRange};

/// A byte range of the file's source text, as `(start, end)`.
pub(crate) type SourceSpan = (u32, u32);

/// One place a clean parse found a possible constant declaration, in the
/// parse tree's preorder.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum ConstantSyntax {
    /// A `const` or `static` item with a name and an initializer.
    Item {
        name: SourceSpan,
        body: SourceSpan,
        ty: Option<SourceSpan>,
    },
    /// The token tree of a macro call whose text spells `const` or
    /// `static`, which the table reads as text (`thread_local!`,
    /// `lazy_static!`).
    MacroTokens(SourceSpan),
}

fn span(range: TextRange) -> SourceSpan {
    (range.start().into(), range.end().into())
}

/// The [`ConstantSyntax`] of a source that parsed cleanly into `tree`.
///
/// A source that never spells `const` or `static` declares none, so it
/// yields nothing without walking the tree. A macro call whose token text
/// spells neither keyword cannot declare one: the table only reads
/// whole-word `const`/`static` from it.
pub(crate) fn constant_syntax(source: &str, tree: &ast::SourceFile) -> Vec<ConstantSyntax> {
    let mut found = Vec::new();
    if !source.contains("const") && !source.contains("static") {
        return found;
    }
    for node in tree.syntax().descendants() {
        let item = if let Some(item) = ast::Const::cast(node.clone()) {
            item.name().zip(item.body()).map(|(name, body)| {
                (
                    name.syntax().text_range(),
                    body.syntax().text_range(),
                    item.ty(),
                )
            })
        } else if let Some(item) = ast::Static::cast(node.clone()) {
            item.name().zip(item.body()).map(|(name, body)| {
                (
                    name.syntax().text_range(),
                    body.syntax().text_range(),
                    item.ty(),
                )
            })
        } else {
            if let Some(macro_call) = ast::MacroCall::cast(node)
                && let Some(tokens) = macro_call.token_tree()
            {
                let range = tokens.syntax().text_range();
                let text = source.get(usize::from(range.start())..usize::from(range.end()));
                if text.is_none_or(|text| text.contains("const") || text.contains("static")) {
                    found.push(ConstantSyntax::MacroTokens(span(range)));
                }
            }
            None
        };
        if let Some((name, body, ty)) = item {
            found.push(ConstantSyntax::Item {
                name: span(name),
                body: span(body),
                ty: ty.map(|ty| span(ty.syntax().text_range())),
            });
        }
    }
    found
}

/// The text of `span` in `source`, when it lies on character boundaries.
pub(crate) fn span_text(source: &str, span: SourceSpan) -> Option<&str> {
    source.get(usize::try_from(span.0).ok()?..usize::try_from(span.1).ok()?)
}
