//! A helper that only a dunder of its own class calls through `self.` may be
//! reached by a test that references the class through an import and uses
//! that dunder's protocol syntax somewhere in its body (#13776). cachetools:
//! `cache[key] = value` runs `Cache.__setitem__`, which calls the static
//! `self.getsizeof(value)`; `cache[key]` runs `LFUCache.__getitem__`, which
//! calls the name-mangled `self.__touch(key)`. A module-level helper called by
//! such a dunder may also be reached through an asserted operator on a binding
//! from the owner class or a validated same-package factory. Reach here is
//! weak: it never earns an oracle or `exposed`.

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
    finding_at_path(
        Path::new("src/cachetools/__init__.py"),
        "tests/test_cache.py",
        source,
        tests_source,
        needle,
    )
}

fn finding_at_path(
    owner_path: &Path,
    test_path: &str,
    source: &str,
    tests_source: &str,
    needle: &str,
) -> Result<Finding, String> {
    let mut owners = extract_owners(owner_path, source);
    apply_class_facts(&mut owners);
    let tests = extract_tests(Path::new(test_path), tests_source);
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
    classify_change(owner_path, index + 1, line, &owners, &tests)
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

fn factory_helper_link_count(
    factory_sources: &[(&str, &str)],
    tests_source: &str,
) -> Result<usize, String> {
    let source = ISODATE_HELPER_PY
        .split("\n\ndef parse_duration")
        .next()
        .ok_or("fixture must contain the class")?;
    let mut owners = extract_owners(Path::new("src/isodate/duration.py"), source);
    for (path, source) in factory_sources {
        owners.extend(extract_owners(Path::new(path), source));
    }
    apply_class_facts(&mut owners);
    let tests = extract_tests(Path::new("tests/test_duration.py"), tests_source);
    assert_eq!(tests.len(), 1, "fixture must collect exactly one test");
    let helper = owners
        .iter()
        .find(|owner| owner.qualified_name == "fquotmod")
        .ok_or("fixture must contain fquotmod")?;
    Ok(related_tests::helper_owner_call_candidates(helper, &owners, &tests).len())
}

const IMPORTED_FACTORY: &str = "from isodate.duration import Duration\n\ndef parse_duration(value):\n    return Duration(value)\n";

#[test]
fn given_cross_module_factory_when_dunder_reaches_helper_then_weak_link_exists()
-> Result<(), String> {
    let source = ISODATE_HELPER_PY.replace(
        "        return fquotmod(newmonth, 1, 13)\n",
        "        return fquotmod(newmonth, 1, 13)\n\n    __radd__ = __add__\n",
    );
    let source = source
        .split("\n\ndef parse_duration")
        .next()
        .ok_or("fixture must contain the class")?;
    let mut owners = extract_owners(Path::new("src/isodate/duration.py"), source);
    owners.extend(extract_owners(
        Path::new("src/isodate/isoduration.py"),
        "from isodate.duration import Duration\n\ndef parse_duration(value):\n    return Duration(1)\n",
    ));
    apply_class_facts(&mut owners);
    let test = extract_tests(
        Path::new("tests/test_duration.py"),
        "from isodate import Duration\n\n@pytest.mark.parametrize(\"dur1, dur2, resadd\", [(\"P1D\",\"P2D\",\"P3D\")])\ndef test_add(dur1, dur2, resadd):\n    duration1 = parse_duration(dur1)\n    duration2 = parse_duration(dur2)\n    result_add = parse_duration(resadd)\n    assert duration1 + duration2 == result_add\n",
    );
    let helper = owners
        .iter()
        .find(|owner| owner.qualified_name == "fquotmod")
        .ok_or("fixture must contain fquotmod")?;
    let dunder = owners
        .iter()
        .find(|owner| owner.qualified_name == "Duration.__add__")
        .ok_or("fixture must contain Duration.__add__")?;
    assert!(
        !dunder.class_facts.plain,
        "the __radd__ alias makes this class non-plain"
    );
    let factory = owners
        .iter()
        .find(|owner| owner.qualified_name == "parse_duration")
        .ok_or("fixture must contain parse_duration")?;
    let found = related_tests::helper_owner_call_candidates(helper, &owners, &test);
    assert_eq!(found.len(), 1, "expected one weak helper link");
    assert_eq!(
        found[0].candidate.relation,
        PythonRelationKind::HelperOwnerCall
    );
    assert_eq!(found[0].caller, "Duration.__add__");
    assert!(related_tests::oracle_operand_protocol_entry_with_factories(
        &test[0].assertions[0].gate_text,
        &test[0],
        dunder,
        std::slice::from_ref(&factory.name),
    ));
    Ok(())
}

#[test]
fn given_foreign_or_shadowed_factory_when_dunder_reaches_helper_then_no_link_exists()
-> Result<(), String> {
    let cases = [
        (
            "foreign import",
            "from isodate import Duration\nfrom other.parsers import parse_duration\n\ndef test_add():\n    left = parse_duration(1)\n    right = parse_duration(2)\n    assert left + right == 3\n",
        ),
        (
            "test assignment",
            "from isodate import Duration\n\ndef test_add():\n    parse_duration = lambda value: value\n    left = parse_duration(1)\n    right = parse_duration(2)\n    assert left + right == 3\n",
        ),
        (
            "test parameter",
            "from isodate import Duration\n\ndef test_add(parse_duration):\n    left = parse_duration(1)\n    right = parse_duration(2)\n    assert left + right == 3\n",
        ),
    ];
    for (label, test) in cases {
        assert_eq!(
            factory_helper_link_count(&[("src/isodate/isoduration.py", IMPORTED_FACTORY)], test,)?,
            0,
            "{label} must not establish the owner's factory"
        );
    }
    Ok(())
}

#[test]
fn given_conflicting_package_factories_when_implicit_name_is_used_then_no_link_exists()
-> Result<(), String> {
    let test = "from isodate import Duration\n\ndef test_add():\n    left = parse_duration(1)\n    right = parse_duration(2)\n    assert left + right == 3\n";
    assert_eq!(
        factory_helper_link_count(
            &[
                ("src/isodate/isoduration.py", IMPORTED_FACTORY),
                ("src/isodate/other.py", IMPORTED_FACTORY),
            ],
            test,
        )?,
        0
    );
    Ok(())
}

#[test]
fn given_duplicate_factory_definition_when_dunder_reaches_helper_then_no_link_exists()
-> Result<(), String> {
    let factory = "from isodate.duration import Duration\n\ndef parse_duration(value):\n    return Duration(value)\n\ndef parse_duration(value):\n    return Duration(value)\n";
    let test = "from isodate import Duration\n\ndef test_add():\n    left = parse_duration(1)\n    right = parse_duration(2)\n    assert left + right == 3\n";
    assert_eq!(
        factory_helper_link_count(&[("src/isodate/isoduration.py", factory)], test)?,
        0
    );
    Ok(())
}

#[test]
fn given_valid_factory_import_spellings_when_dunder_reaches_helper_then_weak_links_exist()
-> Result<(), String> {
    let cases = [
        "from isodate import Duration\nfrom isodate.isoduration import parse_duration as make_duration\n\ndef test_add():\n    left = make_duration(1)\n    right = make_duration(2)\n    assert left + right == make_duration(3)\n",
        "from isodate import Duration\nimport isodate.isoduration as factory\n\ndef test_add():\n    left = factory.parse_duration(1)\n    right = factory.parse_duration(2)\n    assert left + right == factory.parse_duration(3)\n",
    ];
    for test in cases {
        assert_eq!(
            factory_helper_link_count(&[("src/isodate/isoduration.py", IMPORTED_FACTORY)], test)?,
            1,
            "a proven import identity should retain weak reach: {test}"
        );
    }
    Ok(())
}

#[test]
fn given_factory_without_direct_class_return_when_dunder_reaches_helper_then_no_link_exists()
-> Result<(), String> {
    let returns = [
        "return 1",
        "return Duration(value).days",
        "return [Duration(value)]",
        "return (Duration(value),)",
        "Duration(value)\n    return None",
        "return Duration(value)\n    return 1",
    ];
    for returned in returns {
        let factory = format!(
            "from isodate.duration import Duration\n\ndef parse_duration(value):\n    {returned}\n"
        );
        let test = "from isodate import Duration\nfrom isodate.isoduration import parse_duration\n\ndef test_add():\n    left = parse_duration(1)\n    right = parse_duration(2)\n    assert left + right == 3\n";
        assert_eq!(
            factory_helper_link_count(&[("src/isodate/isoduration.py", &factory)], test,)?,
            0,
            "return shape must fail closed: {returned}"
        );
    }
    Ok(())
}

const ISODATE_HELPER_PY: &str = "def fquotmod(val, low, high):
    a, b = val - low, high - low
    div = (a / b).to_integral(ROUND_FLOOR)
    mod = a - div * b
    mod += low
    return int(div), mod


class Duration:
    def __init__(self, days=0):
        self.days = days

    def __add__(self, other):
        newmonth = self.days + other.days
        return fquotmod(newmonth, 1, 13)


def parse_duration(value):
    return Duration(1)
";

#[test]
fn given_operator_reaches_module_helper_from_dunder_then_weakly_reached() -> Result<(), String> {
    // isodate's shape: test_add imports Duration, constructs through the
    // module's parser, then invokes __add__ with `duration1 + duration2`.
    let tests = "from isodate import Duration\n\n@pytest.mark.parametrize(\"dur1, dur2, resadd\", [(\"P1D\", \"P2D\", \"P3D\")])\ndef test_add(dur1, dur2, resadd):\n    duration1 = parse_duration(dur1)\n    duration2 = parse_duration(dur2)\n    result_add = parse_duration(resadd)\n    assert duration1 + duration2 == result_add\n";

    let finding = finding_at_path(
        Path::new("src/isodate/duration.py"),
        "tests/test_duration.py",
        ISODATE_HELPER_PY,
        tests,
        "a, b = val - low, high - low",
    )?;

    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert_reached_without_credit(&finding);
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.starts_with("helper_owner_call: Duration.__add__ calls fquotmod")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn given_unrelated_integer_operator_when_module_helper_changes_then_no_static_path()
-> Result<(), String> {
    let tests = "from isodate import Duration\n\n\ndef test_ints():\n    assert 1 + 2 == 3\n";
    let finding = finding_at_path(
        Path::new("src/isodate/duration.py"),
        "tests/test_duration.py",
        ISODATE_HELPER_PY,
        tests,
        "a, b = val - low, high - low",
    )?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    Ok(())
}

#[test]
fn given_unary_minus_when_sub_helper_changes_then_no_static_path() -> Result<(), String> {
    let source = ISODATE_HELPER_PY.replacen("def __add__", "def __sub__", 1);
    let tests = "from isodate import Duration\n\n\ndef test_neg():\n    duration = parse_duration(\"P1D\")\n    assert -duration == duration\n";
    let finding = finding_at_path(
        Path::new("src/isodate/duration.py"),
        "tests/test_duration.py",
        &source,
        tests,
        "a, b = val - low, high - low",
    )?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    Ok(())
}

#[test]
fn given_explicit_ne_when_eq_helper_changes_then_no_static_path() -> Result<(), String> {
    let source = ISODATE_HELPER_PY
        .replacen("def __add__", "def __eq__", 1)
        .replacen(
            "        self.days = days\n",
            "        self.days = days\n\n    def __ne__(self, other):\n        return True\n",
            1,
        );
    let tests = "from isodate import Duration\n\n\ndef test_ne():\n    left = parse_duration(\"P1D\")\n    right = parse_duration(\"P2D\")\n    assert left != right\n";
    let finding = finding_at_path(
        Path::new("src/isodate/duration.py"),
        "tests/test_duration.py",
        &source,
        tests,
        "a, b = val - low, high - low",
    )?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    Ok(())
}

#[test]
fn given_no_dunder_operator_when_module_helper_changes_then_no_static_path() -> Result<(), String> {
    let tests = "from isodate import Duration\n\n\ndef test_ctor():\n    duration1 = parse_duration(\"P1D\")\n    assert duration1.days == 1\n";
    let finding = finding_at_path(
        Path::new("src/isodate/duration.py"),
        "tests/test_duration.py",
        ISODATE_HELPER_PY,
        tests,
        "a, b = val - low, high - low",
    )?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    Ok(())
}

#[test]
fn given_foreign_class_operator_when_module_helper_changes_then_no_static_path()
-> Result<(), String> {
    let tests = "from other.models import Duration\n\n\ndef test_add():\n    duration1 = Duration(1)\n    duration2 = Duration(2)\n    assert duration1 + duration2 == Duration(3)\n";
    let finding = finding_at_path(
        Path::new("src/isodate/duration.py"),
        "tests/test_duration.py",
        ISODATE_HELPER_PY,
        tests,
        "a, b = val - low, high - low",
    )?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
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

/// The repo-mode reading for `needle` in `src/cachetools/__init__.py`.
fn repo_finding_at(
    label: &str,
    source: &str,
    tests_source: &str,
    needle: &str,
) -> Result<Finding, String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-py-dunder-repo-{label}-{}",
        std::process::id()
    ));
    let write = |relative: &str, contents: &str| -> Result<(), String> {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| format!("create parent: {err}"))?;
        }
        std::fs::write(&path, contents).map_err(|err| format!("write {relative}: {err}"))
    };
    write("src/cachetools/__init__.py", source)?;
    write("tests/test_cache.py", tests_source)?;
    let input = super::repo::select_repo_input_with_limit(
        &root,
        super::repo::RepoWorkingSetLimit {
            limit: 800,
            source: super::repo::RepoWorkingSetCapSource::Default,
        },
    );
    let evidence = super::repo::build_repo_evidence(&input, &root);
    let _ = std::fs::remove_dir_all(&root);
    let line = source
        .lines()
        .position(|line| line.trim() == needle)
        .map(|index| index + 1)
        .ok_or_else(|| format!("fixture must contain `{needle}`"))?;
    evidence
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("expected a repo finding for `{needle}`"))
}

