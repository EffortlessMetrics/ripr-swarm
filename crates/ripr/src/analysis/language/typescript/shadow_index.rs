//! Per-body shadow index (#10925). `local_identifier_declared_in_test_body`
//! is asked about every candidate name of a test body (about 20k calls from
//! 200 bodies on zod). The per-identifier answer walks the body with the
//! conservative lexer and masks its literals again for each name. The index
//! does that work ONCE per body, identifier-independently, and answers each
//! name by lookup:
//!
//! - the lexical walk (`walk_body_lexical`, the same walk the per-identifier
//!   scan runs) records every declared name with its scope and every plain-code
//!   identifier token with its offsets;
//! - the #7507 whole-body binding shapes (catch / arrow / function parameters
//!   and `for` headers) are collected as the identifier-independent regions the
//!   per-identifier scans search (`extract_binding_regions`), and each name is
//!   tested against only the regions that spell it.
//!
//! Every per-name predicate is the same function the per-identifier path calls
//! (`params_bind_identifier`, `header_declares_identifier`,
//! `ts_references_identifier`, `declaration_scope_binds`), so the answers are
//! identical by construction; the differential tests pin that. The index
//! only answers names `is_safe_javascript_identifier` accepts (the unsafe
//! ones keep the exact per-identifier path), and it lives in the per-run
//! resolver cache, so it never outlives the analysis run that built it.

