//! The unread-owner-result-field fact and its paired exact-read credit.
//!
//! The fact is emitted only when the one related test binds the instance a
//! self-returning owner method (or `__init__`) produced, never names the
//! changed field and reads only other attributes of it. Every variant that
//! can reach the field another way, or that is not the one bounded shape,
//! carries no fact and keeps today's orthogonal reading. The repair the fact
//! asks for (`assert r.<field> == <literal>`) is credited `exposed`.

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const SOURCE: &str = "class Req:\n    def __init__(self, url):\n        self.url = url\n        self.timeout_value = 30\n\n    def timeout(self, t):\n        self.timeout_value = t * 2\n        return self\n";
const CHANGED_LINE: usize = 7;
const NEW_LINE: &str = "        self.timeout_value = t * 2";
const OLD_LINE: &str = "        self.timeout_value = t";
const UNREAD_TEST: &str = "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x'\n";
const FACT: &str = "python_owner_result_field_unread: r.timeout_value (test_builder_sets_url)";

fn classify_at(
    source: &str,
    line: usize,
    new_line: &str,
    old_line: &str,
    test_src: &str,
) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/fx/core.py"), source);
    let tests = extract_tests(Path::new("tests/test_t.py"), test_src);
    if tests.is_empty() {
        return Err(format!("fixture must parse a test: {test_src}"));
    }
    if source.lines().nth(line - 1) != Some(new_line) {
        return Err(format!("line {line} must be the changed line in {source}"));
    }
    classify_change_with_source(
        Path::new("src/fx/core.py"),
        line,
        new_line,
        Some(old_line),
        &owners,
        &tests,
        Some(source),
    )
    .ok_or_else(|| "the change must yield a finding".to_string())
}

fn classify(source: &str, test_src: &str) -> Result<Finding, String> {
    classify_at(source, CHANGED_LINE, NEW_LINE, OLD_LINE, test_src)
}

fn has_fact(finding: &Finding) -> bool {
    finding
        .evidence
        .iter()
        .any(|line| line.starts_with("python_owner_result_field_unread:"))
}

fn assert_orthogonal_without_fact(case: &str, finding: &Finding) -> Result<(), String> {
    if has_fact(finding) {
        return Err(format!(
            "{case}: no unread fact expected: {:?}",
            finding.evidence
        ));
    }
    // Other credits may still read some variants (a second owner call whose
    // `.url` is compared directly); only this module's credit is excluded.
    if finding.alignment_reason.as_deref() == Some("strong_oracle_observes_owner_result_field") {
        return Err(format!(
            "{case}: must not take the owner-result field credit: {:?}",
            finding.evidence
        ));
    }
    Ok(())
}

#[test]
fn builder_result_with_unread_field_carries_the_fact() -> Result<(), String> {
    let finding = classify(SOURCE, UNREAD_TEST)?;
    if finding.class != ExposureClass::WeaklyExposed
        || finding.oracle_alignment.as_deref() != Some("orthogonal")
        || finding.alignment_reason.as_deref() != Some("strong_oracle_observes_different_sink")
    {
        return Err(format!(
            "the unread shape keeps its orthogonal weak reading: {:?} {:?} {:?}",
            finding.class, finding.oracle_alignment, finding.alignment_reason
        ));
    }
    let facts: Vec<&String> = finding
        .evidence
        .iter()
        .filter(|line| line.starts_with("python_owner_result_field_unread:"))
        .collect();
    if facts != [FACT] {
        return Err(format!(
            "exactly the one fact expected: {:?}",
            finding.evidence
        ));
    }
    Ok(())
}

#[test]
fn init_owner_result_with_unread_field_carries_the_fact() -> Result<(), String> {
    let source = "class Req:\n    def __init__(self, url):\n        self.url = url\n        self.timeout_value = 60\n";
    let test = "from fx.core import Req\n\n\ndef test_builds_url():\n    r = Req('http://x')\n    assert r.url == 'http://x'\n";
    let finding = classify_at(
        source,
        4,
        "        self.timeout_value = 60",
        "        self.timeout_value = 30",
        test,
    )?;
    if finding.class != ExposureClass::WeaklyExposed
        || finding.oracle_alignment.as_deref() != Some("orthogonal")
        || !finding.evidence.iter().any(|line| {
            line == "python_owner_result_field_unread: r.timeout_value (test_builds_url)"
        })
    {
        return Err(format!(
            "the __init__ shape must carry the fact: {:?}",
            finding.evidence
        ));
    }
    Ok(())
}

