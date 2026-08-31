//! Repository-governed Rust test-harness registry (#3532).
//!
//! This module is the one authority that turns exact
//! `[analysis.test_harnesses]` registrations into typed source facts. It
//! runs once per index build, after the parameterized-test promotion and
//! the test-styles normalizer, so every downstream consumer (diff probes,
//! seam inventory, evidence relation, test selectors, projections) reads
//! the same derived facts.
//!
//! The authority model keeps the required distinctions apart:
//!
//! - a registered custom target is evidence-role, but NOT every function
//!   in it is an executable test — members carry
//!   [`FunctionSourceRole::HarnessHelper`] and only adapter-established
//!   subjects register `TestFact`s;
//! - a registered macro/attribute invocation is a test container, but
//!   RIPR did not expand every generated case — subjects state their
//!   [`HarnessSubjectClaim`];
//! - a known selector route ([`HarnessSelectorCapability::NamedUnexecuted`])
//!   is not a selector that ran — passive analysis never starts Cargo or
//!   a harness;
//! - unregistered or ambiguous harnesses become typed limitations —
//!   never production reclassification and never executable-test
//!   optimism.
//!
//! Everything here is exact-match on configured inputs. There is no
//! inference from filenames, crate imports, macro suffixes, or function
//! names, and a registration that names a file outside the analyzed
//! index simply grants nothing (fail closed).

use super::model::{FunctionFact, FunctionSourceRole, RustIndex, TestFact};
use super::test_styles::normalized_test_attribute_path as normalized_attribute_path;
use super::{
    HarnessLimitationFact, HarnessSelectorCapability, HarnessSubjectClaim, HarnessSubjectFact,
};
use crate::analysis::rust_index::{
    OracleFact, classify_assertion, extract_call_facts, extract_identifier_tokens,
    extract_literal_facts,
};
use crate::analysis::syntax::ra::{LineIndex, parser_oracles_for_function, slice_text};
use crate::config::{TestHarnessAdapter, TestHarnessKind, TestHarnessRegistration};
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, Edition, SourceFile, TextSize};
use std::collections::BTreeSet;
use std::path::Path;

/// Apply every registration whose target file is present in the index.
/// Registrations for files outside the analyzed set grant nothing.
///
/// Empty registrations leave the index untouched (no-op), so
/// repositories without registrations keep every existing output.
pub(super) fn apply_registrations(
    index: &mut RustIndex,
    registrations: &[TestHarnessRegistration],
    production_like_targets: &std::collections::BTreeSet<std::path::PathBuf>,
) {
    if registrations.is_empty() {
        return;
    }
    let mut subjects = Vec::new();
    let mut limitations = Vec::new();
    for registration in registrations {
        // Exact target identity only: a registration whose file is not in
        // this index (stale, wrong package, unanalyzed scope) applies to
        // nothing here.
        let Some(facts) = index.files.get(&registration.target) else {
            continue;
        };
        let source = facts.source.clone();
        match (registration.kind, registration.adapter) {
            (TestHarnessKind::CustomHarnessTarget, TestHarnessAdapter::LibtestMimicV1) => {
                if production_like_targets.contains(&registration.target) {
                    continue;
                }
                apply_libtest_mimic_target(
                    index,
                    registration,
                    &source,
                    &mut subjects,
                    &mut limitations,
                )
            }
            (TestHarnessKind::RegisteredAttribute, TestHarnessAdapter::ExactAttributeV1) => {
                if production_like_targets.contains(&registration.target) {
                    continue;
                }
                apply_registered_attribute(
                    index,
                    registration,
                    &source,
                    &mut subjects,
                    &mut limitations,
                )
            }
            // The config parser rejects unknown kind/adapter pairs, so this
            // arm is unreachable today; if it ever becomes reachable the
            // safe behavior is to grant nothing.
            _ => {}
        }
    }
    subjects.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.start_line.cmp(&right.start_line))
            .then(left.end_line.cmp(&right.end_line))
            .then(left.name.cmp(&right.name))
    });
    limitations.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.line.cmp(&right.line))
            .then(left.code.cmp(&right.code))
            .then(left.detail.cmp(&right.detail))
    });
    limitations.dedup();
    index.harness_subjects = subjects;
    index.harness_limitations = limitations;
}