use super::oracle::is_safe_javascript_identifier;
use super::related_tests::{
    BodyWalkEvent, declaration_line_declarators, declaration_scope_binds, has_member_call_boundary,
    header_declares_identifier, is_javascript_identifier_char, is_object_literal_key,
    params_bind_identifier, walk_body_lexical,
};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
thread_local! {
    /// Count of per-body index builds (#10925): a body is walked once per run
    /// no matter how many names are asked about it.
    pub(crate) static SHADOW_INDEX_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// One line-leading declaration of a name.
struct DeclaredName {
    name_start: usize,
    scope: Option<usize>,
}

/// The identifier-independent regions the #7507 binding scans search.
#[derive(Default)]
struct BindingRegions {
    /// Text between the parens of each `catch (...)`.
    catch_clauses: Vec<String>,
    /// Parameter lists feeding `=>` or following `function`.
    parameter_lists: Vec<String>,
    /// Single-parameter arrows `name =>` that bind `name`.
    bare_arrow_params: HashSet<String>,
    /// Text between the parens of each `for (...)` / `for await (...)`.
    loop_headers: Vec<String>,
}

pub(crate) struct BodyShadowIndex {
    scopes: Vec<(usize, usize)>,
    declarations: HashMap<String, Vec<DeclaredName>>,
    /// Plain-code identifier tokens at a member-call boundary, by name, as
    /// `(start, end)` offsets.
    tokens: HashMap<String, Vec<(usize, usize)>>,
    regions: BindingRegions,
}

impl BodyShadowIndex {
    pub(crate) fn build(body_text: &str) -> Self {
        #[cfg(test)]
        SHADOW_INDEX_BUILDS.with(|builds| builds.set(builds.get() + 1));
        let mut declarations: HashMap<String, Vec<DeclaredName>> = HashMap::new();
        let mut tokens: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
        let scopes = walk_body_lexical(body_text, |event| match event {
            BodyWalkEvent::DeclarationLine {
                line,
                line_start,
                scope,
            } => {
                for (name, offset) in declaration_line_declarators(line) {
                    declarations
                        .entry(name.to_string())
                        .or_default()
                        .push(DeclaredName {
                            name_start: line_start + offset,
                            scope,
                        });
                }
            }
            BodyWalkEvent::CodePosition { idx } => {
                // A use of `name` starts here exactly when the maximal
                // identifier run at `idx` is `name` and the module's member
                // boundary holds: the per-identifier scan's `starts_with` +
                // boundary + next-char checks, for every name at once.
                if !has_member_call_boundary(body_text, idx) {
                    return;
                }
                let run_len: usize = body_text[idx..]
                    .chars()
                    .take_while(|ch| is_javascript_identifier_char(*ch))
                    .map(char::len_utf8)
                    .sum();
                if run_len > 0 {
                    tokens
                        .entry(body_text[idx..idx + run_len].to_string())
                        .or_default()
                        .push((idx, idx + run_len));
                }
            }
        });
        let masked = super::classifier::ts_mask_literals_and_comments(body_text);
        Self {
            scopes,
            declarations,
            tokens,
            regions: extract_binding_regions(&masked),
        }
    }

    /// Whether the name is shadowed in `body_text` (the text the index was
    /// built from), or `None` when the name is outside the index's exact
    /// domain and the caller must run the per-identifier path.
    pub(crate) fn shadows(&self, body_text: &str, identifier: &str) -> Option<bool> {
        if !is_safe_javascript_identifier(identifier) {
            return None;
        }
        Some(self.lexically_shadows(body_text, identifier) || self.binds_shadow(identifier))
    }

    fn lexically_shadows(&self, body_text: &str, identifier: &str) -> bool {
        let Some(declarations) = self.declarations.get(identifier) else {
            return false;
        };
        let Some(tokens) = self.tokens.get(identifier) else {
            return false;
        };
        tokens
            .iter()
            .filter(|(start, end)| {
                // The declaration's own name token is not a use of it.
                !declarations
                    .iter()
                    .any(|declaration| declaration.name_start == *start)
                    && !is_object_literal_key(body_text, *start, *end)
            })
            .any(|(start, _)| {
                declarations.iter().any(|declaration| {
                    declaration_scope_binds(
                        declaration
                            .scope
                            .and_then(|frame| self.scopes.get(frame).copied()),
                        *start,
                    )
                })
            })
    }

    fn binds_shadow(&self, identifier: &str) -> bool {
        let regions = &self.regions;
        regions.bare_arrow_params.contains(identifier)
            || regions.catch_clauses.iter().any(|clause| {
                clause.contains(identifier)
                    && super::classifier::ts_references_identifier(clause, identifier)
            })
            || regions
                .parameter_lists
                .iter()
                .any(|list| list.contains(identifier) && params_bind_identifier(list, identifier))
            || regions.loop_headers.iter().any(|header| {
                header.contains(identifier) && header_declares_identifier(header, identifier)
            })
    }
}

/// Balanced `(`..`)` content at the start of `paren` (the text after an
/// opening paren), or `None` when it never closes.
fn balanced_paren_content(paren: &str) -> Option<&str> {
    let mut depth = 1usize;
    for (offset, c) in paren.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&paren[..offset]);
                }
            }
            _ => {}
        }
    }
    None
}

fn ident_boundary_before(masked: &str, at: usize) -> bool {
    masked[..at]
        .chars()
        .next_back()
        .is_none_or(|ch| !is_javascript_identifier_char(ch))
}

