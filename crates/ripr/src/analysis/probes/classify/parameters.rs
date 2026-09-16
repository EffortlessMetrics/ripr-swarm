//! Parser-confirmed parameter declarations stay unknown, not executable fields.

use super::super::lexical::classify_changed_line;
use super::{ParserProbeShape, source_line_byte_range};
use crate::analysis::rust_index::FileFacts;
use crate::domain::ProbeFamily;
use ra_ap_syntax::{AstNode, Edition, SourceFile, SyntaxKind, SyntaxNode, ast};
use std::ops::Range;

/// This fallback is used only after ordinary parser-owned expressions have
/// been considered. It never promotes exposure or drops the changed source.
/// Exact current-line equality prevents borrowing new-side context for a
/// different removed/stale line. Shared signature/body lines are not eligible.
pub(super) fn parameter_declaration_shape<'a>(
    facts: &'a FileFacts,
    line: usize,
    changed_text: &str,
) -> Option<ParserProbeShape<'a>> {
    if facts.used_lexical_fallback
        || !classify_changed_line(changed_text).contains(&ProbeFamily::FieldConstruction)
    {
        return None;
    }
    let line_range = source_line_byte_range(&facts.source, line)?;
    let source_line = facts.source.get(line_range.clone())?;
    let text = source_line.trim();
    if text.is_empty() || text != changed_text.trim() {
        return None;
    }

    // Reuse the existing parser, not a colon or comment-delimiter heuristic.
    // Real expression shapes take precedence and do not enter this fallback.
    let parse = SourceFile::parse(&facts.source, Edition::CURRENT);
    if !parse.errors().is_empty() {
        return None;
    }
    let root = parse.tree();
    let declaration = parameter_line_span(root.syntax(), line_range)?;
    for parameter in root.syntax().descendants().filter(|node| {
        ast::Param::can_cast(node.kind()) || ast::SelfParam::can_cast(node.kind())
    }) {
        let Some(list) = parameter.parent().and_then(ast::ParamList::cast) else {
            continue;
        };
        if list.syntax().parent().and_then(ast::Fn::cast).is_none() {
            continue;
        }
        let range = parameter.text_range();
        let parameter_start = u32::from(range.start()) as usize;
        let parameter_end = u32::from(range.end()) as usize;
        if parameter_start <= declaration.start && declaration.end <= parameter_end {
            return Some(ParserProbeShape {
                family: ProbeFamily::StaticUnknown,
                start_line: line,
                start_byte: declaration.start,
                text,
                standalone_call: false,
                unsafe_boundary: false,
            });
        }
    }
    None
}