/// libtest-mimic adapter, generation 1 (#3532).
///
/// Establishes one subject per exact source-visible trial registration
/// with a stable string-literal name (`Trial::test("name", ...)`), where
/// the `Trial` path is anchored to the registered marker crate path —
/// either spelled fully qualified (`<marker>::Trial::test`) or bound by a
/// top-level `use` whose path resolves to `<marker>::Trial`. Discovery
/// runs over the file's token stream, so trial registrations inside
/// collection macros (`vec![...]`) are still exact token matches, while
/// everything the issue names as a limitation stays one:
///
/// - dynamic (non-literal) trial names;
/// - trial registration inside a loop (runtime-only discovery);
/// - unanchored or ambiguously imported `Trial` paths;
/// - duplicate trial names (two subject claims on one identity).
fn apply_libtest_mimic_target(
    index: &mut RustIndex,
    registration: &TestHarnessRegistration,
    source: &str,
    subjects: &mut Vec<HarnessSubjectFact>,
    limitations: &mut Vec<HarnessLimitationFact>,
) {
    let target = registration.target.clone();
    demote_harness_target_functions(index, &target);
    let parse = SourceFile::parse(source, Edition::CURRENT);
    if !parse.errors().is_empty() {
        limitations.push(HarnessLimitationFact {
            registration_id: registration.registration_id.clone(),
            code: "parse_unavailable".to_string(),
            file: target,
            line: 1,
            detail: "the registered harness target did not parse; no trial subjects were established (fail closed)".to_string(),
        });
        return;
    }
    let line_index = LineIndex::new(source);
    let trial_bindings = top_level_use_bindings(parse.tree().syntax(), "Trial");
    let tokens: Vec<ra_ap_syntax::SyntaxToken> = parse
        .tree()
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .collect();
    let mut claimed_names = BTreeSet::new();

    for position in 0..tokens.len() {
        let Some(matched) = match_trial_path(&tokens, position, &registration.marker) else {
            continue;
        };
        // Macro definitions are templates, not executable registrations. Do
        // this before path/import/loop diagnostics so dormant templates do
        // not emit limitations either.
        if in_macro_definition(tokens[position].parent_ancestors()) {
            continue;
        }
        let line = line_index.line(tokens[position].text_range().start());
        // Anchoring: the bare `Trial` form needs a top-level import bound
        // from exactly the marker path; the qualified form carries the
        // marker in the path itself.
        if !matched.qualified {
            match resolve_trial_binding(&trial_bindings, &registration.marker) {
                TrialBindingResolution::MarkerAnchored => {}
                TrialBindingResolution::Ambiguous { conflicting } => {
                    limitations.push(HarnessLimitationFact {
                        registration_id: registration.registration_id.clone(),
                        code: "ambiguous_import".to_string(),
                        file: target.clone(),
                        line,
                        detail: format!(
                            "bare `Trial::test` cannot be tied to marker `{}`; conflicting imports bind Trial from ({conflicting})",
                            registration.marker
                        ),
                    });
                    continue;
                }
                TrialBindingResolution::Unbound => {
                    limitations.push(HarnessLimitationFact {
                        registration_id: registration.registration_id.clone(),
                        code: "unanchored_trial_path".to_string(),
                        file: target.clone(),
                        line,
                        detail: format!(
                            "`Trial::test` has no top-level import bound from marker `{}`; the invocation is not classified",
                            registration.marker
                        ),
                    });
                    continue;
                }
            }
        }

        if in_loop(tokens[position].parent_ancestors()) {
            limitations.push(HarnessLimitationFact {
                registration_id: registration.registration_id.clone(),
                code: "dynamic_trial_registration".to_string(),
                file: target.clone(),
                line,
                detail: "trial registration runs inside a loop; runtime-discovered trials remain unresolved".to_string(),
            });
            continue;
        }
        let name_token_index = next_significant(&tokens, matched.name_token_index);
        let Some(name_token) = name_token_index.and_then(|index| tokens.get(index)) else {
            continue;
        };
        let Some(name) = string_literal_token_text(name_token) else {
            limitations.push(HarnessLimitationFact {
                registration_id: registration.registration_id.clone(),
                code: "dynamic_trial_name".to_string(),
                file: target.clone(),
                line: line_index.line(name_token.text_range().start()),
                detail:
                    "trial name is not a simple string literal; generated names remain unresolved"
                        .to_string(),
            });
            continue;
        };
        if !claimed_names.insert(name.clone()) {
            limitations.push(HarnessLimitationFact {
                registration_id: registration.registration_id.clone(),
                code: "duplicate_subject".to_string(),
                file: target.clone(),
                line,
                detail: format!(
                    "trial name `{name}` is registered more than once; conflicting subject claims fail closed"
                ),
            });
            subjects.retain(|subject| {
                !(subject.registration_id == registration.registration_id && subject.name == name)
            });
            remove_file_test(index, &target, &name);
            index
                .tests
                .retain(|test| !(test.file == target && test.name == name));
            continue;
        }
        // The subject span runs from the trial path to the balanced
        // closing parenthesis of the registration call.
        let Some(end_offset) = balanced_call_end(&tokens, matched.open_paren_index) else {
            continue;
        };
        let callback = match trial_callback_name(&tokens, matched.name_token_index) {
            Some(callback_name) => match unique_callback_function(index, &target, &callback_name) {
                Some(function) => Some(function),
                None => {
                    limitations.push(HarnessLimitationFact {
                        registration_id: registration.registration_id.clone(),
                        code: "unresolved_trial_callback".to_string(),
                        file: target.clone(),
                        line,
                        detail: format!(
                            "trial callback `{callback_name}` is not a unique function item in the registered target; subject evidence remains unresolved"
                        ),
                    });
                    None
                }
            },
            None => None,
        };
        let start: TextSize = tokens[position].text_range().start();
        let start_line = line_index.line(start);
        let end_line = line_index.line_for_range_end(TextSize::new(end_offset));
        let body = slice_text(source, start, TextSize::new(end_offset));
        let (calls, literals, assertions) = match callback.as_ref() {
            Some(function) => (
                function.calls.clone(),
                function.literals.clone(),
                parser_oracles_for_function(&function.body, function.start_line).unwrap_or_else(
                    || {
                        parser_oracles_for_node_tokens(
                            &tokens,
                            name_token_index.unwrap_or(matched.name_token_index),
                            &line_index,
                        )
                    },
                ),
            ),
            None => (
                extract_call_facts(&body, start_line),
                extract_literal_facts(&body, start_line),
                parser_oracles_for_node_tokens(&tokens, matched.name_token_index, &line_index),
            ),
        };
        let subject = HarnessSubjectFact {
            registration_id: registration.registration_id.clone(),
            harness_kind: registration.kind.as_str().to_string(),
            adapter: registration.adapter.as_str().to_string(),
            marker: registration.marker.clone(),
            name: name.clone(),
            file: target.clone(),
            start_line,
            end_line,
            body: body.clone(),
            calls: calls.clone(),
            assertions: assertions.clone(),
            literals: literals.clone(),
            selector: HarnessSelectorCapability::NamedUnexecuted,
            claim: HarnessSubjectClaim::NamedInvocation,
            provenance: TestHarnessRegistration::provenance().to_string(),
        };
        let test = TestFact {
            name,
            file: target.clone(),
            start_line,
            end_line,
            body,
            calls,
            assertions,
            literals,
            attrs: Vec::new(),
        };
        subjects.push(subject);
        push_file_test(index, &target, test.clone());
        index.tests.push(test);
    }
}

