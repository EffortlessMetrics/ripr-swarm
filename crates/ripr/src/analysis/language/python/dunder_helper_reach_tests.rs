//! A helper that only a dunder of its own class calls through `self.` is
//! reached by the test that runs the dunder by syntax (#13776). cachetools:
//! `cache[key] = value` runs `Cache.__setitem__`, which calls the static
//! `self.getsizeof(value)`; `cache[key]` runs `LFUCache.__getitem__`, which
//! calls the name-mangled `self.__touch(key)`. Reach here is weak: it never
//! earns an oracle or `exposed`.

use super::owners_tests::{extract_owners, extract_tests};
use super::related_tests::apply_class_facts;
use super::*;
use std::path::Path;

const CACHE_PY: &str = "class Cache:
    def __init__(self, maxsize):
        self.__data = {}

    def __setitem__(self, key, value):
        size = self.getsizeof(value)
        self.__data[key] = value

    @staticmethod
    def getsizeof(value):
        return 1

    def unused_helper(self):
        return 2


class LFUCache(Cache):
    def __getitem__(self, key):
        value = self.__data[key]
        self.__touch(key)
        return value

    def __touch(self, key):
        curr = self.__links[key]
        if curr.count != 1:
            return 0
        return 1
";

fn finding_at(source: &str, tests_source: &str, needle: &str) -> Result<Finding, String> {
    let mut owners = extract_owners(Path::new("src/cachetools/__init__.py"), source);
    apply_class_facts(&mut owners);
    let tests = extract_tests(Path::new("tests/test_cache.py"), tests_source);
    let (index, line) = source
        .lines()
        .enumerate()
        .find(|(_, line)| line.trim() == needle)
        .ok_or_else(|| format!("fixture must contain `{needle}`"))?;
    classify_change(
        Path::new("src/cachetools/__init__.py"),
        index + 1,
        line,
        &owners,
        &tests,
    )
    .ok_or_else(|| format!("expected a finding for `{needle}`"))
}

fn assert_reached_without_credit(finding: &Finding) {
    assert_ne!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        finding
            .related_tests
            .iter()
            .all(|row| row.oracle.is_none() && row.oracle_strength == OracleStrength::Unknown),
        "transitive reach must not carry an oracle: {:?}",
        finding.related_tests
    );
}

#[test]
fn given_subscript_assignment_when_static_helper_changes_then_reached_not_exposed()
-> Result<(), String> {
    let tests = "from cachetools import Cache\n\n\ndef test_setitem():\n    cache = Cache(maxsize=2)\n    cache[1] = 1\n    assert cache is not None\n";

    let finding = finding_at(CACHE_PY, tests, "return 1")?;

    assert_reached_without_credit(&finding);
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::DynamicDispatch),
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn given_explicit_dunder_call_when_static_helper_changes_then_weakly_exposed_without_oracle()
-> Result<(), String> {
    let tests = "from cachetools import Cache\n\n\ndef test_setitem():\n    cache = Cache(maxsize=2)\n    cache.__setitem__(1, 1)\n    assert cache is not None\n";

    let finding = finding_at(CACHE_PY, tests, "return 1")?;

    assert_reached_without_credit(&finding);
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert_eq!(finding.related_tests.len(), 1, "{finding:?}");
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.starts_with("helper_owner_call: Cache.__setitem__ calls")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn given_subscript_read_when_mangled_helper_changes_then_reached_not_exposed() -> Result<(), String>
{
    let tests = "from cachetools import LFUCache\n\n\ndef test_getitem():\n    cache = LFUCache(maxsize=2)\n    cache[1]\n    assert cache is not None\n";

    let finding = finding_at(CACHE_PY, tests, "if curr.count != 1:")?;

    assert_reached_without_credit(&finding);
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::DynamicDispatch),
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn given_unbound_mixin_receiver_when_helper_changes_then_honest_unknown() -> Result<(), String> {
    let tests = "from cachetools import Cache\n\n\nclass CacheTestMixin:\n    Cache = None\n\n    def test_setitem(self):\n        cache = self.Cache(maxsize=2)\n        cache[1] = 1\n        assert cache is not None\n";

    let finding = finding_at(CACHE_PY, tests, "return 1")?;

    assert_reached_without_credit(&finding);
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::DynamicDispatch),
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn given_helper_no_dunder_calls_when_it_changes_then_still_no_static_path() -> Result<(), String> {
    let tests = "from cachetools import Cache\n\n\ndef test_setitem():\n    cache = Cache(maxsize=2)\n    cache[1] = 1\n    assert cache is not None\n";

    let finding = finding_at(CACHE_PY, tests, "return 2")?;

    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert_eq!(finding.static_limit_kind, None);
    Ok(())
}

#[test]
fn given_subscript_on_a_dict_when_helper_changes_then_still_no_static_path() -> Result<(), String> {
    let tests = "def test_dict():\n    cache = {}\n    cache[1] = 1\n    assert cache[1] == 1\n";

    let finding = finding_at(CACHE_PY, tests, "return 1")?;

    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert_eq!(finding.static_limit_kind, None);
    Ok(())
}
