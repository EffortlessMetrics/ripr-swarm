//! Tests for the `cross_module_caller_call` relation (#12955): a test in
//! another package that reaches a changed top-level helper only through a
//! function of another module that imports and calls it reads
//! `static_unknown` with the caller named, never `no_static_path` and never a
//! graded gap. Bindings this adapter cannot read exactly make no edge.

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const CORE_PY: &str =
    "def discount(price):\n    if price > 100:\n        return price - 10\n    return price\n";
const WRAPPER_PY: &str =
    "from wscore.core import discount\n\n\ndef checkout(price):\n    return discount(price)\n";
const CHECKOUT_TEST_PY: &str = "from app.app import checkout\n\n\ndef test_checkout_at_boundary():\n    assert checkout(100) == 90\n";

fn finding_with(sources: &[(&str, &str)], tests_source: &str) -> Result<Finding, String> {
    let mut owners = extract_owners(Path::new("wscore/core.py"), CORE_PY);
    for (file, source) in sources {
        owners.extend(extract_owners(Path::new(file), source));
    }
    let tests = extract_tests(Path::new("app/tests/test_t.py"), tests_source);
    assert!(
        owners
            .iter()
            .any(|owner| owner.file == Path::new("wscore/core.py") && owner.name == "discount"),
        "fixture must parse the changed owner"
    );
    assert!(!tests.is_empty(), "fixture must parse the test");
    classify_change(
        Path::new("wscore/core.py"),
        2,
        "    if price >= 100:",
        &owners,
        &tests,
    )
    .ok_or_else(|| "expected a finding for the changed helper line".to_string())
}

fn assert_cross_module_unknown(finding: &Finding, caller: &str) {
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::MissingImportGraph),
        "{finding:?}"
    );
    assert_eq!(finding.related_tests.len(), 1, "{finding:?}");
    let row = &finding.related_tests[0];
    assert_eq!(row.name, "test_checkout_at_boundary");
    assert_eq!(row.oracle, None, "cross-module reach carries no oracle");
    for expected in [
        "related_test_relation: cross_module_caller_call (test_checkout_at_boundary)".to_string(),
        "related_test_uncertain: cross_module_caller_call (test_checkout_at_boundary)".to_string(),
        format!("cross_module_caller_call: {caller} calls discount (test_checkout_at_boundary)"),
    ] {
        assert!(
            finding.evidence.contains(&expected),
            "missing `{expected}` in {:?}",
            finding.evidence
        );
    }
    assert!(
        finding.missing.iter().any(|line| line
            .contains("related tests reach `discount` only through functions of other modules")
            && line.contains(caller)),
        "the missing line must name the cross-module caller: {:?}",
        finding.missing
    );
}

#[test]
fn given_a_wrapper_in_another_package_when_its_test_pins_it_then_the_helper_reads_static_unknown()
-> Result<(), String> {
    let finding = finding_with(&[("app/app.py", WRAPPER_PY)], CHECKOUT_TEST_PY)?;
    assert_cross_module_unknown(&finding, "checkout (app/app.py)");
    Ok(())
}

#[test]
fn given_an_aliased_import_then_the_alias_call_is_the_edge() -> Result<(), String> {
    let wrapper = "from wscore.core import discount as cut\n\n\ndef checkout(price):\n    return cut(price)\n";
    let finding = finding_with(&[("app/app.py", wrapper)], CHECKOUT_TEST_PY)?;
    assert_cross_module_unknown(&finding, "checkout (app/app.py)");
    Ok(())
}

#[test]
fn given_a_same_module_hop_behind_the_importer_then_the_chain_is_followed() -> Result<(), String> {
    let wrapper = "from wscore.core import discount\n\n\ndef _apply(price):\n    return discount(price)\n\n\ndef checkout(price):\n    return _apply(price)\n";
    let finding = finding_with(&[("app/app.py", wrapper)], CHECKOUT_TEST_PY)?;
    assert_cross_module_unknown(&finding, "checkout (app/app.py)");
    Ok(())
}

#[test]
fn given_no_test_calls_the_wrapper_then_it_stays_no_static_path() -> Result<(), String> {
    let unrelated = "def test_nothing():\n    assert 1 == 1\n";
    let finding = finding_with(&[("app/app.py", WRAPPER_PY)], unrelated)?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(finding.related_tests.is_empty());
    Ok(())
}

