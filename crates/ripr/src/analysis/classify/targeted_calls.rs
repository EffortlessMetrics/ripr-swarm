//! Exact source occurrences for the targeted selector, scoped to one index run.
//! File facts remain line-based for their other consumers. No method receiver
//! identity or macro expansion is inferred here.

use crate::analysis::facts::{RustIndex, TestSummary};
use crate::analysis::syntax::{
    empty_local_macro_invocation_ranges, parse_clean_source_file, ra::LineIndex,
};
use ra_ap_syntax::{
    AstNode, SyntaxNode, TextSize,
    ast::{self, HasGenericArgs, HasName},
};
use std::{cell::RefCell, collections::BTreeMap, ops::Range, path::PathBuf};

/// One parsed call's syntax; unsupported syntax is never receiver authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) enum TargetedCallShape {
    Bare,
    Path { prefix: String, generic: bool },
    Unsupported,
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct TargetedCallOccurrence {
    pub name: String,
    pub text: String,
    pub line: usize,
    /// Exact bytes in the indexed file; absent for an unproven lexical fact.
    pub range: Option<Range<usize>>,
    pub shape: TargetedCallShape,
}

/// Bound to one immutable index; neither parses nor refusals survive this run.
pub(in crate::analysis) struct TargetedCallSyntax<'index> {
    index: &'index RustIndex,
    files: RefCell<BTreeMap<PathBuf, Option<TargetedFile>>>,
}

struct TargetedFile {
    lines: LineIndex,
    functions: BTreeMap<(String, usize, usize), Vec<ast::Fn>>,
    empty_ranges: Vec<Range<usize>>,
    opaque_source: String,
}

impl TargetedFile {
    fn parse(source: &str) -> Option<Self> {
        let parse = parse_clean_source_file(source)?;
        let lines = LineIndex::new(source);
        let empty_ranges = empty_local_macro_invocation_ranges(parse.tree().syntax());
        let opaque_source = crate::analysis::extract::property_macros::mask_evidence_ranges(
            source,
            empty_ranges.iter().cloned(),
        );
        let mut functions: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for function in parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
        {
            if let (Some(name), Some(token)) = (function.name(), function.fn_token()) {
                let key = (
                    name.text().to_string(),
                    lines.line(token.text_range().start()),
                    lines.line_for_range_end(function.syntax().text_range().end()),
                );
                functions.entry(key).or_default().push(function);
            }
        }
        Some(Self {
            lines,
            functions,
            empty_ranges,
            opaque_source: opaque_source.into_owned(),
        })
    }
}

impl<'index> TargetedCallSyntax<'index> {
    pub(in crate::analysis) fn new(index: &'index RustIndex) -> Self {
        Self {
            index,
            files: RefCell::default(),
        }
    }