/// The exact token shapes this adapter matches at `position`:
/// `Trial :: test (` (bare, import-anchored) or
/// `<marker>::Trial :: test (` (qualified). Path separators count as
/// either one `::` token or two `:` tokens — inside macro token trees
/// (`vec![...]`) the raw punctuation form is what the tokenizer emits.
/// Returns the index of the first argument token and whether the
/// qualified form matched.
fn match_trial_path(
    tokens: &[ra_ap_syntax::SyntaxToken],
    position: usize,
    marker: &str,
) -> Option<TrialPathMatch> {
    let text = |index: usize| tokens.get(index).map(|token| token.text());
    let is_ident_eq =
        |index: usize, expected: &str| text(index).is_some_and(|value| value == expected);
    let l_paren = |index: usize| {
        tokens
            .get(index)
            .is_some_and(|token| token.kind() == ra_ap_syntax::SyntaxKind::L_PAREN)
    };
    // Length of the path separator starting at `index`: 1 for `::`, 2 for
    // `:` `:`, or None.
    fn separator(tokens: &[ra_ap_syntax::SyntaxToken], index: usize) -> Option<usize> {
        match tokens.get(index).map(|token| token.kind()) {
            Some(ra_ap_syntax::SyntaxKind::COLON2) => Some(1),
            Some(ra_ap_syntax::SyntaxKind::COLON)
                if tokens
                    .get(index + 1)
                    .is_some_and(|token| token.kind() == ra_ap_syntax::SyntaxKind::COLON) =>
            {
                Some(2)
            }
            _ => None,
        }
    }

    for qualified in [false, true] {
        if (!qualified && !bare_path_starts_at_boundary(tokens, position))
            || (qualified && !qualified_path_starts_at_boundary(tokens, position))
        {
            continue;
        }
        let mut segments: Vec<&str> = if qualified {
            marker.split("::").collect()
        } else {
            Vec::new()
        };
        segments.push("Trial");
        segments.push("test");
        let mut cursor = position;
        let mut matched_segments = 0usize;
        for segment in &segments {
            if !is_ident_eq(cursor, segment) {
                break;
            }
            cursor += 1;
            matched_segments += 1;
            if matched_segments < segments.len() {
                let Some(width) = separator(tokens, cursor) else {
                    break;
                };
                cursor += width;
            }
        }
        if matched_segments == segments.len() && l_paren(cursor) {
            return Some(TrialPathMatch {
                qualified,
                name_token_index: cursor + 1,
                open_paren_index: cursor,
            });
        }
    }
    None
}