#[test]
fn given_a_binding_that_is_not_the_owner_then_no_edge_is_made() -> Result<(), String> {
    let cases = [
        // The same name from another module.
        (
            "from other.core import discount\n\n\ndef checkout(price):\n    return discount(price)\n",
            vec![("other/core.py", "def discount(price):\n    return price\n")],
        ),
        // Only another name of the owner's module is imported; `discount` is
        // a local function of the wrapper's own file.
        (
            "from wscore.core import other\n\n\ndef discount(price):\n    return price\n\n\ndef checkout(price):\n    return discount(price)\n",
            vec![],
        ),
        // The import is shadowed by a top-level def of the same name.
        (
            "from wscore.core import discount\n\n\ndef discount(price):\n    return price\n\n\ndef checkout(price):\n    return discount(price)\n",
            vec![],
        ),
        // A namespace import read as an attribute call is not followed.
        (
            "from wscore import core\n\n\ndef checkout(price):\n    return core.discount(price)\n",
            vec![],
        ),
        // The owner is passed as a value, not called.
        (
            "from wscore.core import discount\n\n\ndef checkout(price):\n    return price\n\n\nHANDLER = discount\n",
            vec![],
        ),
        // A parameter shadows the imported name inside the caller.
        (
            "from wscore.core import discount\n\n\ndef checkout(price, discount):\n    return discount(price)\n",
            vec![],
        ),
    ];
    for (wrapper, extra) in cases {
        let mut sources = vec![("app/app.py", wrapper)];
        sources.extend(extra);
        let finding = finding_with(&sources, CHECKOUT_TEST_PY)?;
        assert_eq!(
            finding.class,
            ExposureClass::NoStaticPath,
            "wrapper must make no edge:\n{wrapper}\n{finding:?}"
        );
        assert!(
            !finding
                .evidence
                .iter()
                .any(|line| line.contains("cross_module_caller_call")),
            "{wrapper}"
        );
    }
    Ok(())
}