    pub(in crate::analysis) fn occurrences(
        &self,
        test: &TestSummary,
        source_provenance_admitted: bool,
        macro_unambiguous: impl Fn(usize, &str) -> bool,
    ) -> Vec<TargetedCallOccurrence> {
        let mut occurrences = Vec::new();
        let facts = self.index.files().get(&test.file);
        let mut files = self.files.borrow_mut();
        let cached = files.entry(test.file.clone()).or_insert_with(|| {
            facts
                .filter(|facts| !facts.used_lexical_fallback)
                .and_then(|facts| TargetedFile::parse(&facts.source))
        });
        if source_provenance_admitted && let (Some(facts), Some(cached)) = (facts, cached.as_ref())
        {
            let lines = &cached.lines;
            let empty_ranges = &cached.empty_ranges;
            let opaque_source = &cached.opaque_source;
            let functions = cached
                .functions
                .get(&(test.name.clone(), test.start_line, test.end_line))
                .into_iter()
                .flatten()
                .filter(|function| {
                    function.fn_token().is_some_and(|token| {
                        facts.source.get(
                            usize::from(token.text_range().start())
                                ..usize::from(function.syntax().text_range().end()),
                        ) == Some(test.body.as_str())
                    })
                })
                .collect::<Vec<_>>();
            if let [function] = functions.as_slice()
                && function.syntax().parent().is_some_and(|parent| {
                    crate::analysis::syntax::supported_targeted_item_context(&parent)
                })
            {
                let function = *function;
                collect(
                    function.syntax(),
                    &facts.source,
                    0,
                    lines,
                    test,
                    &mut occurrences,
                );
                // Preserve the producer's documented parameter-input call surface.
                // Recognition alone never clears opacity: this exact attribute must
                // successfully parse and map to the selected function's source.
                let mut mapped_attributes = Vec::new();
                for attribute in function.syntax().descendants().filter_map(ast::Attr::cast) {
                    if !function.param_list().is_some_and(|parameters| {
                        attribute
                            .syntax()
                            .parent()
                            .and_then(ast::Param::cast)
                            .is_some_and(|parameter| {
                                parameter.syntax().parent().as_ref() == Some(parameters.syntax())
                            })
                    }) || !crate::analysis::extract::runs_its_arguments(
                        &attribute.syntax().text().to_string(),
                    ) {
                        continue;
                    }
                    let Some(ast::Meta::TokenTreeMeta(meta)) = attribute.meta() else {
                        continue;
                    };
                    let Some(tree) = meta.token_tree() else {
                        continue;
                    };
                    if collect_operands(
                        &tree,
                        None,
                        &facts.source,
                        opaque_source,
                        lines,
                        test,
                        &mut occurrences,
                    ) {
                        mapped_attributes.push(attribute.syntax().clone());
                    }
                }
                retain_hidden_except(
                    function.syntax(),
                    opaque_source,
                    0,
                    lines,
                    test,
                    &mut occurrences,
                    &mapped_attributes,
                );
                for invocation in function
                    .syntax()
                    .descendants()
                    .filter_map(ast::MacroCall::cast)
                {
                    if empty_ranges.iter().any(|range| {
                        range.contains(&usize::from(invocation.syntax().text_range().start()))
                    }) {
                        continue;
                    }
                    if invocation
                        .syntax()
                        .ancestors()
                        .skip(1)
                        .filter_map(ast::Fn::cast)
                        .next()
                        .as_ref()
                        != Some(function)
                    {
                        continue;
                    }
                    let Some(name) = invocation.path().map(|path| {
                        path.syntax()
                            .descendants_with_tokens()
                            .filter_map(|element| element.into_token())
                            .filter(|token| !token.kind().is_trivia())
                            .map(|token| token.text().to_string())
                            .collect::<String>()
                    }) else {
                        continue;
                    };
                    let line = lines.line(invocation.syntax().text_range().start());
                    let qualified = name.contains("::");
                    let bound = macro_unambiguous(line, &name)
                        && (!qualified
                            || crate::analysis::syntax::ra::assertion_root_is_unrebound(
                                &name,
                                invocation.syntax(),
                                &|other| {
                                    if empty_ranges.iter().any(|range| {
                                        range.contains(&usize::from(
                                            other.syntax().text_range().start(),
                                        ))
                                    }) {
                                        return true;
                                    }
                                    let Some(path) = other.path() else {
                                        return false;
                                    };
                                    let path: String = path
                                        .syntax()
                                        .descendants_with_tokens()
                                        .filter_map(|e| e.into_token())
                                        .filter(|t| !t.kind().is_trivia())
                                        .map(|t| t.text().to_string())
                                        .collect();
                                    let leaf =
                                        crate::analysis::syntax::drop_in_assertion_leaf(&path)
                                            .unwrap_or(&path);
                                    crate::analysis::syntax::TRUSTED_MACRO_NAMES.contains(&leaf)
                                        && macro_unambiguous(
                                            lines.line(other.syntax().text_range().start()),
                                            &path,
                                        )
                                        && (!path.contains("::")
                                            || crate::analysis::syntax::ra::assertion_root_binding_is_unrebound(
                                                &path,
                                                other.syntax(),
                                            ))
                                },
                            ));
                    // Keep the full path through both declaration and source
                    // binding gates before deriving a supported operand shape.
                    let leaf = if qualified && bound {
                        crate::analysis::syntax::drop_in_assertion_leaf(&name).unwrap_or("")
                    } else if !qualified && bound {
                        name.as_str()
                    } else {
                        ""
                    };
                    let operands = match leaf {
                        "assert" => 1,
                        "assert_eq" | "assert_ne" => 2,
                        _ => {
                            opaque(
                                invocation.syntax(),
                                opaque_source,
                                0,
                                lines,
                                test,
                                &mut occurrences,
                            );
                            continue;
                        }
                    };
                    if !bound {
                        opaque(
                            invocation.syntax(),
                            opaque_source,
                            0,
                            lines,
                            test,
                            &mut occurrences,
                        );
                        continue;
                    }
                    let Some(tree) = invocation.token_tree() else {
                        continue;
                    };
                    if !collect_operands(
                        &tree,
                        Some(operands),
                        &facts.source,
                        opaque_source,
                        lines,
                        test,
                        &mut occurrences,
                    ) {
                        opaque(
                            invocation.syntax(),
                            opaque_source,
                            0,
                            lines,
                            test,
                            &mut occurrences,
                        );
                    }
                }
            }
        }
        // Parser absence, opaque macros, generated subjects and unmodeled operands
        // cannot silently lose a call the existing capture reported.
        for call in test.calls.iter().filter(|call| call.name != test.name) {
            if !occurrences
                .iter()
                .any(|site| site.name == call.name && site.line == call.line)
            {
                occurrences.push(TargetedCallOccurrence {
                    name: call.name.clone(),
                    text: call.text.clone(),
                    line: call.line,
                    range: None,
                    shape: TargetedCallShape::Unsupported,
                });
            }
        }
        occurrences.sort_by(|left, right| {
            left.line
                .cmp(&right.line)
                .then(
                    left.range
                        .as_ref()
                        .map(|range| range.start)
                        .cmp(&right.range.as_ref().map(|range| range.start)),
                )
                .then(left.name.cmp(&right.name))
        });
        occurrences
    }
}

