//! Regression coverage for `source_facts.rs` producer branches that no other
//! suite pins: import-line spans, docstring boundary lines, the syntax-versus-
//! budget parse-error split, the snapshot observation score, the conftest
//! file-name gate, the lone-carriage-return decorator distrust, and the
//! print/log call receiver screen.

use super::source_facts::{
    PythonSourceFactKind, PythonSourceFacts, PythonSourceLimitation, extract_source_facts,
    source_fact_snapshot_observation, source_facts_parse_error, source_facts_syntax_parse_error,
};
use crate::domain::StaticLimitKind;
use std::path::Path;

fn facts_of(path: &str, source: &str) -> PythonSourceFacts {
    extract_source_facts(Path::new(path), source)
}

fn fact_texts(facts: &PythonSourceFacts, kind: PythonSourceFactKind) -> Vec<&str> {
    facts
        .facts
        .iter()
        .filter(|fact| fact.kind == kind)
        .map(|fact| fact.text.as_str())
        .collect()
}

fn spans(ranges: &[std::ops::RangeInclusive<usize>]) -> Vec<(usize, usize)> {
    ranges
        .iter()
        .map(|range| (*range.start(), *range.end()))
        .collect()
}

#[test]
fn import_line_ranges_cover_every_scope_and_multiline_spans() {
    let source = concat!(
        "import os\n",
        "from a import (\n",
        "    b,\n",
        "    c,\n",
        ")\n",
        "flag = 1\n",
        "def f():\n",
        "    import json\n",
        "    return json\n",
        "class Cls:\n",
        "    from d import e\n",
        "if flag:\n",
        "    import g\n",
        "else:\n",
        "    import h\n",
        "try:\n",
        "    import i\n",
        "except ImportError:\n",
        "    import j\n",
        "else:\n",
        "    import k\n",
        "finally:\n",
        "    import l\n",
        "for _ in flag:\n",
        "    import m\n",
        "else:\n",
        "    import n\n",
        "while flag:\n",
        "    import o\n",
        "    break\n",
        "with flag:\n",
        "    import p\n",
        "async def af():\n",
        "    import q\n",
        "    async with flag:\n",
        "        import r\n",
        "    async for _ in flag:\n",
        "        import s\n",
        "match flag:\n",
        "    case 1:\n",
        "        import t\n",
    );
    let facts = facts_of("src/imports.py", source);
    assert!(facts.limitations.is_empty(), "{:?}", facts.limitations);
    let got = spans(&facts.import_line_ranges);
    let expected = [
        (1, 1),
        (2, 5),
        (8, 8),
        (11, 11),
        (13, 13),
        (15, 15),
        (17, 17),
        (19, 19),
        (21, 21),
        (23, 23),
        (25, 25),
        (27, 27),
        (29, 29),
        (32, 32),
        (34, 34),
        (36, 36),
        (38, 38),
        (41, 41),
    ];
    for span in expected {
        assert!(got.contains(&span), "missing {span:?} in {got:?}");
    }
    assert_eq!(
        got.len(),
        expected.len(),
        "no non-import statement may be a span: {got:?}"
    );
}

#[test]
fn docstring_ranges_keep_lines_shared_with_code_analyzable() {
    // A docstring followed by `;` code on its closing line leaves that line out.
    let trailing = "def f():\n    \"\"\"doc\n    more\"\"\"; x = 1\n    return x\n";
    assert_eq!(
        spans(&facts_of("a.py", trailing).docstring_line_ranges),
        vec![(2, 2)]
    );
    // A one-line docstring sharing its line with code is not a docstring line.
    let single = "def f():\n    \"\"\"doc\"\"\"; x = 1\n    return x\n";
    assert!(
        facts_of("a.py", single).docstring_line_ranges.is_empty(),
        "a semicolon-shared single line must stay analyzable"
    );
    // A docstring that opens on the `def` line leaves the header line out.
    let header = "def f(): \"\"\"doc\n    more\"\"\"\n";
    assert_eq!(
        spans(&facts_of("a.py", header).docstring_line_ranges),
        vec![(2, 2)]
    );
    let header_single = "def f(): \"\"\"doc\"\"\"\n";
    assert!(
        facts_of("a.py", header_single)
            .docstring_line_ranges
            .is_empty()
    );
    // A clean one-line docstring is wholly a docstring line.
    let clean = "def f():\n    \"\"\"doc\"\"\"\n    return 1\n";
    assert_eq!(
        spans(&facts_of("a.py", clean).docstring_line_ranges),
        vec![(2, 2)]
    );
}