#[test]
fn test_variants_that_can_reach_the_field_carry_no_fact() -> Result<(), String> {
    for (case, test) in [
        (
            "the test also reads the field",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x'\n    assert r.timeout_value > 0\n",
        ),
        (
            "the field is named in a string",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x', 'timeout_value'\n",
        ),
        (
            "the binding is passed to a helper",
            "from fx.core import Req\n\n\ndef check(req):\n    return req.timeout_value\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    check(r)\n    assert r.url == 'http://x'\n",
        ),
        (
            "the binding is rebound",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    r = Req('http://y')\n    assert r.url == 'http://y'\n",
        ),
        (
            "the binding is rebound by a walrus",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert (r := Req('http://y')).url == 'http://y'\n",
        ),
        (
            "the binding is compared whole",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x'\n    assert r != None\n",
        ),
        (
            "a method of the binding is called",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.describe() == 'x'\n",
        ),
        (
            "the binding is formatted",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x'\n    assert f'{r}' != ''\n",
        ),
        (
            "the binding is read under a branch",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    if r.url:\n        assert r.url == 'http://x'\n",
        ),
        (
            "the owner is called a second time",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.url == Req('http://x').timeout(5).url\n",
        ),
        (
            "the field is read through getattr",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x'\n    assert getattr(r, 'url') == 'http://x'\n",
        ),
        (
            "the owner is patched",
            "from unittest.mock import patch\nfrom fx.core import Req\n\n\n@patch.object(Req, 'timeout')\ndef test_builder_sets_url(timeout):\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x'\n",
        ),
    ] {
        let finding = classify(SOURCE, test)?;
        assert_orthogonal_without_fact(case, &finding)?;
    }
    Ok(())
}

#[test]
fn two_related_tests_carry_no_fact() -> Result<(), String> {
    let test = format!(
        "{UNREAD_TEST}\n\ndef test_builder_sets_url_again():\n    s = Req('http://y').timeout(3)\n    assert s.url == 'http://y'\n"
    );
    let finding = classify(SOURCE, &test)?;
    if finding.related_tests.len() < 2 {
        return Err(format!(
            "fixture must relate both tests: {:?}",
            finding.related_tests
        ));
    }
    assert_orthogonal_without_fact("two related tests", &finding)
}

#[test]
fn owner_variants_that_hide_the_write_carry_no_fact() -> Result<(), String> {
    for (case, source) in [
        (
            "the owner returns a new object",
            "class Req:\n    def __init__(self, url):\n        self.url = url\n        self.timeout_value = 30\n\n    def timeout(self, t):\n        self.timeout_value = t * 2\n        return Req(self.url)\n",
        ),
        (
            "the owner returns nothing",
            "class Req:\n    def __init__(self, url):\n        self.url = url\n        self.timeout_value = 30\n\n    def timeout(self, t):\n        self.timeout_value = t * 2\n        return None\n",
        ),
        (
            "the class defines __getattr__",
            "class Req:\n    def __init__(self, url):\n        self.url = url\n        self.timeout_value = 30\n\n    def timeout(self, t):\n        self.timeout_value = t * 2\n        return self\n\n    def __getattr__(self, name):\n        return self.timeout_value\n",
        ),
        (
            "the class aliases the field through a property",
            "class Req:\n    def __init__(self, url):\n        self._url = url\n        self.timeout_value = 30\n\n    def timeout(self, t):\n        self.timeout_value = t * 2\n        return self\n\n    @property\n    def url(self):\n        return self.timeout_value\n",
        ),
        (
            "the class has a base",
            "class Req(Base):\n    def __init__(self, url):\n        self.url = url\n        self.timeout_value = 30\n\n    def timeout(self, t):\n        self.timeout_value = t * 2\n        return self\n",
        ),
        (
            "the write is conditional",
            "class Req:\n    def __init__(self, url):\n        self.url = url\n        self.timeout_value = 30\n\n    def timeout(self, t):\n        if t > 0:\n            self.timeout_value = t * 2\n        return self\n",
        ),
        (
            "an early return can skip the write",
            "class Req:\n    def __init__(self, url):\n        self.url = url\n        self.timeout_value = 30\n\n    def timeout(self, t):\n        self.timeout_value = t * 2\n        if t < 0:\n            return self\n        return self\n",
        ),
    ] {
        let line = source
            .lines()
            .position(|line| line.trim() == NEW_LINE.trim())
            .map(|index| index + 1)
            .ok_or_else(|| format!("{case}: changed line missing"))?;
        let new_line = source.lines().nth(line - 1).unwrap_or_default();
        let old_line = new_line.replace("t * 2", "t");
        let finding = classify_at(source, line, new_line, &old_line, UNREAD_TEST)?;
        assert_orthogonal_without_fact(case, &finding)?;
    }
    Ok(())
}

