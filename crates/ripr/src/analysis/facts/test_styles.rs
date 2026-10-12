use super::cfg_predicates;
use super::index::{FactArena, IndexedFileFacts};
use super::{FunctionFact, FunctionSourceRole, RustIndex, TestFact};
use crate::analysis::cancellation;
use crate::analysis::extract::extract_assertions;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FunctionKey<'a> {
    file: &'a Path,
    start_line: usize,
    end_line: usize,
    name: &'a str,
    body: &'a str,
}

impl<'a> FunctionKey<'a> {
    fn from_function(function: &'a FunctionFact) -> Self {
        Self {
            file: &function.file,
            start_line: function.start_line,
            end_line: function.end_line,
            name: &function.name,
            body: &function.body,
        }
    }

    fn from_test(test: &'a TestFact) -> Self {
        Self {
            file: &test.file,
            start_line: test.start_line,
            end_line: test.end_line,
            name: &test.name,
            body: &test.body,
        }
    }
}

/// Reconcile parser-backed and lexical-fallback test facts through one exact
/// attribute vocabulary. This runs immediately after index construction so
/// every index consumer sees the same executable-test role regardless of
/// which syntax producer handled the file.
pub(super) fn normalize_index_test_styles(index: &mut RustIndex) -> Result<(), String> {
    for facts in index.files.values_mut() {
        cancellation::checkpoint()?;
        normalize_indexed_file_test_styles(
            facts,
            &mut index.function_facts,
            &mut index.test_facts,
        )?;
    }
    index.test_order.clear();
    let mut role_by_function = BTreeMap::new();
    let mut test_by_function = BTreeMap::new();
    for facts in index.files.values() {
        cancellation::checkpoint()?;
        for &id in &facts.functions {
            cancellation::checkpoint()?;
            let function = &index.function_facts[id];
            role_by_function.insert(FunctionKey::from_function(function), function.source_role);
        }
        for &id in &facts.tests {
            cancellation::checkpoint()?;
            test_by_function.insert(FunctionKey::from_test(&index.test_facts[id]), id);
        }
    }
    let mut updates = Vec::new();
    for &id in &index.function_order {
        cancellation::checkpoint()?;
        let function = &index.function_facts[id];
        let role = role_by_function
            .get(&FunctionKey::from_function(function))
            .copied()
            .unwrap_or(function.source_role);
        updates.push((id, role));
        if role.is_evidence_role()
            && let Some(test) = test_by_function.remove(&FunctionKey::from_function(function))
        {
            index.test_order.push(test);
        }
    }
    drop(role_by_function);
    index.apply_function_roles(updates, std::iter::empty());
    cancellation::checkpoint()?;
    Ok(())
}

fn normalize_indexed_file_test_styles(
    facts: &mut IndexedFileFacts,
    functions: &mut FactArena<FunctionFact>,
    tests: &mut FactArena<TestFact>,
) -> Result<(), String> {
    let mut existing_tests = std::mem::take(&mut facts.tests)
        .into_iter()
        .map(|id| ((tests[id].start_line, tests[id].name.clone()), id))
        .collect::<BTreeMap<_, _>>();
    let lexical_lines = facts
        .used_lexical_fallback
        .then(|| facts.source.lines().collect::<Vec<_>>());
    let cfg_test_lines = std::cell::OnceCell::new();
    let mut normalized_tests = Vec::new();
    for &id in &facts.functions {
        cancellation::checkpoint()?;
        let function = &mut functions[id];
        let existing = existing_tests.remove(&(function.start_line, function.name.clone()));
        let (defines_test, compiled_out) = match lexical_lines.as_deref() {
            Some(lines) => {
                let attributes = lexical_attributes_before(lines, function.start_line);
                let gates = lexical_gate_attributes_before(lines, function.start_line);
                (
                    attributes_define_test(attributes.iter().copied()),
                    attributes_compile_out_in_test_build(gates.iter().map(String::as_str)),
                )
            }
            None => (
                attributes_define_test(function.attrs.iter().map(String::as_str)),
                attributes_compile_out_in_test_build(function.attrs.iter().map(String::as_str)),
            ),
        };
        let has_test_attribute = defines_test && !compiled_out;
        // A test under a cfg that is false in a test build never runs and
        // never compiles (#6293): evidence-only, so it is neither an
        // executable test nor a production probe subject.
        let preserve_cfg_test_role = (defines_test && compiled_out)
            || (!has_test_attribute
                && function.source_role.is_evidence_role()
                && inside_cfg_test_module_at(
                    cfg_test_lines.get_or_init(|| cfg_test_module_lines(&facts.source)),
                    &facts.source,
                    function.start_line,
                ));
        let promotion_claimed_expansion =
            function.source_role == FunctionSourceRole::ParameterizedExpansion;
        function.source_role = if has_test_attribute {
            if promotion_claimed_expansion {
                FunctionSourceRole::ParameterizedExpansion
            } else {
                FunctionSourceRole::TestAttribute
            }
        } else if preserve_cfg_test_role {
            FunctionSourceRole::CfgTestModule
        } else {
            FunctionSourceRole::Production
        };
        if has_test_attribute {
            normalized_tests.push(
                existing.unwrap_or_else(|| tests.allocate(test_fact_from_function(function))),
            );
        }
    }
    facts.tests = normalized_tests;
    Ok(())
}