#[test]
fn docstring_ranges_cover_nested_scopes_and_skip_non_docstrings() {
    let source = concat!(
        "class Cls:\n",
        "    \"\"\"class doc\"\"\"\n",
        "    def m(self):\n",
        "        '''method doc'''\n",
        "        return 1\n",
        "async def af():\n",
        "    \"\"\"async doc\"\"\"\n",
        "if True:\n",
        "    def g():\n",
        "        \"\"\"nested doc\"\"\"\n",
        "try:\n",
        "    def h():\n",
        "        \"\"\"try doc\"\"\"\n",
        "except ValueError:\n",
        "    def k():\n",
        "        \"\"\"handler doc\"\"\"\n",
        "def not_doc():\n",
        "    value = 1\n",
        "    \"\"\"second string\"\"\"\n",
        "    return value\n",
        "def bytes_first():\n",
        "    b\"\"\"bytes\"\"\"\n",
        "    return 1\n",
    );
    let facts = facts_of("src/doc.py", source);
    assert!(facts.limitations.is_empty(), "{:?}", facts.limitations);
    assert_eq!(
        spans(&facts.docstring_line_ranges),
        vec![(2, 2), (4, 4), (7, 7), (10, 10), (13, 13), (16, 16)],
        "only first plain strings of def/class scopes are docstring lines"
    );
}

#[test]
fn syntax_parse_error_excludes_budget_refusals() {
    let syntax = facts_of("src/oops.py", "def broken(:\n    pass\n");
    assert!(source_facts_parse_error(&syntax).is_some());
    assert!(source_facts_syntax_parse_error(&syntax).is_some());

    let deep = format!("def f(x):\n    return {}x\n", "not ".repeat(5_000));
    let budget = facts_of("src/deep.py", &deep);
    let row = source_facts_parse_error(&budget);
    assert!(
        row.is_some_and(|row| row.evidence.contains("parse_budget:")),
        "{:?}",
        budget.limitations
    );
    assert!(
        source_facts_syntax_parse_error(&budget).is_none(),
        "a budget refusal must not be promoted as a syntax failure"
    );

    let valid = facts_of("src/ok.py", "def f():\n    return 1\n");
    assert!(source_facts_parse_error(&valid).is_none());
    assert!(source_facts_syntax_parse_error(&valid).is_none());
}

#[test]
fn parse_error_lookups_ignore_other_unsupported_syntax_rows() {
    let mut facts = facts_of("src/ok.py", "def f():\n    return 1\n");
    facts.limitations.push(PythonSourceLimitation {
        kind: StaticLimitKind::UnsupportedSyntax,
        evidence: "something_else".to_string(),
        missing: String::new(),
    });
    assert!(source_facts_parse_error(&facts).is_none());
    assert!(source_facts_syntax_parse_error(&facts).is_none());
}

#[test]
fn malformed_snapshot_observation_is_exactly_the_limitation_weight() -> Result<(), String> {
    let facts = facts_of("src/oops.py", "def broken(:\n    pass\n");
    let limitation = facts
        .limitations
        .first()
        .ok_or_else(|| "expected a parse limitation".to_string())?;
    let expected = 2 // `src`, `oops.py`
        + "python".len()
        + limitation.kind.as_str().len()
        + limitation.evidence.len()
        + limitation.missing.len();
    assert_eq!(source_fact_snapshot_observation(&facts), expected);
    assert!(source_fact_snapshot_observation(&facts_of("ok.py", "flag = 1\n")) > 0);
    Ok(())
}

#[test]
fn snapshot_observation_grows_with_extracted_facts() {
    let small = facts_of("m.py", "flag = 1\n");
    let large = facts_of("m.py", "x = 1\n\ndef f(a):\n    return a + 1\n");
    assert!(source_fact_snapshot_observation(&large) > source_fact_snapshot_observation(&small));
}

#[test]
fn conftest_implicit_setup_requires_the_conftest_file_name() {
    let source = "import pytest\n\n\n@pytest.fixture(autouse=True)\ndef setup():\n    yield\n";
    assert!(facts_of("tests/conftest.py", source).conftest_implicit_setup);
    assert!(!facts_of("tests/helpers.py", source).conftest_implicit_setup);
    assert!(!facts_of("tests/test_conftest.py", source).conftest_implicit_setup);
    assert!(
        !facts_of("tests/conftest.py", "def helper():\n    return 1\n").conftest_implicit_setup,
        "a conftest without installed setup has no implicit setup"
    );
}

fn unplain_decorators_of(source: &str, owner: &str) -> Option<Vec<String>> {
    facts_of("src/m.py", source)
        .owners
        .into_iter()
        .find(|candidate| candidate.name == owner)
        .map(|owner| owner.unplain_decorators)
}

#[test]
fn lone_carriage_return_distrusts_typing_markers_but_crlf_does_not() {
    let body = "from typing import final\n@final\ndef rate(n):\n    return n + 1\n";
    assert_eq!(
        unplain_decorators_of(body, "rate"),
        Some(Vec::new()),
        "plain LF source keeps the marker trusted"
    );
    let crlf = body.replace('\n', "\r\n");
    assert_eq!(
        unplain_decorators_of(&crlf, "rate"),
        Some(Vec::new()),
        "CRLF is a line break `str::lines` sees, so it must not distrust"
    );
    let lone_cr = body.replace('\n', "\r");
    let got = unplain_decorators_of(&lone_cr, "rate");
    assert_ne!(
        got,
        Some(Vec::new()),
        "lone CR must not be trusted: {got:?}"
    );
}

