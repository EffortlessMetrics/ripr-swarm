//! Parser-owned regex and JSX spans for the shared literal masker (#9936).

use super::{Allocator, GetSpan, Parser, SourceType};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeValue, JSXClosingElement, JSXElement, JSXFragment,
    JSXOpeningElement, JSXText, RegExpLiteral,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::Span;
use std::collections::BTreeSet;

pub(super) struct Prepared {
    pub(super) code: String,
    pub(super) operand_ends: BTreeSet<usize>,
    pub(super) regex_known: bool,
}

#[derive(PartialEq, Eq)]
struct Plan {
    blank: Vec<bool>,
    operand_ends: BTreeSet<usize>,
}

impl Plan {
    fn set(&mut self, span: Span, blank: bool) {
        if let Some(bytes) = self.blank.get_mut(span.start as usize..span.end as usize) {
            bytes.fill(blank);
        }
    }

    fn finish_operand(&mut self, span: Span) {
        if span.end > span.start {
            self.operand_ends.insert(span.end as usize - 1);
        }
    }
}

impl<'a> Visit<'a> for Plan {
    fn visit_reg_exp_literal(&mut self, node: &RegExpLiteral<'a>) {
        let close = node.span.start + 1 + node.regex.pattern.text.len() as u32;
        self.set(Span::new(node.span.start + 1, close), true);
        self.set(Span::new(close + 1, node.span.end), true);
        self.finish_operand(node.span);
    }

    fn visit_jsx_element(&mut self, node: &JSXElement<'a>) {
        self.finish_operand(node.span);
        walk::walk_jsx_element(self, node);
    }

    fn visit_jsx_opening_element(&mut self, node: &JSXOpeningElement<'a>) {
        self.set(node.span, true);
        self.set(node.name.span(), false);
        for attribute in &node.attributes {
            match attribute {
                JSXAttributeItem::Attribute(attribute) => {
                    if let Some(value) = &attribute.value
                        && !matches!(value, JSXAttributeValue::StringLiteral(_))
                    {
                        self.set(value.span(), false);
                    }
                }
                JSXAttributeItem::SpreadAttribute(attribute) => self.set(attribute.span, false),
            }
        }
        // Restore executable containers before walking them: nested JSX and
        // regex masks must override the enclosing attribute's restoration.
        walk::walk_jsx_opening_element(self, node);
    }

    fn visit_jsx_closing_element(&mut self, node: &JSXClosingElement<'a>) {
        self.set(node.span, true);
        self.set(node.name.span(), false);
    }

    fn visit_jsx_fragment(&mut self, node: &JSXFragment<'a>) {
        self.set(node.opening_fragment.span, true);
        self.set(node.closing_fragment.span, true);
        self.finish_operand(node.span);
        walk::walk_jsx_fragment(self, node);
    }

    fn visit_jsx_text(&mut self, node: &JSXText<'a>) {
        self.set(node.span, true);
    }
}

fn parse_plan(text: &str, source_type: SourceType, prefix: &str, suffix: &str) -> Option<Plan> {
    let source = format!("{prefix}{text}{suffix}");
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source, source_type).parse();
    if !parsed.errors.is_empty() {
        return None;
    }
    let mut plan = Plan {
        blank: vec![false; source.len()],
        operand_ends: BTreeSet::new(),
    };
    plan.visit_program(&parsed.program);
    let start = prefix.len();
    let end = start + text.len();
    Some(Plan {
        blank: plan.blank[start..end].to_vec(),
        operand_ends: plan
            .operand_ends
            .range(start..end)
            .map(|at| at - start)
            .collect(),
    })
}

fn plan_for(text: &str, source_type: Option<SourceType>) -> Option<Plan> {
    for (prefix, suffix) in [
        ("", ""),
        ("async function* __ripr_mask_body__() {\n", "\n}"),
        ("class __RiprMaskOwner__ {\n", "\n}"),
    ] {
        if let Some(source_type) = source_type {
            if let Some(plan) = parse_plan(text, source_type, prefix, suffix) {
                return Some(plan);
            }
        } else {
            match (
                parse_plan(text, SourceType::ts(), prefix, suffix),
                parse_plan(text, SourceType::tsx(), prefix, suffix),
            ) {
                (Some(ts), Some(tsx)) => return (ts == tsx).then_some(ts),
                (Some(plan), None) | (None, Some(plan)) => return Some(plan),
                (None, None) => {}
            }
        }
    }
    None
}

pub(super) fn prepare(text: &str, source_type: Option<SourceType>) -> Prepared {
    let plan =
        (text.contains('`') || text.contains("</") || text.contains("/>") || text.contains("<>"))
            .then(|| plan_for(text, source_type))
            .flatten();
    let Some(plan) = plan else {
        return Prepared {
            code: text.to_string(),
            operand_ends: BTreeSet::new(),
            regex_known: false,
        };
    };
    let mut prepared = Prepared {
        code: String::with_capacity(text.len()),
        operand_ends: BTreeSet::new(),
        regex_known: true,
    };
    for (at, c) in text.char_indices() {
        if plan.operand_ends.contains(&at) {
            prepared.operand_ends.insert(prepared.code.len());
        }
        prepared.code.push(if plan.blank[at] && !c.is_whitespace() {
            ' '
        } else {
            c
        });
    }
    prepared
}
