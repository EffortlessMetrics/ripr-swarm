use super::{FunctionFact, RustIndex, TestFact};
use crate::analysis::extract::{extract_assertions, extract_literal_facts};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

type FunctionKey = (PathBuf, usize, usize, String);

pub(super) fn promote_explicit_test_case_functions(index: &mut RustIndex) {
    let mut known_tests = index.tests.iter().map(test_key).collect::<BTreeSet<_>>();
    let mut promoted = Vec::new();

    for function in &mut index.functions {
        if !is_explicit_test_case_function(function) {
            continue;
        }
        let key = function_key(function);
        if !known_tests.insert(key) {
            continue;
        }
        function.is_test = true;
        promoted.push(test_fact(function));
    }

    if promoted.is_empty() {
        return;
    }

    for test in &promoted {
        let key = test_key(test);
        let Some(file) = index.files.get_mut(&test.file) else {
            continue;
        };
        if let Some(function) = file
            .functions
            .iter_mut()
            .find(|function| function_key(function) == key)
        {
            function.is_test = true;
        }
        if file
            .tests
            .iter()
            .all(|existing| test_key(existing) != key)
        {
            file.tests.push(test.clone());
            file.tests.sort_by(|left, right| {
                left.start_line
                    .cmp(&right.start_line)
                    .then(left.end_line.cmp(&right.end_line))
                    .then(left.name.cmp(&right.name))
            });
        }
    }

    index.tests.extend(promoted);
    let positions = index
        .functions
        .iter()
        .enumerate()
        .map(|(position, function)| (function_key(function), position))
        .collect::<BTreeMap<_, _>>();
    index.tests.sort_by_key(|test| {
        positions
            .get(&test_key(test))
            .copied()
            .unwrap_or(usize::MAX)
    });
}

fn is_explicit_test_case_function(function: &FunctionFact) -> bool {
    function
        .attrs
        .iter()
        .any(|attribute| is_explicit_test_case_attribute(attribute))
}

fn is_explicit_test_case_attribute(attribute: &str) -> bool {
    let compact = attribute
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    compact.starts_with("#[test_case(") || compact.starts_with("#[test_case::test_case(")
}

fn test_fact(function: &FunctionFact) -> TestFact {
    let mut literals = function.literals.clone();
    for attribute in function
        .attrs
        .iter()
        .filter(|attribute| is_explicit_test_case_attribute(attribute))
    {
        literals.extend(extract_literal_facts(attribute, function.start_line));
    }
    literals.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.value.cmp(&right.value))
    });
    literals.dedup_by(|left, right| left.line == right.line && left.value == right.value);

    TestFact {
        name: function.name.clone(),
        file: function.file.clone(),
        start_line: function.start_line,
        end_line: function.end_line,
        body: function.body.clone(),
        calls: function.calls.clone(),
        assertions: extract_assertions(&function.body, function.start_line),
        literals,
        attrs: function.attrs.clone(),
    }
}

fn function_key(function: &FunctionFact) -> FunctionKey {
    (
        function.file.clone(),
        function.start_line,
        function.end_line,
        function.name.clone(),
    )
}

fn test_key(test: &TestFact) -> FunctionKey {
    (
        test.file.clone(),
        test.start_line,
        test.end_line,
        test.name.clone(),
    )
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(name: &str) -> Result<TempDir, Box<dyn Error>> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("ripr-{name}-{stamp}"));
        fs::create_dir_all(&root)?;
        Ok(TempDir(root))
    }

    #[test]
    fn explicit_test_case_attributes_feed_the_ordinary_rust_index()
    -> Result<(), Box<dyn Error>> {
        let root = temp_dir("test-case-attributes")?;
        fs::create_dir_all(root.0.join("tests"))?;
        fs::write(
            root.0.join("Cargo.toml"),
            "[package]\nname='test-case-fixture'\nversion='0.1.0'\nedition='2024'\n",
        )?;
        fs::write(
            root.0.join("tests/parameterized.rs"),
            r#"
fn helper(value: i32) -> i32 {
    value * 2
}

#[rstest]
#[case(1, 2)]
fn rstest_case(input: i32, expected: i32) {
    assert_eq!(helper(input), expected);
}

#[test_case(2, 4)]
fn test_case_case(input: i32, expected: i32) {
    assert_eq!(helper(input), expected);
}

#[test_case::test_case(3, 6)]
fn qualified_test_case(input: i32, expected: i32) {
    assert_eq!(helper(input), expected);
}

#[case(7)]
fn orphan_case(input: i32) {
    assert_eq!(helper(input), input);
}

#[test]
fn ordinary_test() {
    assert_eq!(helper(4), 8);
}
"#,
        )?;

        let index = crate::analysis::facts::build_index(
            &root.0,
            &[PathBuf::from("tests/parameterized.rs")],
        )?;
        let test_names = index
            .tests
            .iter()
            .map(|test| test.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(index.functions.len(), 6);
        assert_eq!(
            test_names,
            vec![
                "rstest_case",
                "test_case_case",
                "qualified_test_case",
                "ordinary_test"
            ]
        );
        assert!(
            index
                .functions
                .iter()
                .find(|function| function.name == "helper")
                .is_some_and(|function| !function.is_test),
            "unannotated helper must remain production role"
        );
        assert!(
            index
                .functions
                .iter()
                .find(|function| function.name == "orphan_case")
                .is_some_and(|function| !function.is_test),
            "a case row without an explicit test harness must not be promoted"
        );

        let test_case = index
            .tests
            .iter()
            .find(|test| test.name == "test_case_case")
            .ok_or("missing unqualified test-case fact")?;
        assert!(
            test_case
                .attrs
                .iter()
                .any(|attribute| attribute.contains("test_case"))
        );
        assert!(!test_case.assertions.is_empty());
        assert_eq!(
            test_case
                .literals
                .iter()
                .map(|literal| literal.value.as_str())
                .collect::<Vec<_>>(),
            vec!["2", "4"]
        );

        let qualified = index
            .tests
            .iter()
            .find(|test| test.name == "qualified_test_case")
            .ok_or("missing qualified test-case fact")?;
        assert_eq!(
            qualified
                .literals
                .iter()
                .map(|literal| literal.value.as_str())
                .collect::<Vec<_>>(),
            vec!["3", "6"]
        );

        let file = index
            .files
            .get(Path::new("tests/parameterized.rs"))
            .ok_or("missing file facts")?;
        assert_eq!(
            file.tests
                .iter()
                .map(|test| test.name.as_str())
                .collect::<Vec<_>>(),
            test_names
        );
        Ok(())
    }
}