fn bare_path_starts_at_boundary(tokens: &[ra_ap_syntax::SyntaxToken], position: usize) -> bool {
    let mut cursor = position;
    while cursor > 0 {
        cursor -= 1;
        if matches!(
            tokens[cursor].kind(),
            ra_ap_syntax::SyntaxKind::WHITESPACE | ra_ap_syntax::SyntaxKind::COMMENT
        ) {
            continue;
        }
        return match tokens[cursor].kind() {
            ra_ap_syntax::SyntaxKind::COLON2 => false,
            ra_ap_syntax::SyntaxKind::COLON => {
                // A single colon can be a record-field initializer; a
                // doubled colon is always a qualified path separator.
                let mut previous = cursor;
                while previous > 0 {
                    previous -= 1;
                    if matches!(
                        tokens[previous].kind(),
                        ra_ap_syntax::SyntaxKind::WHITESPACE | ra_ap_syntax::SyntaxKind::COMMENT
                    ) {
                        continue;
                    }
                    return tokens[previous].kind() != ra_ap_syntax::SyntaxKind::COLON;
                }
                true
            }
            _ => true,
        };
    }
    true
}

fn qualified_path_starts_at_boundary(
    tokens: &[ra_ap_syntax::SyntaxToken],
    position: usize,
) -> bool {
    let Some(previous_index) = previous_significant(tokens, position) else {
        return true;
    };
    let previous = &tokens[previous_index];
    let separator_start = if previous.kind() == ra_ap_syntax::SyntaxKind::COLON2 {
        previous_index
    } else if previous.kind() == ra_ap_syntax::SyntaxKind::COLON
        && tokens
            .get(previous_index.saturating_sub(1))
            .is_some_and(|token| token.kind() == ra_ap_syntax::SyntaxKind::COLON)
    {
        previous_index.saturating_sub(1)
    } else {
        return true;
    };
    // Permit an explicitly rooted `::marker::Trial` path, but reject
    // `other::marker::Trial`: the configured marker must own the complete
    // qualified path rather than merely appear as a suffix.
    previous_significant(tokens, separator_start)
        .is_none_or(|index| tokens[index].kind() != ra_ap_syntax::SyntaxKind::IDENT)
}

fn previous_significant(tokens: &[ra_ap_syntax::SyntaxToken], before: usize) -> Option<usize> {
    (0..before).rev().find(|index| {
        !matches!(
            tokens[*index].kind(),
            ra_ap_syntax::SyntaxKind::WHITESPACE | ra_ap_syntax::SyntaxKind::COMMENT
        )
    })
}

fn next_significant(tokens: &[ra_ap_syntax::SyntaxToken], from: usize) -> Option<usize> {
    (from..tokens.len()).find(|index| {
        !matches!(
            tokens[*index].kind(),
            ra_ap_syntax::SyntaxKind::WHITESPACE | ra_ap_syntax::SyntaxKind::COMMENT
        )
    })
}

struct TrialPathMatch {
    qualified: bool,
    name_token_index: usize,
    open_paren_index: usize,
}

fn trial_callback_name(
    tokens: &[ra_ap_syntax::SyntaxToken],
    name_token_index: usize,
) -> Option<String> {
    let mut index = name_token_index + 1;
    while let Some(token) = tokens.get(index) {
        if token.kind() == ra_ap_syntax::SyntaxKind::COMMA {
            let mut callback_index = index + 1;
            while tokens.get(callback_index).is_some_and(|callback| {
                matches!(
                    callback.kind(),
                    ra_ap_syntax::SyntaxKind::WHITESPACE | ra_ap_syntax::SyntaxKind::COMMENT
                )
            }) {
                callback_index += 1;
            }
            let first = tokens.get(callback_index)?;
            if first.kind() != ra_ap_syntax::SyntaxKind::IDENT {
                return None;
            }
            let mut callback = first.text().to_string();
            callback_index += 1;
            loop {
                while tokens.get(callback_index).is_some_and(|token| {
                    matches!(
                        token.kind(),
                        ra_ap_syntax::SyntaxKind::WHITESPACE | ra_ap_syntax::SyntaxKind::COMMENT
                    )
                }) {
                    callback_index += 1;
                }
                let separator_width = match tokens.get(callback_index).map(|token| token.kind()) {
                    Some(ra_ap_syntax::SyntaxKind::COLON2) => Some(1),
                    Some(ra_ap_syntax::SyntaxKind::COLON)
                        if tokens.get(callback_index + 1).is_some_and(|token| {
                            token.kind() == ra_ap_syntax::SyntaxKind::COLON
                        }) =>
                    {
                        Some(2)
                    }
                    _ => None,
                };
                let Some(separator_width) = separator_width else {
                    break;
                };
                callback_index += separator_width;
                while tokens.get(callback_index).is_some_and(|token| {
                    matches!(
                        token.kind(),
                        ra_ap_syntax::SyntaxKind::WHITESPACE | ra_ap_syntax::SyntaxKind::COMMENT
                    )
                }) {
                    callback_index += 1;
                }
                let segment = tokens
                    .get(callback_index)
                    .filter(|token| token.kind() == ra_ap_syntax::SyntaxKind::IDENT)?;
                callback.push_str("::");
                callback.push_str(segment.text());
                callback_index += 1;
            }
            return Some(callback);
        }
        if token.kind() == ra_ap_syntax::SyntaxKind::R_PAREN {
            return None;
        }
        index += 1;
    }
    None
}

