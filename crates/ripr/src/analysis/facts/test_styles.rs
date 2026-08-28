use super::{FileFacts, FunctionFact, RustIndex, TestFact};
use crate::analysis::extract::extract_assertions;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FunctionKey {
    file: PathBuf,
    start_line: usize,
    name: String,
}

impl FunctionKey {
    fn from_function(function: &FunctionFact) -> Self {
        Self {
            file: function.file.clone(),
            start_line: function.start_line,
            name: function.name.clone(),
        }
    }

    fn from_test(test: &TestFact) -> Self {
        Self {
            file: test.file.clone(),
            start_line: test.start_line,
            name: test.name.clone(),
        }
    }
}

/// Reconcile parser-backed and lexical-fallback test facts through one exact
/// attribute vocabulary. This runs immediately after index construction so
/// every index consumer sees the same executable-test role regardless of
/// which syntax producer handled the file.
pub(super) fn normalize_index_test_styles(index: &mut RustIndex) {
    for facts in index.files.values_mut() {
        normalize_file_test_styles(facts);
    }

    let mut role_by_function = BTreeMap::new();
    let mut test_by_function = BTreeMap::new();
    for facts in index.files.values() {
        for function in &facts.functions {
            role_by_function.insert(FunctionKey::from_function(function), function.is_test);
        }
        for test in &facts.tests {
            test_by_function.insert(FunctionKey::from_test(test), test.clone());
        }
    }

    for function in &mut index.functions {
        if let Some(is_test) = role_by_function.get(&FunctionKey::from_function(function)) {
            function.is_test = *is_test;
        }
    }

    let mut normalized_tests = Vec::new();
    for function in &index.functions {
        if !function.is_test {
            continue;
        }
        if let Some(test) = test_by_function.remove(&FunctionKey::from_function(function)) {
            normalized_tests.push(test);
        }
    }
    index.tests = normalized_tests;
}

fn normalize_file_test_styles(facts: &mut FileFacts) {
    let mut existing_tests = std::mem::take(&mut facts.tests)
        .into_iter()
        .map(|test| ((test.start_line, test.name.clone()), test))
        .collect::<BTreeMap<_, _>>();
    let lexical_lines = facts
        .used_lexical_fallback
        .then(|| facts.source.lines().collect::<Vec<_>>());
    let mut normalized_tests = Vec::new();

    for function in &mut facts.functions {
        let key = (function.start_line, function.name.clone());
        let existing_test = existing_tests.remove(&key);
        let has_test_attribute = match lexical_lines.as_deref() {
            Some(lines) => {
                attributes_define_test(lexical_attributes_before(lines, function.start_line))
            }
            None => attributes_define_test(function.attrs.iter().map(String::as_str)),
        };

        // Parser-backed cfg(test) helpers are evidence-role functions without
        // executable TestFacts. Preserve that producer-owned distinction while
        // correcting prefix lookalikes that arrived with a TestFact.
        let is_test_helper = function.is_test && existing_test.is_none();
        function.is_test = has_test_attribute || is_test_helper;

        if has_test_attribute {
            normalized_tests.push(match existing_test {
                Some(test) => test,
                None => test_fact_from_function(function),
            });
        }
    }

    facts.tests = normalized_tests;
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
    }
}

fn attributes_define_test<'attribute>(
    attributes: impl IntoIterator<Item = &'attribute str>,
) -> bool {
    attributes.into_iter().any(|attribute| {
        normalized_test_attribute_path(attribute)
            .as_deref()
            .is_some_and(is_test_attribute_path)
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

fn normalized_test_attribute_path(attribute: &str) -> Option<String> {
    let body = attribute.trim().strip_prefix("#[")?;
    let closing = body.rfind(']')?;
    let trailing = body.get(closing + 1..)?.trim();
    if !trailing.is_empty() && !trailing.starts_with("//") && !trailing.starts_with("/*") {
        return None;
    }
    let head = body.get(..closing)?.trim();
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

fn is_test_attribute_path(path: &str) -> bool {
    matches!(
        path,
        "test"
            | "tokio::test"
            | "async_std::test"
            | "rstest"
            | "rstest::rstest"
            | "quickcheck"
            | "quickcheck_macros::quickcheck"
            | "wasm_bindgen_test"
            | "wasm_bindgen_test::wasm_bindgen_test"
            | "test_case"
            | "test_case::test_case"
            | "ntest::test_case"
            | "test_matrix"
            | "test_case::test_matrix"
    )
}

#[cfg(test)]
mod tests;
