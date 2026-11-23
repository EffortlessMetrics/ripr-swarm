//! Tests for the strip-side and `round`-digits gate (#16203): the edit is
//! discriminated only by an argument the two versions treat differently.

use super::call_variant::{
    CallVariant, call_variant, elements_agree, key_present, literal_dict_keys, variant_agrees,
};
use super::owners_tests::{extract_owners, extract_tests};
use super::reexport_tests::{unique_tempdir, write_file};
use super::*;

const SOURCE: &str = "def clean(s):\n    return s.strip()\n\n\ndef money(x):\n    return round(x, 2)\n\n\ndef top(xs):\n    return sorted(xs)[0]\n\n\ndef low(xs):\n    xs.append(9)\n    return sorted(xs)[0]\n\n\ndef score(d):\n    return d.get(\"k\", 0) + 1\n";

fn classify(line: usize, old: &str, new: &str, test_source: &str) -> Result<Finding, String> {
    classify_in(SOURCE, line, old, new, test_source)
}

/// Classifies a change with the owner file's new-side source, as the diff
/// path does, so the builtin-name check can read it.
fn classify_in(
    source: &str,
    line: usize,
    old: &str,
    new: &str,
    test_source: &str,
) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/text.py"), source);
    let tests = extract_tests(Path::new("tests/test_text.py"), test_source);
    classify_change_with_source(
        Path::new("src/text.py"),
        line,
        new,
        Some(old),
        &owners,
        &tests,
        Some(source),
    )
    .ok_or_else(|| "a call change should classify".to_string())
}

fn classify_clean(test_source: &str) -> Result<Finding, String> {
    classify(
        2,
        "    return s.strip()",
        "    return s.lstrip()",
        test_source,
    )
}

fn classify_money(test_source: &str) -> Result<Finding, String> {
    classify(
        6,
        "    return round(x, 2)",
        "    return round(x, 3)",
        test_source,
    )
}

fn classify_top(test_source: &str) -> Result<Finding, String> {
    classify(
        10,
        "    return sorted(xs)[0]",
        "    return sorted(xs, reverse=True)[0]",
        test_source,
    )
}

fn classify_source(
    source: &str,
    line: usize,
    old: &str,
    new: &str,
    test_source: &str,
) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/text.py"), source);
    let tests = extract_tests(Path::new("tests/test_text.py"), test_source);
    classify_change_with_old(
        Path::new("src/text.py"),
        line,
        new,
        Some(old),
        &owners,
        &tests,
    )
    .ok_or_else(|| "a call change should classify".to_string())
}

#[test]
fn inputs_without_trailing_whitespace_do_not_expose_strip_to_lstrip() -> Result<(), String> {
    let finding = classify_clean(
        "from src.text import clean\n\n\ndef test_clean():\n    assert clean(\"  a\") == \"a\"\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value
                == "call `clean` with `s` with text followed by trailing whitespace"),
        "{:?}",
        finding.activation.missing_discriminators
    );
    assert_eq!(finding.ripr.infect.state, StageState::Weak);
    Ok(())
}

