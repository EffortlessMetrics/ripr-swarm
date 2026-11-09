//! A helper that only a dunder of its own class calls through `self.` may be
//! reached by a test that references the class through an import and uses
//! that dunder's protocol syntax somewhere in its body (#13776). cachetools:
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
        return curr.count
";

fn finding_at(source: &str, tests_source: &str, needle: &str) -> Result<Finding, String> {
    let mut owners = extract_owners(Path::new("src/cachetools/__init__.py"), source);
    apply_class_facts(&mut owners);
    let tests = extract_tests(Path::new("tests/test_cache.py"), tests_source);
    // A fixture that collects no test would read as an honest unknown.
    assert_eq!(tests.len(), 1, "fixture must collect exactly one test");
    let matches: Vec<(usize, &str)> = source
        .lines()
        .enumerate()
        .filter(|(_, line)| line.trim() == needle)
        .collect();
    // The needle names exactly one changed line, so the finding's owner is
    // the one the test means.
    assert_eq!(matches.len(), 1, "fixture must contain `{needle}` once");
    let (index, line) = matches[0];
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
fn given_unbound_mixin_receiver_when_helper_changes_then_no_reach_claimed() -> Result<(), String> {
    // `self.Cache` is not an import-provenanced reference to the owner's
    // class, so the adapter claims nothing: wrong reach is worse than a miss.
    let tests = "from cachetools import Cache\n\n\nclass TestCache:\n    Cache = None\n\n    def test_setitem(self):\n        cache = self.Cache(maxsize=2)\n        cache[1] = 1\n        assert cache is not None\n";

    let finding = finding_at(CACHE_PY, tests, "return 1")?;

    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert_eq!(finding.static_limit_kind, None, "{finding:?}");
    Ok(())
}

#[test]
fn given_same_named_class_from_another_module_when_helper_changes_then_no_static_path()
-> Result<(), String> {
    let tests = "from other.models import Cache\n\n\ndef test_setitem():\n    cache = Cache(maxsize=2)\n    cache[1] = 1\n    assert cache is not None\n";

    let finding = finding_at(CACHE_PY, tests, "return 1")?;

    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert_eq!(finding.static_limit_kind, None, "{finding:?}");
    Ok(())
}

#[test]
fn given_class_only_constructed_when_helper_changes_then_no_static_path() -> Result<(), String> {
    let tests = "from cachetools import Cache\n\n\ndef test_construct():\n    cache = Cache(maxsize=2)\n    assert cache is not None\n";

    let finding = finding_at(CACHE_PY, tests, "return 1")?;

    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    // The existing same-class limitation names it; no reach is claimed.
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::PythonTransitiveReachUnresolved),
        "{finding:?}"
    );
    assert!(finding.related_tests.is_empty(), "{finding:?}");
    Ok(())
}