/// Parse expression operands without expanding their enclosing macro/attribute.
/// Assertion diagnostics remain opaque; documented parameter inputs use all
/// operands. The same source mapping and capture filter apply to both surfaces.
fn collect_operands(
    tree: &ast::TokenTree,
    operand_limit: Option<usize>,
    source: &str,
    opaque_source: &str,
    lines: &LineIndex,
    test: &TestSummary,
    out: &mut Vec<TargetedCallOccurrence>,
) -> bool {
    let range = tree.syntax().text_range();
    let start = usize::from(range.start()) + 1;
    let end = usize::from(range.end()).saturating_sub(1);
    let Some(tokens) = source.get(start..end) else {
        return false;
    };
    const PREFIX: &str = "fn occurrence(){let _ = (";
    // Preserve source bytes, including trailing comments. Only a top-level
    // comma terminates the operand list; nested token trees remain operands.
    let trailing_comma = tree
        .syntax()
        .children_with_tokens()
        .filter(|element| {
            !element.kind().is_trivia()
                && element.text_range().start() > range.start()
                && element.text_range().end() < range.end()
        })
        .last()
        .is_some_and(|element| element.as_token().is_some_and(|token| token.text() == ","));
    let comma = if trailing_comma { "" } else { "," };
    // A final line comment must not consume the synthetic closing delimiter.
    let wrapped = format!("{PREFIX}{tokens}\n{comma});}}");
    let Some(parse) = parse_clean_source_file(&wrapped) else {
        return false;
    };
    let Some(tuple) = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::TupleExpr::cast)
    else {
        return false;
    };
    if operand_limit.is_some_and(|limit| tuple.fields().count() < limit) {
        return false;
    }
    let Some(shift) = isize::try_from(start)
        .ok()
        .zip(isize::try_from(PREFIX.len()).ok())
        .and_then(|(start, prefix)| start.checked_sub(prefix))
    else {
        return false;
    };
    for (position, operand) in tuple.fields().enumerate() {
        if operand_limit.is_some_and(|limit| position >= limit) {
            opaque(operand.syntax(), opaque_source, shift, lines, test, out);
            continue;
        }
        collect(operand.syntax(), source, shift, lines, test, out);
        retain_hidden(operand.syntax(), opaque_source, shift, lines, test, out);
        for nested in operand
            .syntax()
            .descendants()
            .filter_map(ast::MacroCall::cast)
        {
            opaque(nested.syntax(), opaque_source, shift, lines, test, out);
        }
    }
    true
}