#[test]
fn an_input_with_trailing_whitespace_keeps_strip_to_lstrip_exposed() -> Result<(), String> {
    let finding = classify_clean(
        "from src.text import clean\n\n\ndef test_clean():\n    assert clean(\"  a\") == \"a\"\n    assert clean(\"b \") == \"b\"\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.ripr.infect.state, StageState::Yes);
    Ok(())
}

#[test]
fn inputs_within_the_smaller_digit_count_do_not_expose_a_round_change() -> Result<(), String> {
    let finding = classify_money(
        "from src.text import money\n\n\ndef test_money():\n    assert money(1.5) == 1.5\n    assert money(3) == 3\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value == "call `money` with `x` with more than 2 decimal places"),
        "{:?}",
        finding.activation.missing_discriminators
    );
    Ok(())
}

#[test]
fn an_input_with_more_decimals_keeps_a_round_change_exposed() -> Result<(), String> {
    let finding = classify_money(
        "from src.text import money\n\n\ndef test_money():\n    assert money(1.2345) == 1.23\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_unread_argument_keeps_its_reading() -> Result<(), String> {
    let finding = classify_clean(
        "from src.text import clean\n\nWORDS = [\"a \"]\n\n\ndef test_clean():\n    assert clean(\"a\") == \"a\"\n    assert clean(*WORDS) == \"a\"\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn only_a_strip_side_or_round_digits_edit_on_a_name_is_read() {
    assert_eq!(
        call_variant("return s.strip()", "return s.rstrip()"),
        Some(CallVariant::Strip {
            param: "s".to_string(),
            old: "strip".to_string(),
            new: "rstrip".to_string(),
        })
    );
    assert_eq!(
        call_variant("total = round(x, 2) + 1", "total = round(x, 0) + 1"),
        Some(CallVariant::Round {
            param: "x".to_string(),
            old: 2,
            new: 0,
        })
    );
    // An argument to strip changes which characters go.
    assert_eq!(
        call_variant("return s.strip(\"x\")", "return s.lstrip(\"x\")"),
        None
    );
    // An attribute receiver is not a parameter value.
    assert_eq!(
        call_variant("return self.s.strip()", "return self.s.lstrip()"),
        None
    );
    // Another method name.
    assert_eq!(call_variant("return s.upper()", "return s.lower()"), None);
    // A digit count on a computed argument, or a module's own `round`.
    assert_eq!(
        call_variant("return round(x * 2, 2)", "return round(x * 2, 3)"),
        None
    );
    assert_eq!(
        call_variant("return np.round(x, 2)", "return np.round(x, 3)"),
        None
    );
    // Two edits.
    assert_eq!(
        call_variant("return s.strip() + t", "return s.lstrip() + u"),
        None
    );
}

#[test]
fn agreement_follows_python_strip_and_round() {
    let strip = |old: &str, new: &str| CallVariant::Strip {
        param: "s".to_string(),
        old: old.to_string(),
        new: new.to_string(),
    };
    assert_eq!(
        variant_agrees(&strip("strip", "lstrip"), "\"  a\""),
        Some(true)
    );
    assert_eq!(
        variant_agrees(&strip("strip", "lstrip"), "\"a \""),
        Some(false)
    );
    assert_eq!(
        variant_agrees(&strip("strip", "rstrip"), "'a\t'"),
        Some(true)
    );
    assert_eq!(
        variant_agrees(&strip("lstrip", "rstrip"), "\"   \""),
        Some(true)
    );
    assert_eq!(
        variant_agrees(&strip("lstrip", "rstrip"), "\" a\""),
        Some(false)
    );
    // Escapes and prefixed literals are not read.
    assert_eq!(variant_agrees(&strip("strip", "lstrip"), "\"a\\n\""), None);
    assert_eq!(variant_agrees(&strip("strip", "lstrip"), "b\"a \""), None);
    let round = CallVariant::Round {
        param: "x".to_string(),
        old: 2,
        new: 3,
    };
    assert_eq!(variant_agrees(&round, "1.50"), Some(true));
    assert_eq!(variant_agrees(&round, "-7"), Some(true));
    assert_eq!(variant_agrees(&round, "1.255"), Some(false));
    assert_eq!(variant_agrees(&round, "\"1.5\""), None);
    assert_eq!(variant_agrees(&round, "1e3"), None);
}

#[test]
fn lists_without_two_different_elements_do_not_expose_a_sort_direction_flip() -> Result<(), String>
{
    let finding = classify_top(
        "from src.text import top\n\n\ndef test_top():\n    assert top([4]) == 4\n    assert top([2, 2]) == 2\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value == "call `top` with `xs` holding two or more different elements"),
        "{:?}",
        finding.activation.missing_discriminators
    );
    assert_eq!(finding.ripr.infect.state, StageState::Weak);
    Ok(())
}

#[test]
fn a_list_with_different_elements_keeps_a_sort_direction_flip_exposed() -> Result<(), String> {
    let finding = classify_top(
        "from src.text import top\n\n\ndef test_top():\n    assert top([4]) == 4\n    assert top([4, 1]) == 1\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.ripr.infect.state, StageState::Yes);
    Ok(())
}

#[test]
fn an_unread_list_keeps_a_sort_direction_flip_exposed() -> Result<(), String> {
    let finding = classify_top(
        "from src.text import top\n\nNUMS = [4, 1]\n\n\ndef test_top():\n    assert top([4]) == 4\n    assert top(NUMS) == 1\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_list_the_body_changes_before_the_sort_keeps_its_reading() -> Result<(), String> {
    // `low([4])` sorts `[4, 9]`, where the direction decides the result.
    let finding = classify(
        15,
        "    return sorted(xs)[0]",
        "    return sorted(xs, reverse=True)[0]",
        "from src.text import low\n\n\ndef test_low():\n    assert low([4]) == 4\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_prior_use_spelled_with_a_normalized_identifier_keeps_the_sort_exposed() -> Result<(), String> {
    // #16221: Python NFKC-normalises identifiers, so `K` and `K` are the same
    // parameter. The body appends to the list before the sort, so the sort
    // sees `[4, 9]` and the direction decides the result. The raw prior-use
    // scan must not miss the append and reason from the original singleton.
    let source = "def top(K):\n    K.append(9)\n    return sorted(K)[0]\n";
    let finding = classify_source(
        source,
        3,
        "    return sorted(K)[0]",
        "    return sorted(K, reverse=True)[0]",
        "from src.text import top\n\n\ndef test_top():\n    assert top([4]) == 4\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn only_a_lone_reverse_flag_flip_on_a_named_sort_is_read() {
    assert_eq!(
        call_variant("return sorted(xs)[0]", "return sorted(xs, reverse=True)[0]"),
        Some(CallVariant::Sort {
            param: "xs".to_string(),
            old_call: "sorted(xs)".to_string(),
            new_call: "sorted(xs, reverse=True)".to_string(),
        })
    );
    assert_eq!(
        call_variant("    xs.sort(reverse=True)", "    xs.sort()"),
        Some(CallVariant::Sort {
            param: "xs".to_string(),
            old_call: "xs.sort(reverse=True)".to_string(),
            new_call: "xs.sort()".to_string(),
        })
    );
    // A key changes the order the flag reverses.
    assert_eq!(
        call_variant(
            "return sorted(xs, key=len)",
            "return sorted(xs, key=len, reverse=True)"
        ),
        None
    );
    // An explicit flag that keeps its value is no change of direction.
    assert_eq!(
        call_variant("return sorted(xs)", "return sorted(xs, reverse=False)"),
        None
    );
    // A computed argument, an attribute receiver, or a module's own sort.
    assert_eq!(
        call_variant(
            "return sorted(xs + ys)",
            "return sorted(xs + ys, reverse=True)"
        ),
        None
    );
    assert_eq!(
        call_variant("    self.xs.sort()", "    self.xs.sort(reverse=True)"),
        None
    );
    assert_eq!(
        call_variant("return np.sorted(xs)", "return np.sorted(xs, reverse=True)"),
        None
    );
    // Switching between the two forms, or another edit beside the flag.
    assert_eq!(
        call_variant("return sorted(xs)", "return xs.sort(reverse=True)"),
        None
    );
    assert_eq!(
        call_variant("return sorted(xs)[0]", "return sorted(xs, reverse=True)[1]"),
        None
    );
    // Two sort calls on the line.
    assert_eq!(
        call_variant(
            "return sorted(xs) + sorted(xs)",
            "return sorted(xs, reverse=True) + sorted(xs)"
        ),
        None
    );
}

#[test]
fn sorting_agrees_only_without_two_different_elements() {
    let list = |items: &[&str]| {
        items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(elements_agree(&list(&[])), Some(true));
    assert_eq!(elements_agree(&list(&["4"])), Some(true));
    assert_eq!(elements_agree(&list(&["2", "2"])), Some(true));
    assert_eq!(elements_agree(&list(&["1_000", "1000"])), Some(true));
    // An int and a float are not read: `2**53 + 1` and its float differ.
    assert_eq!(
        elements_agree(&list(&["9007199254740993", "9007199254740993.0"])),
        None
    );
    assert_eq!(elements_agree(&list(&["2", "2.0"])), None);
    assert_eq!(elements_agree(&list(&["\"a\"", "'a'"])), Some(true));
    assert_eq!(elements_agree(&list(&["4", "1"])), Some(false));
    assert_eq!(elements_agree(&list(&["\"a\"", "\"b\""])), Some(false));
    assert_eq!(elements_agree(&list(&["\"a\\n\"", "'a\\n'"])), None);
}

#[test]
fn a_module_round_or_sorted_keeps_its_reading() -> Result<(), String> {
    // The module's own `round` adds its digits, so `money(1.5)` gives 3.5
    // before and 4.5 after; the builtin model must not be applied.
    let round_source =
        "def round(x, n):\n    return x + n\n\n\ndef money(x):\n    return round(x, 2)\n";
    let finding = classify_in(
        round_source,
        6,
        "    return round(x, 2)",
        "    return round(x, 3)",
        "from src.text import money\n\n\ndef test_money():\n    assert money(1.5) == 3.5\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    // The module's own `sorted` ignores the flag here, but the rule cannot
    // know what a rebound name does, so it leaves the reading alone.
    let sorted_source = "def sorted(xs, reverse=False):\n    return list(xs)\n\n\ndef top(xs):\n    return sorted(xs)[0]\n";
    let finding = classify_in(
        sorted_source,
        6,
        "    return sorted(xs)[0]",
        "    return sorted(xs, reverse=True)[0]",
        "from src.text import top\n\n\ndef test_top():\n    assert top([4]) == 4\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    // The same sort with the builtin intact still reads weakly.
    let finding = classify_in(
        "def top(xs):\n    return sorted(xs)[0]\n",
        2,
        "    return sorted(xs)[0]",
        "    return sorted(xs, reverse=True)[0]",
        "from src.text import top\n\n\ndef test_top():\n    assert top([4]) == 4\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn a_parametrized_list_keeps_a_sort_direction_flip_exposed() -> Result<(), String> {
    // The test changes the decorator's list in place before the call.
    let finding = classify_top(
        "import pytest\n\nfrom src.text import top\n\n\n@pytest.mark.parametrize(\"xs\", [[4]])\ndef test_top(xs):\n    xs.append(9)\n    assert top(xs) == 4\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_recursive_owner_keeps_its_reading() -> Result<(), String> {
    // `tidy("a!")` recurses with `"a "`, where `strip` and `lstrip` differ.
    let source = "def tidy(s):\n    if s.endswith(\"!\"):\n        return tidy(s[:-1] + \" \")\n    return s.strip()\n";
    let finding = classify_in(
        source,
        4,
        "    return s.strip()",
        "    return s.lstrip()",
        "from src.text import tidy\n\n\ndef test_tidy():\n    assert tidy(\"a!\") == \"a\"\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_call_shaped_line_inside_a_multiline_string_is_not_read_as_a_call() -> Result<(), String> {
    let root = unique_tempdir("call-variant-multiline-string")?;
    let production = PathBuf::from("src/text.py");
    write_file(
        &root.join(&production),
        "def template(s):\n    return \"\"\"\n    return s.lstrip()\n    \"\"\".strip() + s\n",
    )?;
    write_file(
        &root.join("tests/test_text.py"),
        "from src.text import template\n\n\ndef test_template():\n    assert template(\"a\") == \"return s.strip()a\"\n",
    )?;
    let options = AnalysisOptions {
        root: root.clone(),
        base: None,
        diff_file: None,
        mode: crate::analysis::AnalysisMode::Draft,
        include_unchanged_tests: false,
        resolve_tsconfig_paths: false,
        perl_facts_path: None,
        perl_producer_failure: None,
        git_timeout: None,
        git_candidate: None,
        production_like_targets: Default::default(),
        test_harnesses: Vec::new(),
        resolved_subject_identity: None,
        open_rust_index_paths: Default::default(),
    };
    let changed_line = |text: &str| crate::analysis::diff::ChangedLine {
        line: 3,
        new_side_line: 3,
        text: text.to_string(),
    };
    let changed_files = vec![ChangedFile {
        path: production,
        added_lines: vec![changed_line("    return s.lstrip()")],
        removed_lines: vec![changed_line("    return s.strip()")],
    }];
    let result = PythonAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed_files);
    let cleanup = std::fs::remove_dir_all(&root);
    let result = result?;
    cleanup.map_err(|err| format!("remove_dir_all({}): {err}", root.display()))?;
    let read_as_call = result.findings.iter().any(|finding| {
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value.contains("trailing whitespace"))
    });
    assert!(!read_as_call, "{:#?}", result.findings);
    Ok(())
}

fn classify_score(test_source: &str) -> Result<Finding, String> {
    classify(
        19,
        "    return d.get(\"k\", 0) + 1",
        "    return d.get(\"k\", 5) + 1",
        test_source,
    )
}

#[test]
fn dicts_holding_the_key_do_not_expose_a_get_fallback_change() -> Result<(), String> {
    let finding = classify_score(
        "from src.text import score\n\n\ndef test_score():\n    assert score({\"k\": 2}) == 3\n    assert score({'k': 0, 'j': 9}) == 1\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value == "call `score` with `d` without the key \"k\""),
        "{:?}",
        finding.activation.missing_discriminators
    );
    assert_eq!(finding.ripr.infect.state, StageState::Weak);
    Ok(())
}

#[test]
fn a_dict_without_the_key_keeps_a_get_fallback_change_exposed() -> Result<(), String> {
    let finding = classify_score(
        "from src.text import score\n\n\ndef test_score():\n    assert score({\"k\": 2}) == 3\n    assert score({}) == 1\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.ripr.infect.state, StageState::Yes);
    Ok(())
}

#[test]
fn an_unpacked_dict_keeps_a_get_fallback_change_exposed() -> Result<(), String> {
    let finding = classify_score(
        "from src.text import score\n\nBASE = {}\n\n\ndef test_score():\n    assert score({\"k\": 2}) == 3\n    assert score({**BASE, \"j\": 1}) == 1\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn only_a_lone_literal_fallback_edit_on_a_named_get_is_read() {
    assert_eq!(
        call_variant("return d.get(\"k\", 0) + 1", "return d.get(\"k\", 5) + 1"),
        Some(CallVariant::GetFallback {
            param: "d".to_string(),
            key: "\"k\"".to_string(),
            old: "0".to_string(),
            new: "5".to_string(),
        })
    );
    assert_eq!(
        call_variant("    x = d.get(1, None)", "    x = d.get(1, \"\")"),
        Some(CallVariant::GetFallback {
            param: "d".to_string(),
            key: "1".to_string(),
            old: "None".to_string(),
            new: "\"\"".to_string(),
        })
    );
    // A changed key, a named key or fallback, or an attribute receiver.
    assert_eq!(
        call_variant("return d.get(\"k\", 0)", "return d.get(\"j\", 0)"),
        None
    );
    assert_eq!(
        call_variant("return d.get(k, 0)", "return d.get(k, 5)"),
        None
    );
    assert_eq!(
        call_variant("return d.get(\"k\", a)", "return d.get(\"k\", b)"),
        None
    );
    assert_eq!(
        call_variant("return self.d.get(\"k\", 0)", "return self.d.get(\"k\", 5)"),
        None
    );
    // Another method, or a fallback that is not the last argument.
    assert_eq!(
        call_variant("return d.pop(\"k\", 0)", "return d.pop(\"k\", 5)"),
        None
    );
    assert_eq!(
        call_variant("return d.get(\"k\", 0, x)", "return d.get(\"k\", 5, x)"),
        None
    );
}

#[test]
fn dict_keys_are_read_only_from_a_literal_display() {
    let keys = |items: &[&str]| {
        items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        literal_dict_keys("{\"k\": 2, 3: x}"),
        Some(keys(&["\"k\"", "3"]))
    );
    assert_eq!(literal_dict_keys(" {} "), Some(keys(&[])));
    assert_eq!(literal_dict_keys("{**base}"), None);
    assert_eq!(literal_dict_keys("{f(): 1}"), None);
    assert_eq!(literal_dict_keys("{1, 2}"), None);
    assert_eq!(literal_dict_keys("dict(k=1)"), None);
    assert_eq!(key_present(&keys(&["\"k\""]), "'k'"), Some(true));
    assert_eq!(key_present(&keys(&["\"j\""]), "\"k\""), Some(false));
    assert_eq!(key_present(&keys(&["1"]), "\"1\""), None);
}