#[test]
fn given_a_same_module_caller_and_a_cross_module_caller_then_the_same_module_reading_holds()
-> Result<(), String> {
    let core = "def discount(price):\n    if price > 100:\n        return price - 10\n    return price\n\n\ndef total(price):\n    return discount(price)\n";
    let mut owners = extract_owners(Path::new("wscore/core.py"), core);
    owners.extend(extract_owners(Path::new("app/app.py"), WRAPPER_PY));
    let tests_source = "from app.app import checkout\nfrom wscore.core import total\n\n\ndef test_checkout_at_boundary():\n    assert checkout(100) == 90\n\n\ndef test_total():\n    assert total(100) == 90\n";
    let tests = extract_tests(Path::new("app/tests/test_t.py"), tests_source);
    let owner = owners
        .iter()
        .find(|owner| owner.name == "discount")
        .ok_or("fixture must parse discount")?;
    let relations: Vec<(String, String, bool)> =
        related_tests::helper_owner_call_candidates(owner, &owners, &tests)
            .iter()
            .map(|helper| {
                (
                    helper.candidate.test.name.clone(),
                    helper.candidate.relation.as_str().to_string(),
                    helper.cross_module,
                )
            })
            .collect();
    assert_eq!(
        relations,
        vec![
            (
                "test_checkout_at_boundary".to_string(),
                "cross_module_caller_call".to_string(),
                true
            ),
            (
                "test_total".to_string(),
                "helper_owner_call".to_string(),
                false
            ),
        ]
    );
    let finding = classify_change(
        Path::new("wscore/core.py"),
        2,
        "    if price >= 100:",
        &owners,
        &tests,
    )
    .ok_or("expected a finding")?;
    assert_ne!(
        finding.static_limit_kind,
        Some(StaticLimitKind::MissingImportGraph),
        "a same-module caller keeps the existing helper reading: {finding:?}"
    );
    assert_ne!(finding.class, ExposureClass::NoStaticPath);
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn given_more_cross_module_callers_than_the_cap_then_the_owner_reads_static_unknown()
-> Result<(), String> {
    let count = related_tests::MAX_CROSS_MODULE_CALLERS + 1;
    let files: Vec<String> = (0..count)
        .map(|index| format!("app{index}/app.py"))
        .collect();
    let sources: Vec<(&str, &str)> = files
        .iter()
        .map(|file| (file.as_str(), WRAPPER_PY))
        .collect();
    let unrelated = "def test_nothing():\n    assert 1 == 1\n";
    let finding = finding_with(&sources, unrelated)?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
    assert!(
        finding.missing.iter().any(|line| line.contains(&format!(
            "more than {} functions of other modules",
            related_tests::MAX_CROSS_MODULE_CALLERS
        ))),
        "{:?}",
        finding.missing
    );

    // At the cap the callers are followed, and with no test reaching any of
    // them the owner keeps its `no_static_path` reading.
    let at_cap: Vec<(&str, &str)> = sources
        .iter()
        .take(related_tests::MAX_CROSS_MODULE_CALLERS)
        .copied()
        .collect();
    let finding = finding_with(&at_cap, unrelated)?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    Ok(())
}

#[test]
fn given_a_deeper_same_module_chain_and_a_cross_module_caller_then_the_same_module_reading_holds()
-> Result<(), String> {
    // Review of #12955: the same-module caller sits two hops from the owner,
    // the cross-module caller one hop; the same-module caller still wins.
    let core = "def discount(price):\n    if price > 100:\n        return price - 10\n    return price\n\n\ndef mid(price):\n    return discount(price)\n\n\ndef outer(price):\n    return mid(price)\n";
    let mut owners = extract_owners(Path::new("wscore/core.py"), core);
    owners.extend(extract_owners(Path::new("app/app.py"), WRAPPER_PY));
    let tests_source = "from app.app import checkout\nfrom wscore.core import outer\n\n\ndef test_both():\n    assert outer(100) == 90\n    assert checkout(100) == 90\n";
    let tests = extract_tests(Path::new("app/tests/test_t.py"), tests_source);
    let owner = owners
        .iter()
        .find(|owner| owner.name == "discount")
        .ok_or("fixture must parse discount")?;
    let relations: Vec<(String, String, String)> =
        related_tests::helper_owner_call_candidates(owner, &owners, &tests)
            .iter()
            .map(|helper| {
                (
                    helper.candidate.test.name.clone(),
                    helper.candidate.relation.as_str().to_string(),
                    helper.caller.clone(),
                )
            })
            .collect();
    assert_eq!(
        relations,
        vec![(
            "test_both".to_string(),
            "helper_owner_call".to_string(),
            "outer".to_string()
        )]
    );
    Ok(())
}

#[test]
fn given_an_ambiguous_local_binding_then_no_edge_is_made() -> Result<(), String> {
    for wrapper in [
        // Two imports bind the owner's name.
        "from wscore.core import discount\nfrom wscore.core import discount as cut\n\n\ndef checkout(price):\n    return discount(price) + cut(price)\n",
        // The alias is also bound by another import.
        "from wscore.core import discount as cut\nfrom other.core import cut\n\n\ndef checkout(price):\n    return cut(price)\n",
    ] {
        let finding = finding_with(&[("app/app.py", wrapper)], CHECKOUT_TEST_PY)?;
        assert_eq!(
            finding.class,
            ExposureClass::NoStaticPath,
            "wrapper must make no edge:\n{wrapper}\n{finding:?}"
        );
    }
    Ok(())
}

#[test]
fn given_a_method_caller_in_another_module_then_a_same_named_bare_call_makes_no_edge()
-> Result<(), String> {
    // Devin review of #13890: `Checkout.checkout` imports and calls the owner,
    // and the test calls an unrelated `checkout` from another module without
    // ever building a `Checkout`. A method in another module is never a
    // cross-module entry, so no test is linked and the owner keeps its reading.
    let wrapper = "from wscore.core import discount\n\n\nclass Checkout:\n    def checkout(self, price):\n        return discount(price)\n";
    let unrelated = "from other.lib import checkout\n\n\ndef test_checkout_at_boundary():\n    assert checkout(100) == 90\n";
    let finding = finding_with(
        &[
            ("app/app.py", wrapper),
            (
                "other/lib.py",
                "def checkout(price):\n    return price - 10\n",
            ),
        ],
        unrelated,
    )?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(finding.related_tests.is_empty(), "{finding:?}");
    Ok(())
}

#[test]
fn given_a_method_behind_a_cross_module_hop_then_no_edge_is_made() -> Result<(), String> {
    // Exact-head review of #13890: past the module boundary a method is not a
    // caller at a later hop either, whether the test calls its name bare or on
    // an unrelated receiver.
    let tests = [
        "from app.app import Checkout\n\n\ndef test_cart():\n    cart = object()\n    assert cart.checkout(100) == 90\n",
        "def test_cart():\n    assert checkout(100) == 90\n",
    ];
    let wrappers = [
        "from wscore.core import discount\n\n\ndef _apply(price):\n    return discount(price)\n\n\nclass Checkout:\n    def checkout(self, price):\n        return _apply(price)\n",
        "from wscore.core import discount\n\n\nclass Checkout:\n    def checkout(self, price):\n        return discount(price)\n",
    ];
    for wrapper in wrappers {
        for test in tests {
            let finding = finding_with(&[("app/app.py", wrapper)], test)?;
            assert_eq!(
                finding.class,
                ExposureClass::NoStaticPath,
                "wrapper:\n{wrapper}\ntest:\n{test}\n{finding:?}"
            );
            assert!(finding.related_tests.is_empty(), "{finding:?}");
        }
    }
    Ok(())
}