/// Collect, in one pass per keyword, the regions the per-identifier
/// `body_has_catch_binding`, `body_has_arrow_or_function_param_binding` and
/// `body_has_loop_header_binding` scans test a name against. The filters here
/// are exactly those scans' identifier-independent conditions.
fn extract_binding_regions(masked: &str) -> BindingRegions {
    let mut regions = BindingRegions::default();
    // `catch (...)`.
    let mut from = 0usize;
    while let Some(rel) = masked[from..].find("catch") {
        let at = from + rel;
        from = at + "catch".len();
        if !ident_boundary_before(masked, at) {
            continue;
        }
        let rest = masked[from..].trim_start();
        if let Some(content) = rest.strip_prefix('(').and_then(balanced_paren_content) {
            regions.catch_clauses.push(content.to_string());
        }
    }
    // Bare single-parameter arrow `name =>`: the identifier run directly
    // before ` =>` binds unless it is a type-space use.
    let mut from = 0usize;
    while let Some(rel) = masked[from..].find(" =>") {
        let space = from + rel;
        from = space + " =>".len();
        let head = &masked[..space];
        let name_len: usize = head
            .chars()
            .rev()
            .take_while(|ch| is_javascript_identifier_char(*ch))
            .map(char::len_utf8)
            .sum();
        if name_len == 0 {
            continue;
        }
        let start = space - name_len;
        let before = &masked[..start];
        if before
            .chars()
            .next_back()
            .is_some_and(|ch| is_javascript_identifier_char(ch) || ch == '.')
        {
            continue;
        }
        // `: typeof price => x` spells a type-space use, not a parameter.
        let word: String = before
            .chars()
            .rev()
            .skip_while(|ch: &char| ch.is_whitespace())
            .take_while(|ch: &char| is_javascript_identifier_char(*ch))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if matches!(word.as_str(), "typeof" | "keyof") {
            continue;
        }
        if before.chars().rev().find(|ch: &char| !ch.is_whitespace()) == Some(':') {
            continue;
        }
        regions
            .bare_arrow_params
            .insert(masked[start..space].to_string());
    }
    // Parenthesized parameter list directly feeding `=>`.
    let mut from = 0usize;
    while let Some(rel) = masked[from..].find("=>") {
        let at = from + rel;
        from = at + "=>".len();
        let before = masked[..at].trim_end();
        let Some(open) = before.rfind('(') else {
            continue;
        };
        let Some(params) = balanced_paren_content(&before[open + 1..]) else {
            continue;
        };
        let close = open + 1 + params.len();
        let between = before[close + 1..].trim_start();
        // The `(` must open a parameter list, not a call.
        let paren_before = before[..open].trim_end().chars().next_back();
        if (between.is_empty() || between.starts_with(':'))
            && !params.contains(';')
            && paren_before.is_none_or(|ch| {
                !is_javascript_identifier_char(ch) && ch != '.' && ch != ')' && ch != ']'
            })
        {
            regions.parameter_lists.push(params.to_string());
        }
    }
    // `function` keyword parameter lists.
    let mut from = 0usize;
    while let Some(rel) = masked[from..].find("function") {
        let at = from + rel;
        from = at + "function".len();
        if !ident_boundary_before(masked, at) {
            continue;
        }
        let mut rest = masked[from..].trim_start();
        if let Some(stripped) = rest.strip_prefix('*') {
            rest = stripped.trim_start();
        }
        if let Some(first) = rest.chars().next()
            && (first.is_ascii_alphabetic() || first == '_' || first == '$')
        {
            let name_len: usize = rest
                .char_indices()
                .take_while(|(_, ch)| is_javascript_identifier_char(*ch))
                .map(|(i, ch)| i + ch.len_utf8())
                .last()
                .unwrap_or(0);
            rest = rest[name_len..].trim_start();
        }
        if let Some(content) = rest.strip_prefix('(').and_then(balanced_paren_content) {
            regions.parameter_lists.push(content.to_string());
        }
    }
    // `for (...)` / `for await (...)`.
    let mut from = 0usize;
    while let Some(rel) = masked[from..].find("for") {
        let at = from + rel;
        from = at + "for".len();
        let after_ok = masked[from..]
            .chars()
            .next()
            .is_none_or(|ch| !is_javascript_identifier_char(ch));
        if !ident_boundary_before(masked, at) || !after_ok {
            continue;
        }
        let mut rest = masked[from..].trim_start();
        if let Some(after_await) = rest.strip_prefix("await") {
            if after_await
                .chars()
                .next()
                .is_none_or(|ch| !is_javascript_identifier_char(ch))
            {
                rest = after_await.trim_start();
            } else {
                continue;
            }
        }
        if let Some(content) = rest.strip_prefix('(').and_then(balanced_paren_content) {
            regions.loop_headers.push(content.to_string());
        }
    }
    regions
}