fn unique_callback_function(index: &RustIndex, target: &Path, name: &str) -> Option<FunctionFact> {
    let name = match name.rsplit("::").next() {
        Some(name) => name,
        None => name,
    };
    let mut matches = index
        .files
        .get(target)?
        .functions
        .iter()
        .filter(|function| {
            function.name == name && function.source_role == FunctionSourceRole::HarnessHelper
        })
        .cloned();
    let function = matches.next()?;
    matches.next().is_none().then_some(function)
}

/// Offset just past the `)` balancing the `(` at `open_paren_index`,
/// bounded to the token stream. `None` (unbalanced) means the span is not
/// provable and the match is skipped.
fn balanced_call_end(tokens: &[ra_ap_syntax::SyntaxToken], open_paren_index: usize) -> Option<u32> {
    let mut depth: usize = 0;
    for token in &tokens[open_paren_index..] {
        match token.kind() {
            ra_ap_syntax::SyntaxKind::L_PAREN => depth += 1,
            ra_ap_syntax::SyntaxKind::R_PAREN => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    let end: u32 = token.text_range().end().into();
                    return Some(end);
                }
            }
            _ => {}
        }
    }
    None
}

/// Assertion evidence for one trial subject: the exact assertion-macro and
/// `unwrap`/`expect` tokens inside the registration's own argument span.
/// Only tokens between the name argument and the balanced close count, so
/// adjacent code never credits this subject.
fn parser_oracles_for_node_tokens(
    tokens: &[ra_ap_syntax::SyntaxToken],
    name_token_index: usize,
    line_index: &LineIndex,
) -> Vec<OracleFact> {
    let mut assertions = Vec::new();
    let mut depth: usize = 1;
    let mut index = name_token_index + 1;
    while index < tokens.len() && depth > 0 {
        let token = &tokens[index];
        match token.kind() {
            ra_ap_syntax::SyntaxKind::L_PAREN => depth += 1,
            ra_ap_syntax::SyntaxKind::R_PAREN => depth = depth.saturating_sub(1),
            ra_ap_syntax::SyntaxKind::IDENT => {
                let name = token.text();
                let bang_index = next_significant(tokens, index + 1);
                let macro_open_index = if is_assertion_macro_name(name)
                    && bang_index
                        .and_then(|index| tokens.get(index))
                        .is_some_and(|token| token.kind() == ra_ap_syntax::SyntaxKind::BANG)
                {
                    bang_index.and_then(|bang_index| {
                        let open_index = next_significant(tokens, bang_index + 1)?;
                        tokens
                            .get(open_index)
                            .is_some_and(|token| {
                                matches!(
                                    token.kind(),
                                    ra_ap_syntax::SyntaxKind::L_PAREN
                                        | ra_ap_syntax::SyntaxKind::L_CURLY
                                        | ra_ap_syntax::SyntaxKind::L_BRACK
                                )
                            })
                            .then_some(open_index)
                    })
                } else {
                    None
                };
                let is_macro_invocation = macro_open_index.is_some();
                let is_fallible_call = matches!(name, "unwrap" | "expect")
                    && index >= 1
                    && tokens[index - 1].kind() == ra_ap_syntax::SyntaxKind::DOT
                    && next_significant(tokens, index + 1)
                        .and_then(|index| tokens.get(index))
                        .is_some_and(|token| token.kind() == ra_ap_syntax::SyntaxKind::L_PAREN);
                if !is_macro_invocation && !is_fallible_call {
                    index += 1;
                    continue;
                }
                // assert!(..) / assert_eq!(..) — the bang and token tree
                // follow; the assertion text is the macro invocation.
                let mut text = if is_fallible_call {
                    format!(".{name}(")
                } else {
                    name.to_string()
                };
                let mut cursor = index + 1;
                if is_macro_invocation {
                    text.push('!');
                    if let Some(open_index) = macro_open_index {
                        cursor = open_index;
                    }
                    if let Some(tree_text) = macro_token_tree_text(tokens, cursor) {
                        text.push_str(&tree_text);
                    }
                } else if is_fallible_call {
                    // The method name itself is sufficient for the shared
                    // classifier to establish a smoke oracle. Keep the
                    // argument body out of this small structural witness.
                }
                let classification = classify_assertion(&text);
                assertions.push(OracleFact {
                    line: line_index.line(token.text_range().start()),
                    kind: classification.kind,
                    strength: classification.strength,
                    observed_tokens: extract_identifier_tokens(&text),
                    text,
                });
            }
            _ => {}
        }
        index += 1;
    }
    assertions.sort_by(|left, right| left.line.cmp(&right.line).then(left.text.cmp(&right.text)));
    assertions
}