fn test_fact_from_function(function: &FunctionFact) -> TestFact {
    TestFact {
        name: function.name.clone(),
        file: function.file.clone(),
        start_line: function.start_line,
        end_line: function.end_line,
        body: function.body.clone(),
        calls: function.calls.clone(),
        assertions: extract_assertions(&function.body, function.start_line),
        literals: function.literals.clone(),
        attrs: function.attrs.clone(),
        // #3727 Slice A: the reconciled test mirrors the function's shadow
        // facts — same body, same decisions.
        nested_fn_names: function.nested_fn_names.clone(),
        let_bindings: function.let_bindings.clone(),
    }
}

/// Whether any attribute is a recognised test attribute (`#[test]`,
/// `#[tokio::test]`, `#[rstest]`, ...). The one authority for which functions
/// are tests; the inline test-region cage consumes it to decide whether an
/// inserted function is a new test.
pub(crate) fn attributes_define_test<'attribute>(
    attributes: impl IntoIterator<Item = &'attribute str>,
) -> bool {
    attributes.into_iter().any(|attribute| {
        normalized_test_attribute_path(attribute)
            .as_deref()
            .is_some_and(is_test_attribute_path)
    })
}

/// A `#[test]` under a `cfg` that is false in a test build (`cfg(any())`,
/// `cfg(not(test))`) never runs, so it cannot discriminate anything (#6293).
/// Only a provably false gate counts; feature, target and custom atoms stay
/// unknown and keep the test, as before.
fn attributes_compile_out_in_test_build<'attribute>(
    attributes: impl IntoIterator<Item = &'attribute str>,
) -> bool {
    attributes.into_iter().any(|attribute| {
        cfg_predicates::attribute_test_build_availability(attribute) == Some(false)
    })
}

fn lexical_attributes_before<'source>(
    lines: &[&'source str],
    start_line: usize,
) -> Vec<&'source str> {
    let function_index = start_line.saturating_sub(1).min(lines.len());
    let mut attributes = Vec::new();

    for line in lines[..function_index].iter().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("#[") {
            attributes.push(trimmed);
            continue;
        }
        break;
    }

    attributes.reverse();
    attributes
}

/// Collects the attributes immediately preceding `start_line` so the
/// compile-out check sees a multi-line gate (`#[cfg(\n    any()\n)]`) the
/// same way a one-line gate is seen (#6293, #7043).
///
/// The scan is a single forward pass over a byte-bounded suffix of the
/// prefix: comments and blank lines are skipped, complete `#[...]` forms
/// are split with the shared bracket authority, and other code resets the
/// trailing sequence. An unclosed `#[` stops the scan (fail open) instead
/// of walking the rest of the file. This path is independent of
/// [`join_leading_attribute`]'s 32-line bound, which still governs
/// cfg-test *module* detection.
fn lexical_gate_attributes_before(lines: &[&str], start_line: usize) -> Vec<String> {
    // Real gates are small; this bound keeps an unclosed `#[` and a long
    // run of `]`-ending junk linear in the budget rather than the file.
    const MAX_GATE_SCAN_BYTES: usize = 8 * 1024;
    let function_index = start_line.saturating_sub(1).min(lines.len());
    let window_start = lexical_gate_scan_window_start(lines, function_index, MAX_GATE_SCAN_BYTES);
    lines
        .get(window_start..function_index)
        .map(collect_trailing_gate_attributes)
        .unwrap_or_default()
}

fn lexical_gate_scan_window_start(lines: &[&str], end: usize, max_bytes: usize) -> usize {
    let mut consumed = 0usize;
    let mut start = end;
    while start > 0 {
        let previous = start - 1;
        let Some(line) = lines.get(previous) else {
            break;
        };
        let line_bytes = line.len().saturating_add(1);
        if consumed.saturating_add(line_bytes) > max_bytes {
            break;
        }
        consumed += line_bytes;
        start = previous;
    }
    start
}