/// Bound the line by parser-owned nontrivia tokens, excluding its trailing
/// parameter separator. Comment text is preserved in the shape, but cannot
/// extend its containment range. Real code after a comment remains in range.
fn parameter_line_span(root: &SyntaxNode, line: Range<usize>) -> Option<Range<usize>> {
    let mut start = None;
    let mut end = None;
    for token in root
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        let range = token.text_range();
        let token_start = u32::from(range.start()) as usize;
        let token_end = u32::from(range.end()) as usize;
        if token_start >= line.end {
            break;
        }
        if token_end <= line.start
            || matches!(token.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
        {
            continue;
        }
        // A multiline nontrivia token cannot be a complete parameter-only line.
        if token_start < line.start || token_end > line.end {
            return None;
        }
        if start.is_none() {
            start = Some(token_start);
        }
        if token.kind() != SyntaxKind::COMMA {
            end = Some(token_end);
        }
    }
    let start = start?;
    let end = end?;
    Some(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
    use std::path::Path;

    const SOURCE: &str = "struct Path;\nfn project(\n    out: &Path,\n) {}\n";

    #[test]
    fn parameter_shape_preserves_exact_coordinates_and_unknown_family() -> Result<(), String> {
        for source in [SOURCE.to_string(), SOURCE.replace('\n', "\r\n")] {
            let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
            let shape = parameter_declaration_shape(&facts, 3, "    out: &Path,")
                .ok_or_else(|| "missing parser-confirmed parameter".to_string())?;
            assert_eq!(shape.family, ProbeFamily::StaticUnknown);
            assert_eq!(shape.start_line, 3);
            assert_eq!(Some(shape.start_byte), source.find("out: &Path,"));
            assert_eq!(shape.text, "out: &Path,");
            assert!(!shape.standalone_call);
            assert!(!shape.unsafe_boundary);
        }
        Ok(())
    }

    #[test]
    fn parameter_shape_requires_current_parser_owned_source() -> Result<(), String> {
        let mut facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), SOURCE)?;
        assert!(parameter_declaration_shape(&facts, 3, "out: &Other,").is_none());
        assert!(parameter_declaration_shape(&facts, 0, "out: &Path,").is_none());
        assert!(parameter_declaration_shape(&facts, 30, "out: &Path,").is_none());
        facts.used_lexical_fallback = true;
        assert!(parameter_declaration_shape(&facts, 3, "out: &Path,").is_none());
        facts.used_lexical_fallback = false;
        facts.source = "struct Path;\nfn project(\n    out: &Path,\n".to_string();
        assert!(parameter_declaration_shape(&facts, 3, "out: &Path,").is_none());
        facts.source.clear();
        assert!(parameter_declaration_shape(&facts, 3, "out: &Path,").is_none());
        Ok(())
    }

    #[test]
    fn parameter_shape_rejects_non_fn_context_and_shared_body() -> Result<(), String> {
        for (source, line, text) in [
            (
                "struct Path;\nstruct Wrap<'a> { out: &'a Path }\nfn p() -> Wrap<'static> {\n    Wrap {\n        out: &Path,\n    }\n}\n",
                5,
                "out: &Path,",
            ),
            (
                "struct Path;\ntype Handler = fn(\n    out: &Path,\n);\n",
                3,
                "out: &Path,",
            ),
            (
                "struct Path;\nfn project(\n    out: &Path,) -> i32 { 7 }\n",
                3,
                "out: &Path,) -> i32 { 7 }",
            ),
            (
                "struct Path;\nfn project() { let callback = |\n    out: &Path,\n| 7; }\n",
                3,
                "out: &Path,",
            ),
            (
                "struct Path;\nfn project(\n    out: &Path, /* note */) -> i32 { 7 }\n",
                3,
                "out: &Path, /* note */) -> i32 { 7 }",
            ),
            (
                "struct Path;\nfn project(\n    out: &Path, /* note */ other: &Path,\n) {}\n",
                3,
                "out: &Path, /* note */ other: &Path,",
            ),
        ] {
            let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), source)?;
            assert!(
                parameter_declaration_shape(&facts, line, text).is_none(),
                "non-parameter or shared line was reclassified: {source}"
            );
        }
        Ok(())
    }

    #[test]
    fn parameter_comment_trivia_preserves_text_and_byte_coordinates() -> Result<(), String> {
        for suffix in [
            ", // output: never executable",
            ", /* output */",
            ", /* outer /* nested */ note */ // tail",
            " /* no comma on last parameter */",
        ] {
            for newline in ["\n", "\r\n"] {
                let text = format!("out: &Path{suffix}");
                let source = format!("// λ\nstruct Path;\nfn project(\n    {text}\n) {{}}\n")
                    .replace('\n', newline);
                let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
                let shape = parameter_declaration_shape(&facts, 4, &text)
                    .ok_or_else(|| format!("commented parameter was lost: {text}"))?;
                assert_eq!(shape.family, ProbeFamily::StaticUnknown);
                assert_eq!(shape.start_line, 4);
                assert_eq!(Some(shape.start_byte), source.find("out: &Path"));
                assert_eq!(shape.text, text);
                assert!(!shape.unsafe_boundary);
            }
        }
        Ok(())
    }
}