#[test]
fn exact_read_of_the_bound_field_is_credited() -> Result<(), String> {
    let test = "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x'\n    assert r.timeout_value == 20\n";
    let finding = classify(SOURCE, test)?;
    if finding.class != ExposureClass::Exposed
        || finding.oracle_alignment.as_deref() != Some("direct")
        || finding.alignment_reason.as_deref() != Some("strong_oracle_observes_owner_result_field")
        || has_fact(&finding)
    {
        return Err(format!(
            "the repaired test must read exposed: {:?} {:?} {:?} {:?}",
            finding.class, finding.oracle_alignment, finding.alignment_reason, finding.evidence
        ));
    }
    Ok(())
}

#[test]
fn inexact_or_unbounded_field_reads_are_not_credited() -> Result<(), String> {
    for (case, test, source) in [
        (
            "a relational read",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.timeout_value > 0\n",
            SOURCE,
        ),
        (
            "a read against a computed value",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.timeout_value == r.timeout_value\n",
            SOURCE,
        ),
        (
            "a read on an owner returning a new object",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.timeout_value == 20\n",
            "class Req:\n    def __init__(self, url):\n        self.url = url\n        self.timeout_value = 30\n\n    def timeout(self, t):\n        self.timeout_value = t * 2\n        return Req(self.url)\n",
        ),
    ] {
        let finding = classify(source, test)?;
        if finding.alignment_reason.as_deref() == Some("strong_oracle_observes_owner_result_field")
        {
            return Err(format!(
                "{case}: must not take the owner-result field credit: {:?}",
                finding.evidence
            ));
        }
    }
    Ok(())
}

/// The write's effect on the returned instance must stay unread by anything
/// but the test: a later call or bare `self` can overwrite it, and a read of
/// the field anywhere in the class can derive another attribute the test
/// already observes. Each variant keeps the orthogonal reading (asserted as
/// a precondition) but carries no fact, and an exact read of the field gets
/// no owner-result credit.
#[test]
fn owner_variants_that_overwrite_or_derive_from_the_write_carry_no_fact() -> Result<(), String> {
    let head = "class Req:\n    def __init__(self, url):\n        self.url = url\n        self.timeout_value = 30\n\n";
    for (case, body) in [
        (
            "a later call can reset the field",
            "    def timeout(self, t):\n        self.timeout_value = t * 2\n        self._reset()\n        return self\n\n    def _reset(self):\n        self.timeout_value = 30\n",
        ),
        (
            "a later alias of self can overwrite it",
            "    def timeout(self, t):\n        self.timeout_value = t * 2\n        s = self\n        s.timeout_value = 30\n        return self\n",
        ),
        (
            "setattr can overwrite it",
            "    def timeout(self, t):\n        self.timeout_value = t * 2\n        setattr(self, 'timeout_value', 30)\n        return self\n",
        ),
        (
            "a later attribute derives from it",
            "    def timeout(self, t):\n        self.timeout_value = t * 2\n        self.url = self.url + str(self.timeout_value)\n        return self\n",
        ),
        (
            "another method reads it",
            "    def timeout(self, t):\n        self.timeout_value = t * 2\n        return self\n\n    def describe(self):\n        return self.timeout_value\n",
        ),
    ] {
        let source = format!("{head}{body}");
        let line = source
            .lines()
            .position(|line| line.trim() == NEW_LINE.trim())
            .map(|index| index + 1)
            .ok_or_else(|| format!("{case}: changed line missing"))?;
        let finding = classify_at(&source, line, NEW_LINE, OLD_LINE, UNREAD_TEST)?;
        if finding.class != ExposureClass::WeaklyExposed
            || finding.oracle_alignment.as_deref() != Some("orthogonal")
        {
            return Err(format!(
                "{case}: precondition, the orthogonal weak reading: {:?} {:?} {:?}",
                finding.class, finding.oracle_alignment, finding.alignment_reason
            ));
        }
        assert_orthogonal_without_fact(case, &finding)?;
        let exact = "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x'\n    assert r.timeout_value == 30\n";
        let read = classify_at(&source, line, NEW_LINE, OLD_LINE, exact)?;
        if read.alignment_reason.as_deref() == Some("strong_oracle_observes_owner_result_field") {
            return Err(format!(
                "{case}: an exact read must not take the owner-result credit"
            ));
        }
    }
    Ok(())
}