fn collect_trailing_gate_attributes(lines: &[&str]) -> Vec<String> {
    if lines.is_empty() {
        return Vec::new();
    }
    let joined = lines.join("\n");
    let bytes = joined.as_bytes();
    let mut index = 0usize;
    let mut attributes = Vec::new();
    while index < bytes.len() {
        index = skip_whitespace_and_comments(bytes, index);
        if index >= bytes.len() {
            break;
        }
        let Some(rest) = joined.get(index..) else {
            break;
        };
        if rest.starts_with("#[") || rest.starts_with("#![") {
            match cfg_predicates::split_leading_attribute(rest) {
                Some((attribute, remainder)) => {
                    attributes.push(attribute.to_string());
                    let next = joined.len().saturating_sub(remainder.len());
                    if next <= index {
                        break;
                    }
                    index = next;
                }
                None => {
                    // Unclosed `#[`: do not rescan from every later `#`
                    // (that would be quadratic). Fail open for this region.
                    attributes.clear();
                    break;
                }
            }
            continue;
        }
        attributes.clear();
        let next = skip_line(bytes, index);
        if next <= index {
            break;
        }
        index = next;
    }
    attributes
}

fn skip_whitespace_and_comments(bytes: &[u8], mut index: usize) -> usize {
    while let Some(&byte) = bytes.get(index) {
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            index += 2;
            while bytes.get(index).is_some_and(|current| *current != b'\n') {
                index += 1;
            }
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            match skip_nested_block_comment(bytes, index) {
                Some(next) => {
                    index = next;
                    continue;
                }
                None => break,
            }
        }
        break;
    }
    index
}

fn skip_nested_block_comment(bytes: &[u8], index: usize) -> Option<usize> {
    let mut depth = 1usize;
    let mut cursor = index.checked_add(2)?;
    while cursor < bytes.len() {
        let current = bytes.get(cursor).copied()?;
        let next = bytes.get(cursor + 1).copied();
        if current == b'/' && next == Some(b'*') {
            depth = depth.saturating_add(1);
            cursor = cursor.checked_add(2)?;
        } else if current == b'*' && next == Some(b'/') {
            cursor = cursor.checked_add(2)?;
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(cursor);
            }
        } else {
            cursor += 1;
        }
    }
    None
}

fn skip_line(bytes: &[u8], mut index: usize) -> usize {
    while let Some(&byte) = bytes.get(index) {
        index += 1;
        if byte == b'\n' {
            break;
        }
    }
    index
}

/// Line walk that preserves producer-owned cfg-test evidence roles. The
/// cfg-term semantics come from the one shared authority
/// (`cfg_predicates`, #3530) — the same source the parser producer
/// consumes — so this walk can never disagree with the producer about
/// whether an attribute gates its module on test. Attributes spanning
/// multiple lines are joined before classification; only exact bounded
/// forms the walk can prove grant credit, and everything else fails closed.
fn is_inside_cfg_test_module(source: &str, function_start_line: usize) -> bool {
    let lines: Vec<&str> = source
        .lines()
        .take(function_start_line.saturating_sub(1))
        .collect();
    cfg_test_module_walk(&lines, |_, _| {})
}

/// [`is_inside_cfg_test_module`] for every line of one file from a single
/// walk: entry `n` answers for a function whose first `n` lines precede it.
/// Asking per function rewalked the file from its top each time, which was
/// quadratic in large test files and most of `index_test_styles` on warm
/// runs (#5363). `None` marks a line inside a multi-line attribute: the
/// prefix walk would stop mid-attribute, so that answer comes from the
/// prefix walk itself.
fn cfg_test_module_lines(source: &str) -> Vec<Option<bool>> {
    let lines: Vec<&str> = source.lines().collect();
    let mut inside = vec![None; lines.len() + 1];
    let at_end = cfg_test_module_walk(&lines, |index, state| inside[index] = Some(state));
    inside[lines.len()] = Some(at_end);
    inside
}

fn inside_cfg_test_module_at(
    table: &[Option<bool>],
    source: &str,
    function_start_line: usize,
) -> bool {
    match table.get(function_start_line.saturating_sub(1)) {
        Some(Some(inside)) => *inside,
        _ => is_inside_cfg_test_module(source, function_start_line),
    }
}