#[test]
fn given_dunder_of_another_class_when_helper_changes_then_no_static_path() -> Result<(), String> {
    // `Beta.__getitem__` calls `self.__touch`, but `Alpha.__touch` is Alpha's own
    // name-mangled helper: no dunder of Alpha reaches it.
    let source = "class Alpha:\n    def __touch(self, key):\n        return 1\n\n\nclass Beta:\n    def __getitem__(self, key):\n        return self.__touch(key)\n\n    def __touch(self, key):\n        return 2\n";
    let mut owners = extract_owners(Path::new("src/cachetools/__init__.py"), source);
    apply_class_facts(&mut owners);
    let entries_of = |qualified: &str| {
        owners
            .iter()
            .find(|owner| owner.qualified_name == qualified)
            .map(|owner| owner.class_facts.dunder_entries.clone())
    };
    assert_eq!(entries_of("Alpha.__touch"), Some(Vec::new()));
    assert_eq!(
        entries_of("Beta.__touch"),
        Some(vec!["__getitem__".to_string()])
    );
    let tests = "from cachetools import Alpha, Beta\n\n\ndef test_getitem():\n    b = Beta()\n    b[1]\n    assert Alpha is not None\n";

    let finding = finding_at(source, tests, "return 1")?;

    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert_eq!(finding.static_limit_kind, None, "{finding:?}");
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
fn given_dict_subscript_without_owner_import_when_helper_changes_then_no_static_path()
-> Result<(), String> {
    let tests = "def test_dict():\n    cache = {}\n    cache[1] = 1\n    assert cache[1] == 1\n";

    let finding = finding_at(CACHE_PY, tests, "return 1")?;

    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert_eq!(finding.static_limit_kind, None);
    Ok(())
}

#[test]
fn given_owner_import_and_dict_subscript_when_helper_changes_then_static_unknown_by_design()
-> Result<(), String> {
    // The protocol syntax is not tied to an instance of the owner's class: a
    // dict subscript in a test that imports `Cache` withdraws the confident
    // `no_static_path` claim. That is accepted (RIPR-SPEC-0201): the limit
    // grants no relation, oracle or credit.
    let tests = "from cachetools import Cache\n\n\ndef test_dict():\n    assert Cache is not None\n    cache = {}\n    cache[1] = 1\n    assert cache[1] == 1\n";

    let finding = finding_at(CACHE_PY, tests, "return 1")?;

    assert_reached_without_credit(&finding);
    assert!(finding.related_tests.is_empty(), "{finding:?}");
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::DynamicDispatch),
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn given_two_reaching_dunders_when_test_uses_the_second_then_evidence_names_it()
-> Result<(), String> {
    let source = "class Cache:\n    def __getitem__(self, key):\n        return self._check(key)\n\n    def __setitem__(self, key, value):\n        self._check(key)\n\n    def _check(self, key):\n        return key + 7\n";
    let mut owners = extract_owners(Path::new("src/cachetools/__init__.py"), source);
    apply_class_facts(&mut owners);
    // `__getitem__` is the first reaching dunder, so naming the first entry
    // would name a protocol the test never used.
    assert_eq!(
        owners
            .iter()
            .find(|owner| owner.qualified_name == "Cache._check")
            .map(|owner| owner.class_facts.dunder_entries.clone()),
        Some(vec!["__getitem__".to_string(), "__setitem__".to_string()])
    );
    let tests = "from cachetools import Cache\n\n\ndef test_setitem():\n    cache = Cache()\n    cache[1] = 1\n    assert cache is not None\n";

    let finding = finding_at(source, tests, "return key + 7")?;

    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::DynamicDispatch),
        "{finding:?}"
    );
    let text = format!("{:?}", finding.evidence);
    assert!(text.contains("dunder method `__setitem__`"), "{text}");
    assert!(!text.contains("dunder method `__getitem__`"), "{text}");
    Ok(())
}

const INIT_HELPER_PY: &str = "class Cache:
    def __init__(self, m):
        self._check(m)

    def _check(self, m):
        if m < 0:
            raise ValueError
";

#[test]
fn given_helper_called_only_from_init_when_class_constructed_then_reading_unchanged()
-> Result<(), String> {
    // Constructor dunders start no dunder search: `_check` has no reaching
    // dunder, so construction alone neither relates it through a
    // `helper_owner_call` row nor withdraws the same-class limitation.
    let mut owners = extract_owners(Path::new("src/cachetools/__init__.py"), INIT_HELPER_PY);
    apply_class_facts(&mut owners);
    let check = owners
        .iter()
        .find(|owner| owner.qualified_name == "Cache._check")
        .ok_or("fixture must extract Cache._check")?;
    assert!(check.class_facts.dunder_entries.is_empty(), "{check:?}");
    let tests = "from cachetools import Cache\n\n\ndef test_construct():\n    c = Cache(2)\n    assert c is not None\n";

    let finding = finding_at(INIT_HELPER_PY, tests, "if m < 0:")?;

    assert_ne!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.starts_with("helper_owner_call:")),
        "{:?}",
        finding.evidence
    );
    assert_ne!(
        finding.static_limit_kind,
        Some(StaticLimitKind::DynamicDispatch),
        "{finding:?}"
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    Ok(())
}

#[test]
fn given_call_chain_from_dunder_when_depth_exceeds_bound_then_helper_has_no_entry() {
    let source = "class Deep:\n    def __getitem__(self, key):\n        return self.h1(key)\n\n    def h1(self, key):\n        return self.h2(key)\n\n    def h2(self, key):\n        return self.h3(key)\n\n    def h3(self, key):\n        return self.h4(key)\n\n    def h4(self, key):\n        return self.h5(key)\n\n    def h5(self, key):\n        return self.h6(key)\n\n    def h6(self, key):\n        return key\n";
    let mut owners = extract_owners(Path::new("src/cachetools/__init__.py"), source);
    apply_class_facts(&mut owners);
    let entries_of = |qualified: &str| {
        owners
            .iter()
            .find(|owner| owner.qualified_name == qualified)
            .map(|owner| owner.class_facts.dunder_entries.clone())
    };
    let reached = Some(vec!["__getitem__".to_string()]);
    assert_eq!(entries_of("Deep.h1"), reached);
    assert_eq!(entries_of("Deep.h5"), reached, "five hops are followed");
    assert_eq!(
        entries_of("Deep.h6"),
        Some(Vec::new()),
        "a sixth hop is past the bound"
    );
}