fn macro_token_tree_text(
    tokens: &[ra_ap_syntax::SyntaxToken],
    open_index: usize,
) -> Option<String> {
    let mut delimiters = Vec::new();
    let mut text = String::new();
    for token in tokens.iter().skip(open_index) {
        let kind = token.kind();
        match kind {
            ra_ap_syntax::SyntaxKind::L_PAREN
            | ra_ap_syntax::SyntaxKind::L_CURLY
            | ra_ap_syntax::SyntaxKind::L_BRACK => delimiters.push(kind),
            ra_ap_syntax::SyntaxKind::R_PAREN
            | ra_ap_syntax::SyntaxKind::R_CURLY
            | ra_ap_syntax::SyntaxKind::R_BRACK => {
                let opening = delimiters.pop()?;
                let expected = match opening {
                    ra_ap_syntax::SyntaxKind::L_PAREN => ra_ap_syntax::SyntaxKind::R_PAREN,
                    ra_ap_syntax::SyntaxKind::L_CURLY => ra_ap_syntax::SyntaxKind::R_CURLY,
                    ra_ap_syntax::SyntaxKind::L_BRACK => ra_ap_syntax::SyntaxKind::R_BRACK,
                    _ => return None,
                };
                if kind != expected {
                    return None;
                }
            }
            _ => {}
        }
        text.push_str(token.text());
        if delimiters.is_empty() {
            return Some(text);
        }
    }
    None
}

fn is_assertion_macro_name(name: &str) -> bool {
    crate::analysis::syntax::ra::is_assertion_macro_leaf(name)
}
/// Exact registered test-producing attribute adapter, generation 1
/// (#3532). Promotes functions carrying the exact registered attribute
/// path through the shared #3531 role authority. The attribute must
/// either be spelled as the full registered path (`#[<marker>]`) or bound
/// in the target file by a top-level `use` resolving to the marker; a
/// lookalike spelling or a same-named unrelated import never classifies.
fn apply_registered_attribute(
    index: &mut RustIndex,
    registration: &TestHarnessRegistration,
    source: &str,
    subjects: &mut Vec<HarnessSubjectFact>,
    limitations: &mut Vec<HarnessLimitationFact>,
) {
    let target = registration.target.clone();
    let parse = SourceFile::parse(source, Edition::CURRENT);
    let use_bindings = if parse.errors().is_empty() {
        top_level_use_bindings(parse.tree().syntax(), marker_leaf(&registration.marker))
    } else {
        limitations.push(HarnessLimitationFact {
            registration_id: registration.registration_id.clone(),
            code: "parse_unavailable".to_string(),
            file: target.clone(),
            line: 1,
            detail: "the registered attribute target did not parse; no registered subjects were established (fail closed)".to_string(),
        });
        BTreeSet::new()
    };
    let promoted_functions: Vec<(usize, String, TestFact, HarnessSubjectFact)> = {
        let Some(facts) = index.files.get(&target) else {
            return;
        };
        let mut promoted = Vec::new();
        for function in &facts.functions {
            let Some(resolution) =
                resolve_registered_attribute(&function.attrs, &registration.marker, &use_bindings)
            else {
                continue;
            };
            let line = match resolution {
                RegisteredAttributeResolution::Matched => function.start_line,
                RegisteredAttributeResolution::Ambiguous { detail } => {
                    limitations.push(HarnessLimitationFact {
                        registration_id: registration.registration_id.clone(),
                        code: "ambiguous_import".to_string(),
                        file: target.clone(),
                        line: function.start_line,
                        detail,
                    });
                    continue;
                }
                RegisteredAttributeResolution::Unresolved { detail } => {
                    limitations.push(HarnessLimitationFact {
                        registration_id: registration.registration_id.clone(),
                        code: "unresolved_marker_import".to_string(),
                        file: target.clone(),
                        line: function.start_line,
                        detail,
                    });
                    continue;
                }
            };
            // Built-in exact test attributes keep precedence: the function
            // is already an executable test and must not register twice
            // (#3499's families continue to work without duplicate
            // registry entries).
            if function.source_role.registers_executable_test() {
                continue;
            }
            let test = registered_attribute_test_fact(function);
            let subject = HarnessSubjectFact {
                registration_id: registration.registration_id.clone(),
                harness_kind: registration.kind.as_str().to_string(),
                adapter: registration.adapter.as_str().to_string(),
                marker: registration.marker.clone(),
                name: function.name.clone(),
                file: target.clone(),
                start_line: function.start_line,
                end_line: function.end_line,
                body: function.body.clone(),
                calls: function.calls.clone(),
                assertions: test.assertions.clone(),
                literals: function.literals.clone(),
                selector: HarnessSelectorCapability::NamedUnexecuted,
                claim: HarnessSubjectClaim::NamedFunction,
                provenance: TestHarnessRegistration::provenance().to_string(),
            };
            promoted.push((line, function.name.clone(), test, subject));
        }
        promoted
    };
    if promoted_functions.is_empty() {
        return;
    }

    let Some(facts) = index.files.get_mut(&target) else {
        return;
    };
    for function in &mut facts.functions {
        if promoted_functions
            .iter()
            .any(|(line, name, _, _)| *line == function.start_line && *name == function.name)
            && !function.source_role.registers_executable_test()
        {
            function.source_role = FunctionSourceRole::RegisteredTestAttribute;
        }
    }
    for (_, name, test, subject) in &promoted_functions {
        let _ = name;
        subjects.push(subject.clone());
        push_file_test(index, &target, test.clone());
        index.tests.push(test.clone());
    }
    // Mirror the promotion into the flat function list so every consumer
    // of `index.functions` sees the same producer-owned role.
    for function in &mut index.functions {
        if function.file != target {
            continue;
        }
        if promoted_functions
            .iter()
            .any(|(line, name, _, _)| *line == function.start_line && *name == function.name)
            && !function.source_role.registers_executable_test()
        {
            function.source_role = FunctionSourceRole::RegisteredTestAttribute;
        }
    }
}