#[test]
fn given_dunder_reached_helper_when_repo_mode_then_reading_matches_diff_mode() -> Result<(), String>
{
    let tests = "from cachetools import Cache\n\n\ndef test_setitem():\n    cache = Cache(maxsize=2)\n    cache[1] = 1\n    assert cache is not None\n";

    let diff = finding_at(CACHE_PY, tests, "return 1")?;
    let repo = repo_finding_at("reach", CACHE_PY, tests, "return 1")?;

    assert_eq!(diff.class, ExposureClass::StaticUnknown, "{diff:?}");
    assert_eq!(repo.class, diff.class, "{repo:?}");
    assert_eq!(repo.static_limit_kind, diff.static_limit_kind, "{repo:?}");
    assert_eq!(
        repo.static_limit_kind,
        Some(StaticLimitKind::DynamicDispatch)
    );
    assert_reached_without_credit(&repo);
    Ok(())
}

#[test]
fn given_no_dunder_use_when_repo_mode_then_helper_stays_no_static_path() -> Result<(), String> {
    // Negative control: the same class, but the test never uses the dunder's
    // protocol syntax, so repo mode must not invent a reach.
    let tests = "from cachetools import Cache\n\n\ndef test_ctor():\n    cache = Cache(maxsize=2)\n    assert cache is not None\n";

    let diff = finding_at(CACHE_PY, tests, "return 1")?;
    let repo = repo_finding_at("control", CACHE_PY, tests, "return 1")?;

    assert_eq!(diff.class, ExposureClass::NoStaticPath, "{diff:?}");
    assert_eq!(repo.class, diff.class, "{repo:?}");
    Ok(())
}