#[test]
fn star_import_withholds_trust_for_every_decorator() {
    let source =
        "from typing import final\nfrom helpers import *\n\n@final\ndef rate(n):\n    return n\n";
    assert_eq!(
        unplain_decorators_of(source, "rate"),
        Some(vec!["final".to_string()])
    );
}

#[test]
fn print_and_log_calls_are_classified_by_receiver_name() {
    let source = concat!(
        "import logging\n",
        "def f(self, logger):\n",
        "    print('x')\n",
        "    logging.info('a')\n",
        "    logger.exception('b')\n",
        "    logger.log(1, 'c')\n",
        "    self.logger.info('d')\n",
        "    logging.getLogger('n')\n",
        "    logger.setLevel(1)\n",
        "    other.info('e')\n",
    );
    let facts = facts_of("src/log.py", source);
    assert_eq!(
        fact_texts(&facts, PythonSourceFactKind::PrintCall),
        vec!["print('x')"]
    );
    assert_eq!(
        fact_texts(&facts, PythonSourceFactKind::LogCall),
        vec![
            "logging.info('a')",
            "logger.exception('b')",
            "logger.log(1, 'c')"
        ],
        "only logging./logger. receivers with a log-level method are log calls"
    );
}

#[test]
fn statement_forms_emit_owner_attributed_facts() {
    let source = concat!(
        "class Box:\n",
        "    def update(self, n):\n",
        "        self.total: int = n\n",
        "        self.total += 1\n",
        "        label = 'a' if n > 0 else 'b'\n",
        "        items = [x for x in range(n) if x != 2]\n",
        "        try:\n",
        "            risky()\n",
        "        except (ValueError, KeyError):\n",
        "            raise RuntimeError('bad') from None\n",
        "        return label, items\n",
    );
    let facts = facts_of("src/box.py", source);
    assert!(facts.limitations.is_empty(), "{:?}", facts.limitations);
    assert_eq!(
        fact_texts(&facts, PythonSourceFactKind::AttributeWrite),
        vec!["self.total", "self.total"]
    );
    assert!(
        !fact_texts(&facts, PythonSourceFactKind::Predicate).is_empty(),
        "conditional expression and comprehension filter emit predicates"
    );
    assert_eq!(
        fact_texts(&facts, PythonSourceFactKind::Raise),
        vec!["raise RuntimeError('bad') from None"]
    );
    assert!(
        facts
            .facts
            .iter()
            .filter(|fact| fact.kind != PythonSourceFactKind::Module)
            .all(|fact| fact.owner.as_deref().is_some_and(|o| o.starts_with("Box"))),
        "every non-module fact in the class carries its class-qualified owner"
    );
    assert!(
        facts
            .facts
            .iter()
            .any(|fact| fact.kind == PythonSourceFactKind::Method
                && fact.owner.as_deref() == Some("Box.update"))
    );
}

#[test]
fn module_fact_spans_the_whole_source_and_empty_module_is_clean() -> Result<(), String> {
    let source = "flag = 1\n";
    let facts = facts_of("m.py", source);
    let module = facts
        .facts
        .iter()
        .find(|fact| fact.kind == PythonSourceFactKind::Module)
        .ok_or_else(|| "expected module fact".to_string())?;
    assert_eq!((module.start_byte, module.end_byte), (0, source.len()));
    assert_eq!(module.owner, None);

    let empty = facts_of("empty.py", "");
    assert!(empty.limitations.is_empty());
    assert!(source_facts_parse_error(&empty).is_none());
    assert!(empty.docstring_line_ranges.is_empty());
    assert!(empty.import_line_ranges.is_empty());
    Ok(())
}

#[test]
fn module_rebinding_lists_are_shared_across_tests_not_copied_per_test() {
    // Each test used to receive its own copy of every module-level name, so a
    // file of N tests held N copies of N names (a 1 MB test file exhausted
    // memory). Sharing keeps the same facts at O(N).
    let mut source = String::from("import os\nvalue = 1\nvalue = 2\n");
    for index in 0..400 {
        source.push_str(&format!(
            "def test_{index}():\n    assert {index} == {index}\n"
        ));
    }
    let facts = facts_of("tests/test_many.py", &source);
    assert_eq!(facts.tests.len(), 400);
    let first = &facts.tests[0].constant_rebinding;
    // Control: the shared lists are populated, so the pointer equality below
    // compares real data rather than empty defaults.
    assert!(first.module_bound.iter().any(|name| name == "test_399"));
    assert!(first.module_names.iter().any(|name| name == "value"));
    assert!(first.rebinds("value"));
    assert!(!first.rebinds("os"));
    for test in &facts.tests[1..] {
        let bindings = &test.constant_rebinding;
        assert!(std::sync::Arc::ptr_eq(
            &first.module_bound,
            &bindings.module_bound
        ));
        assert!(std::sync::Arc::ptr_eq(
            &first.module_names,
            &bindings.module_names
        ));
        assert!(std::sync::Arc::ptr_eq(
            &first.attributes,
            &bindings.attributes
        ));
    }
}