/// A class-level value other than a plain literal may be a custom
/// descriptor that reads or writes the changed field under the name the test
/// reads, and its code lives outside the class; a `match` guard after the
/// write hides a call. Each keeps the orthogonal reading but carries no fact.
#[test]
fn descriptor_or_hidden_expression_shapes_carry_no_fact() -> Result<(), String> {
    let method =
        "    def timeout(self, t):\n        self.timeout_value = t * 2\n        return self\n";
    let init =
        "    def __init__(self, url):\n        self.url = url\n        self.timeout_value = 30\n\n";
    for (case, source) in [
        (
            "a class-level call binding",
            format!("class Req:\n    other = Alias('timeout_value')\n\n{init}{method}"),
        ),
        (
            "a class-level name binding",
            format!(
                "_alias = Alias('timeout_value')\n\n\nclass Req:\n    other = _alias\n\n{init}{method}"
            ),
        ),
        (
            "a decorated method",
            format!(
                "class Req:\n{init}{method}\n    @alias('timeout_value')\n    def other(self):\n        return 0\n"
            ),
        ),
        (
            "a match guard after the write",
            format!(
                "class Req:\n{init}    def timeout(self, t):\n        self.timeout_value = t * 2\n        match [t]:\n            case _ if self._reset() is None:\n                pass\n        return self\n"
            ),
        ),
        (
            "a read by string name",
            format!(
                "class Req:\n{init}{method}\n    def other(self):\n        return getattr(self, 'timeout_value')\n"
            ),
        ),
    ] {
        let line = source
            .lines()
            .position(|line| line.trim() == NEW_LINE.trim())
            .map(|index| index + 1)
            .ok_or_else(|| format!("{case}: changed line missing"))?;
        let finding = classify_at(&source, line, NEW_LINE, OLD_LINE, UNREAD_TEST)?;
        if finding.class != ExposureClass::WeaklyExposed
            || finding.oracle_alignment.as_deref() != Some("orthogonal")
        {
            return Err(format!(
                "{case}: precondition, the orthogonal weak reading: {:?} {:?} {:?}",
                finding.class, finding.oracle_alignment, finding.alignment_reason
            ));
        }
        assert_orthogonal_without_fact(case, &finding)?;
    }
    Ok(())
}