/// Walks `lines` and returns whether the end is inside a `cfg(test)`
/// module. `at_line(index, inside)` reports the same answer for the prefix
/// `lines[..index]` at every line where a walk step starts; a walk of that
/// prefix alone takes the same steps, since no attribute join before
/// `index` reaches past it.
fn cfg_test_module_walk(lines: &[&str], mut at_line: impl FnMut(usize, bool)) -> bool {
    let mut scopes: Vec<bool> = Vec::new();
    // How many open scopes are `cfg(test)` modules, so asking "inside one?"
    // at every step costs O(1) instead of a scan of the scope stack.
    let mut cfg_test_scopes = 0usize;
    let mut pending_cfg_test = false;
    let mut index = 0usize;

    while index < lines.len() {
        at_line(index, cfg_test_scopes != 0);
        let remainder_storage;
        let line: &str = if lines[index].trim_start().starts_with("#[") {
            match join_leading_attribute(&lines[index..]) {
                Some((attribute, mut remainder, consumed_lines)) => {
                    if cfg_predicates::attribute_requires_test(&attribute) {
                        pending_cfg_test = true;
                    }
                    // The closing line's remainder is processed below; the
                    // loop's own `index += 1` then moves past it.
                    index += consumed_lines.saturating_sub(1);
                    // One line can carry several attributes; the shared
                    // splitter classifies each leading attribute in turn.
                    while remainder.trim_start().starts_with("#[") {
                        let Some((next_attribute, rest)) =
                            cfg_predicates::split_leading_attribute(&remainder)
                        else {
                            break;
                        };
                        if cfg_predicates::attribute_requires_test(next_attribute) {
                            pending_cfg_test = true;
                        }
                        remainder = rest.to_string();
                    }
                    remainder_storage = remainder;
                    remainder_storage.as_str()
                }
                None => lines[index],
            }
        } else {
            lines[index]
        };

        let trimmed = line.trim();
        let declares_cfg_test_module =
            pending_cfg_test && trimmed.contains("mod ") && trimmed.contains('{');
        let mut module_opened = false;
        for character in line.chars() {
            match character {
                '{' => {
                    let is_cfg_test_module = declares_cfg_test_module && !module_opened;
                    cfg_test_scopes += usize::from(is_cfg_test_module);
                    scopes.push(is_cfg_test_module);
                    module_opened = true;
                }
                '}' => {
                    cfg_test_scopes -= usize::from(scopes.pop() == Some(true));
                }
                _ => {}
            }
        }
        if declares_cfg_test_module || (!trimmed.starts_with("#[") && !trimmed.is_empty()) {
            pending_cfg_test = false;
        }
        index += 1;
    }

    cfg_test_scopes != 0
}

/// Joins continuation lines until the leading attribute's closing bracket so
/// multi-line attribute spellings the parser accepts stay visible to this
/// walk. Returns the attribute text, the unconsumed remainder of the closing
/// line, and how many lines the attribute spans. `None` (no complete
/// attribute) fails closed with no gate credit.
fn join_leading_attribute(lines: &[&str]) -> Option<(String, String, usize)> {
    // An unclosed `#[` must not swallow the rest of the file: real
    // attributes span a handful of lines, so past this bound the walk fails
    // closed for the candidate instead of joining unrelated lines.
    const MAX_JOINED_LINES: usize = 32;
    let mut joined = String::new();
    for (offset, line) in lines.iter().enumerate() {
        if offset >= MAX_JOINED_LINES {
            return None;
        }
        joined.push_str(line);
        joined.push('\n');
        if let Some((attribute, remainder)) = cfg_predicates::split_leading_attribute(&joined) {
            return Some((attribute.to_string(), remainder.to_string(), offset + 1));
        }
    }
    None
}

pub(super) fn normalized_test_attribute_path(attribute: &str) -> Option<String> {
    let body = attribute.trim().strip_prefix("#[")?;
    let closing = body.rfind(']')?;
    let trailing = body.get(closing + 1..)?.trim();
    if !trailing.is_empty() && !trailing.starts_with("//") && !trailing.starts_with("/*") {
        return None;
    }
    let head = body.get(..closing)?.trim();
    if !attribute_arguments_are_balanced(head) {
        return None;
    }
    let path = head
        .split('(')
        .next()?
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let path = path.trim_start_matches("::");
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

fn attribute_arguments_are_balanced(head: &str) -> bool {
    let Some(opening) = head.find('(') else {
        return true;
    };
    let mut depth = 0usize;
    for character in head[opening..].chars() {
        match character {
            '(' => depth = depth.saturating_add(1),
            ')' => {
                let Some(next_depth) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next_depth;
            }
            _ => {}
        }
    }
    depth == 0
}

/// The attribute paths that make a Rust function an executable test. The
/// #6965 unbuilt-file drop derives its test-bearing markers from this list,
/// so a path added here is also walked there.
pub(crate) const BUILT_IN_TEST_ATTRIBUTE_PATHS: &[&str] = &[
    "test",
    "tokio::test",
    "async_std::test",
    "rstest",
    "rstest::rstest",
    "quickcheck",
    "quickcheck_macros::quickcheck",
    "wasm_bindgen_test",
    "wasm_bindgen_test::wasm_bindgen_test",
    "test_case",
    "test_case::test_case",
    "ntest::test_case",
    "test_matrix",
    "test_case::test_matrix",
];

fn is_test_attribute_path(path: &str) -> bool {
    BUILT_IN_TEST_ATTRIBUTE_PATHS.contains(&path)
}

#[cfg(test)]
mod tests;