fn collect(
    root: &SyntaxNode,
    source: &str,
    shift: isize,
    lines: &LineIndex,
    test: &TestSummary,
    out: &mut Vec<TargetedCallOccurrence>,
) {
    for node in root.descendants() {
        if node
            .ancestors()
            .take_while(|node| node != root)
            .skip(1)
            .any(|node| ast::Fn::can_cast(node.kind()) || ast::MacroCall::can_cast(node.kind()))
        {
            continue;
        }
        let (name, name_start, shape) = if let Some(call) = ast::CallExpr::cast(node.clone()) {
            let Some(ast::Expr::PathExpr(expression)) = call.expr() else {
                continue;
            };
            let Some(path) = expression.path() else {
                continue;
            };
            let Some(segment) = path.segment() else {
                continue;
            };
            let Some(name) = segment.name_ref() else {
                continue;
            };
            let spelling = name.text().to_string();
            // Terminal type arguments instantiate the already-bound callable.
            // Generic qualifiers still require authority this selector lacks.
            let generic = path.qualifier().is_some_and(|prefix| {
                prefix
                    .segments()
                    .any(|segment| segment.generic_arg_list().is_some())
            });
            let raw_or_anchored = path.segments().any(|segment| {
                segment.type_anchor().is_some()
                    || segment
                        .name_ref()
                        .is_none_or(|name| name.text().starts_with("r#"))
            });
            let shape = if raw_or_anchored
                || path
                    .syntax()
                    .text()
                    .to_string()
                    .trim_start()
                    .starts_with("::")
            {
                TargetedCallShape::Unsupported
            } else if let Some(prefix) = path.qualifier() {
                TargetedCallShape::Path {
                    prefix: prefix
                        .segments()
                        .filter_map(|segment| {
                            segment.name_ref().map(|name| name.text().to_string())
                        })
                        .collect::<Vec<_>>()
                        .join("::"),
                    generic,
                }
            } else {
                TargetedCallShape::Bare
            };
            (
                spelling.trim_start_matches("r#").to_string(),
                name.syntax().text_range().start(),
                shape,
            )
        } else if let Some(call) = ast::MethodCallExpr::cast(node.clone()) {
            let Some(name) = call.name_ref() else {
                continue;
            };
            (
                name.text().trim_start_matches("r#").to_string(),
                name.syntax().text_range().start(),
                TargetedCallShape::Unsupported,
            )
        } else {
            continue;
        };
        let Some(range) = mapped_range(&node, shift) else {
            continue;
        };
        let Some(name_offset) =
            shifted(name_start, shift).and_then(|offset| u32::try_from(offset).ok())
        else {
            continue;
        };
        let line = lines.line(TextSize::from(name_offset));
        if name == test.name
            || !test
                .calls
                .iter()
                .any(|call| call.name == name && call.line == line)
        {
            continue;
        }
        let Some(text) = source.get(range.clone()) else {
            continue;
        };
        out.push(TargetedCallOccurrence {
            name,
            text: text.to_string(),
            line,
            range: Some(range),
            shape,
        });
    }
}

/// Opaque token text can never lend an exact parsed occurrence. Preserve every
/// existing captured name even when an established same-name call shares its line.
fn opaque(
    node: &SyntaxNode,
    source: &str,
    shift: isize,
    lines: &LineIndex,
    test: &TestSummary,
    out: &mut Vec<TargetedCallOccurrence>,
) {
    let Some(range) = mapped_range(node, shift) else {
        return;
    };
    let Some(text) = source.get(range.clone()) else {
        return;
    };
    let Ok(start) = u32::try_from(range.start) else {
        return;
    };
    for call in
        crate::analysis::extract::extract_call_facts(text, lines.line(TextSize::from(start)))
    {
        if call.name != test.name
            && test
                .calls
                .iter()
                .any(|fact| fact.name == call.name && fact.line == call.line)
        {
            out.push(TargetedCallOccurrence {
                name: call.name,
                text: call.text,
                line: call.line,
                range: None,
                shape: TargetedCallShape::Unsupported,
            });
        }
    }
}

fn shifted(offset: TextSize, shift: isize) -> Option<usize> {
    isize::try_from(usize::from(offset))
        .ok()?
        .checked_add(shift)
        .and_then(|offset| usize::try_from(offset).ok())
}

fn mapped_range(node: &SyntaxNode, shift: isize) -> Option<Range<usize>> {
    Some(shifted(node.text_range().start(), shift)?..shifted(node.text_range().end(), shift)?)
}

fn retain_hidden(
    root: &SyntaxNode,
    source: &str,
    shift: isize,
    lines: &LineIndex,
    test: &TestSummary,
    out: &mut Vec<TargetedCallOccurrence>,
) {
    retain_hidden_except(root, source, shift, lines, test, out, &[]);
}

fn retain_hidden_except(
    root: &SyntaxNode,
    source: &str,
    shift: isize,
    lines: &LineIndex,
    test: &TestSummary,
    out: &mut Vec<TargetedCallOccurrence>,
    mapped_attributes: &[SyntaxNode],
) {
    for node in root.descendants().filter(|node| node != root) {
        if ast::Fn::can_cast(node.kind())
            || (ast::Attr::can_cast(node.kind()) && !mapped_attributes.contains(&node))
            || ast::MacroRules::can_cast(node.kind())
            || ast::MacroDef::can_cast(node.kind())
        {
            opaque(&node, source, shift, lines, test, out);
        }
    }
}