fn registered_attribute_test_fact(function: &FunctionFact) -> TestFact {
    TestFact {
        name: function.name.clone(),
        file: function.file.clone(),
        start_line: function.start_line,
        end_line: function.end_line,
        body: function.body.clone(),
        calls: function.calls.clone(),
        assertions: parser_oracles_for_function(&function.body, function.start_line)
            .unwrap_or_else(|| {
                crate::analysis::rust_index::extract_assertions(&function.body, function.start_line)
            }),
        literals: function.literals.clone(),
        attrs: function.attrs.clone(),
    }
}

fn push_file_test(index: &mut RustIndex, target: &Path, test: TestFact) {
    let Some(facts) = index.files.get_mut(target) else {
        return;
    };
    if facts
        .tests
        .iter()
        .all(|existing| existing.name != test.name || existing.start_line != test.start_line)
    {
        facts.tests.push(test);
        facts.tests.sort_by(|left, right| {
            left.start_line
                .cmp(&right.start_line)
                .then(left.end_line.cmp(&right.end_line))
                .then(left.name.cmp(&right.name))
        });
    }
}

fn remove_file_test(index: &mut RustIndex, target: &Path, name: &str) {
    if let Some(facts) = index.files.get_mut(target) {
        facts.tests.retain(|test| test.name != name);
    }
}

/// Demote every function in a registered `harness = false` target to the
/// evidence-only helper role and drop the target's executable
/// `TestFact`s. With `harness = false` the libtest harness never
/// collects this file, so a `#[test]` attribute alone establishes nothing and
/// no member enters the executable-test denominator on its own: the
/// whole target is helper-evidence, and executable subjects come only
/// from adapter-established trial registrations.
fn demote_harness_target_functions(index: &mut RustIndex, target: &Path) {
    let demoted: BTreeSet<String> = {
        let Some(facts) = index.files.get(target) else {
            return;
        };
        facts
            .functions
            .iter()
            .filter(|function| function.source_role != FunctionSourceRole::HarnessHelper)
            .map(|function| function.name.clone())
            .collect()
    };
    if demoted.is_empty() {
        return;
    }
    let Some(facts) = index.files.get_mut(target) else {
        return;
    };
    for function in &mut facts.functions {
        function.source_role = FunctionSourceRole::HarnessHelper;
    }
    facts.tests.retain(|test| !demoted.contains(&test.name));
    index
        .tests
        .retain(|test| test.file != target || !demoted.contains(&test.name));
    for function in &mut index.functions {
        if function.file == target {
            function.source_role = FunctionSourceRole::HarnessHelper;
        }
    }
}

enum RegisteredAttributeResolution {
    Matched,
    Ambiguous { detail: String },
    Unresolved { detail: String },
}

/// Resolve one function's attributes against the registered marker.
fn resolve_registered_attribute(
    attrs: &[String],
    marker: &str,
    use_bindings: &BTreeSet<String>,
) -> Option<RegisteredAttributeResolution> {
    for attribute in attrs {
        let Some(path) = normalized_attribute_path(attribute) else {
            continue;
        };
        if path == marker {
            return Some(RegisteredAttributeResolution::Matched);
        }
        let Some((_prefix, name)) = marker.rsplit_once("::") else {
            continue;
        };
        if path != name {
            continue;
        }
        // Bare spelling: the file must bind the name from exactly the
        // registered path. Any other binding of the same name is the
        // same-named-unrelated-import conflict.
        let anchored = use_bindings.contains(marker);
        let conflicting = use_bindings
            .iter()
            .any(|binding| binding != marker && binding.ends_with(&format!("::{name}")));
        if anchored && !conflicting {
            return Some(RegisteredAttributeResolution::Matched);
        }
        if conflicting {
            return Some(RegisteredAttributeResolution::Ambiguous {
                detail: format!(
                    "`#[{name}]` cannot be tied to registered marker `{marker}`; the file's imports bind that name from more than one path"
                ),
            });
        }
        return Some(RegisteredAttributeResolution::Unresolved {
            detail: format!(
                "`#[{name}]` matches the last segment of registered marker `{marker}`, but no top-level import in this file binds that name from the registered path"
            ),
        });
    }
    None
}