/// Each variant is refused by one guard alone: the orthogonal weak reading
/// holds (asserted as a precondition) and nothing else in the fixture would
/// refuse the fact. The changed right-hand side may not call (`t.pop()` can
/// mutate what the test reads), bind with a walrus or use a bare `self`; the
/// changed statement is alone on its line; and the dynamic-access markers
/// (`__getattr`, `property`, `getattr(`, `patch`, `mock`) each refuse.
#[test]
fn single_guard_variants_carry_no_fact() -> Result<(), String> {
    let head = "class Req:\n    def __init__(self, url):\n        self.url = url\n        self.retries = 0\n        self.timeout_value = 30\n\n    def timeout(self, t):\n";
    let tail = "        return self\n";
    let mut cases: Vec<(&str, String, &str, String)> = Vec::new();
    for (case, pre, new_line, old_line, extra) in [
        (
            "an alias of self made before the write overwrites it",
            "        s = self\n",
            "        self.timeout_value = t * 2",
            "        self.timeout_value = t",
            "",
        ),
        (
            "the changed right-hand side calls",
            "",
            "        self.timeout_value = t.pop()",
            "        self.timeout_value = t",
            "",
        ),
        (
            "the changed right-hand side binds a walrus",
            "",
            "        self.timeout_value = (u := t * 2)",
            "        self.timeout_value = t",
            "",
        ),
        (
            "the changed right-hand side uses a bare self",
            "",
            "        self.timeout_value = [self, t][1]",
            "        self.timeout_value = t",
            "",
        ),
        (
            "another statement shares the changed line",
            "",
            "        self.retries = 3; self.timeout_value = t * 2",
            "        self.retries = 3; self.timeout_value = t",
            "",
        ),
        (
            "the class defines __getattr__",
            "",
            "        self.timeout_value = t * 2",
            "        self.timeout_value = t",
            "\n    def __getattr__(self, name):\n        return 30\n",
        ),
        (
            "the class text names property",
            "",
            "        self.timeout_value = t * 2",
            "        self.timeout_value = t",
            "\n    def describe(self):\n        return 'property'\n",
        ),
        (
            "another method stores the field",
            "",
            "        self.timeout_value = t * 2",
            "        self.timeout_value = t",
            "\n    def _clear(self):\n        self.timeout_value = 0\n",
        ),
        (
            "the field is stored through another base",
            "",
            "        self.timeout_value = t * 2",
            "        self.timeout_value = t",
            "\n    def zero(self):\n        self.me.timeout_value = 0\n",
        ),
        (
            "another method keeps a bound method on the instance",
            "",
            "        self.timeout_value = t * 2",
            "        self.timeout_value = t",
            "\n    def hook(self):\n        self.again = self.__init__\n",
        ),
        (
            "another method aliases self",
            "",
            "        self.timeout_value = t * 2",
            "        self.timeout_value = t",
            "\n    def link(self):\n        self.me = self\n",
        ),
        (
            "another method keeps a closure that can overwrite the field",
            "",
            "        self.timeout_value = t * 2",
            "        self.timeout_value = t",
            "\n    def hook(self):\n        def clear():\n            self.timeout_value = 0\n        self.on_send = clear\n",
        ),
        (
            "another method keeps a lambda",
            "",
            "        self.timeout_value = t * 2",
            "        self.timeout_value = t",
            "\n    def hook(self):\n        self.on_send = lambda: 0\n",
        ),
        (
            "another method names its first parameter otherwise",
            "",
            "        self.timeout_value = t * 2",
            "        self.timeout_value = t",
            "\n    def leak(this):\n        REG.append(this)\n",
        ),
        (
            "another method uses try",
            "",
            "        self.timeout_value = t * 2",
            "        self.timeout_value = t",
            "\n    def leak(self):\n        try:\n            pass\n        except ValueError:\n            pass\n",
        ),
    ] {
        let after = if pre.is_empty() {
            ""
        } else {
            "        s.timeout_value = 30\n"
        };
        let source = format!("{head}{pre}{new_line}\n{after}{tail}{extra}");
        cases.push((case, source, new_line, old_line.to_string()));
    }
    for (case, source, new_line, old_line) in &cases {
        let line = source
            .lines()
            .position(|line| line == *new_line)
            .map(|index| index + 1)
            .ok_or_else(|| format!("{case}: changed line missing"))?;
        let finding = classify_at(source, line, new_line, old_line, UNREAD_TEST)?;
        assert_weak_orthogonal_without_fact(case, &finding)?;
        // The exact-read credit shares the guard: reading `r.retries`, which
        // the semicolon variant also writes, must not read as exposed.
        for read in ["retries == 3", "timeout_value == 20"] {
            let exact = format!(
                "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x'\n    assert r.{read}\n"
            );
            let credited = classify_at(source, line, new_line, old_line, &exact)?;
            if credited.alignment_reason.as_deref()
                == Some("strong_oracle_observes_owner_result_field")
            {
                return Err(format!(
                    "{case}: `r.{read}` must not take the owner-result credit"
                ));
            }
        }
    }
    let with_reset = format!("{SOURCE}\n    def reset(self):\n        self.retries = 0\n");
    let reset_test = "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    reset = r.reset\n    reset()\n    assert r.url == 'http://x'\n";
    let finding = classify(&with_reset, reset_test)?;
    assert_weak_orthogonal_without_fact("the test aliases a bound method", &finding)?;
    for (case, test) in [
        (
            "the test calls getattr",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    r = Req('http://x').timeout(10)\n    x = getattr(object, 'mro', None)\n    assert r.url == 'http://x'\n",
        ),
        (
            "the test text names patch",
            "from fx.core import Req\n\n\ndef test_builder_sets_url():\n    label = 'patch'\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x'\n",
        ),
        (
            "the test mocks",
            "from unittest import mock\nfrom fx.core import Req\n\n\ndef test_builder_sets_url():\n    m = mock.Mock()\n    r = Req('http://x').timeout(10)\n    assert r.url == 'http://x'\n",
        ),
    ] {
        let finding = classify(SOURCE, test)?;
        assert_weak_orthogonal_without_fact(case, &finding)?;
    }
    Ok(())
}

fn assert_weak_orthogonal_without_fact(case: &str, finding: &Finding) -> Result<(), String> {
    if finding.class != ExposureClass::WeaklyExposed
        || finding.oracle_alignment.as_deref() != Some("orthogonal")
    {
        return Err(format!(
            "{case}: precondition, the orthogonal weak reading: {:?} {:?} {:?}",
            finding.class, finding.oracle_alignment, finding.alignment_reason
        ));
    }
    assert_orthogonal_without_fact(case, finding)
}