fn marker_leaf(marker: &str) -> &str {
    marker.rsplit_once("::").map_or(marker, |(_, leaf)| leaf)
}

/// Collect the full binding paths for `name` from top-level `use` items
/// (e.g. `use libtest_mimic::Trial;` binds `libtest_mimic::Trial`;
/// `use myco::{contract_test, other};` binds `myco::contract_test`).
/// Bounded to the shapes this walker can establish: glob imports, re-exports
/// through nested modules, and aliases away from the bound name grant no
/// binding, so the authority fails closed instead of guessing.
fn top_level_use_bindings(syntax: &ra_ap_syntax::SyntaxNode, name: &str) -> BTreeSet<String> {
    let mut bindings = BTreeSet::new();
    for use_item in syntax.children().filter_map(ast::Use::cast) {
        let Some(use_tree) = use_item.use_tree() else {
            continue;
        };
        collect_use_bindings(&use_tree, String::new(), name, &mut bindings);
    }
    bindings
}

fn collect_use_bindings(
    tree: &ast::UseTree,
    prefix: String,
    name: &str,
    out: &mut BTreeSet<String>,
) {
    let path_text = tree.path().map(|path| {
        path.syntax()
            .text()
            .to_string()
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>()
    });
    let alias = tree.rename().and_then(|rename| rename.name());
    if let Some(list) = tree.use_tree_list() {
        let child_prefix = match (&path_text, prefix.is_empty()) {
            (Some(path), false) => format!("{prefix}::{path}"),
            (Some(path), true) => path.clone(),
            (None, _) => prefix,
        };
        for nested in list.use_trees() {
            collect_use_bindings(&nested, child_prefix.clone(), name, out);
        }
        return;
    }
    let Some(tree_name) = alias.map(|named| named.text().to_string()).or_else(|| {
        path_text
            .as_ref()
            .and_then(|path| path.rsplit("::").next())
            .map(ToString::to_string)
    }) else {
        return;
    };
    if tree_name != name {
        return;
    }
    let full = match (&path_text, prefix.is_empty()) {
        (Some(path), false) => format!("{prefix}::{path}"),
        (Some(path), true) => path.clone(),
        (None, false) => prefix,
        (None, true) => tree_name,
    };
    out.insert(full);
}

enum TrialBindingResolution {
    MarkerAnchored,
    Ambiguous { conflicting: String },
    Unbound,
}

fn resolve_trial_binding(bindings: &BTreeSet<String>, marker: &str) -> TrialBindingResolution {
    let anchored_full = format!("{marker}::Trial");
    let matches_marker =
        |binding: &String| binding == &anchored_full || binding == &format!("::{anchored_full}");
    let anchored = bindings.iter().any(matches_marker);
    let conflicting = bindings
        .iter()
        .filter(|binding| !matches_marker(binding))
        .cloned()
        .collect::<Vec<_>>();
    if anchored && conflicting.is_empty() {
        return TrialBindingResolution::MarkerAnchored;
    }
    if anchored || conflicting.len() > 1 {
        return TrialBindingResolution::Ambiguous {
            conflicting: conflicting.join(", "),
        };
    }
    // No binding at all, or exactly one binding from another path: the
    // invocation cannot be tied to the registered marker.
    TrialBindingResolution::Unbound
}

/// Whether the ancestor chain of a token sits inside a loop expression —
/// the bounded signal for runtime-only trial discovery. Works through
/// macro token trees too, because tokens keep their ancestor nodes.
fn in_loop(mut ancestors: impl Iterator<Item = ra_ap_syntax::SyntaxNode>) -> bool {
    ancestors.any(|ancestor| {
        ast::WhileExpr::can_cast(ancestor.kind())
            || ast::ForExpr::can_cast(ancestor.kind())
            || ast::LoopExpr::can_cast(ancestor.kind())
    })
}

fn in_macro_definition(mut ancestors: impl Iterator<Item = ra_ap_syntax::SyntaxNode>) -> bool {
    ancestors.any(|ancestor| ast::MacroRules::can_cast(ancestor.kind()))
}

/// Exact simple string-literal text of one token. Escaped spellings,
/// raw strings, and empty names are not demonstrated by this bounded parser and
/// stay unresolved (fail closed).
fn string_literal_token_text(token: &ra_ap_syntax::SyntaxToken) -> Option<String> {
    if token.kind() != ra_ap_syntax::SyntaxKind::STRING {
        return None;
    }
    let text = token.text().trim();
    let body = text.strip_prefix('"')?.strip_suffix('"')?;
    if body.is_empty() || body.contains(BACKSLASH) {
        return None;
    }
    Some(body.to_string())
}

/// The one escape character this bounded parser refuses to guess about.
const BACKSLASH: char = '\\';

#[cfg(test)]
mod tests;
