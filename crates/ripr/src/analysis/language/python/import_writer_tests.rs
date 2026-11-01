//! `from unittest.mock import patch` names a writer without calling it, so it
//! does not unsettle the `pytest` import root that parametrize argnames read
//! (#11102, pallets/click's `test_termui.py`). A call or an alias still does.

use super::fixtures_tests::workspace;
use super::*;
use std::path::Path;

const SHOP: &str = "def total(amount):\n    return amount * 2\n";

const PARAMETRIZED: &str = "import pytest\n\nfrom shop import total\n\n\n@pytest.mark.parametrize((\"amount\", \"expected\"), [(3, 6), (4, 8)])\ndef test_total(amount, expected):\n    assert total(amount) == expected\n";

fn classify_total(prefix: &str) -> Result<Finding, String> {
    let (owners, tests) = workspace(&[
        ("shop/__init__.py", SHOP),
        ("tests/test_shop.py", &format!("{prefix}{PARAMETRIZED}")),
    ]);
    classify_change(
        Path::new("shop/__init__.py"),
        2,
        "    return amount * 3",
        &owners,
        &tests,
    )
    .ok_or_else(|| "the changed line must classify".to_string())
}

#[test]
fn an_imported_patch_name_does_not_unsettle_the_argnames() -> Result<(), String> {
    let plain = classify_total("")?;
    assert_eq!(plain.static_limit_kind, None, "{:?}", plain.evidence);
    let imported = classify_total("from unittest.mock import patch\n")?;
    assert_eq!(imported.static_limit_kind, None, "{:?}", imported.evidence);
    assert_eq!(imported.class, plain.class);
    Ok(())
}

#[test]
fn a_call_or_an_alias_of_the_writer_keeps_the_limit() -> Result<(), String> {
    // An aliased call is not a recognised substitution form, so it only
    // unsettles the argnames. A module-scope `patch(..)` or `setattr(..)` is a
    // substitution form in top-level test-module code, so the stronger
    // `mocked_module` limit (`fixture_substitution.rs`) may speak first;
    // either way the finding keeps a static limit and earns no credit.
    for (case, prefix) in [
        (
            "alias",
            "from unittest.mock import patch as p\n\np(\"pytest.mark\", None)\n",
        ),
        (
            "module-scope call",
            "from unittest.mock import patch\n\npatch(\"pytest.mark\", None)\n",
        ),
        (
            "module-scope setattr",
            "import os\n\nsetattr(os, \"x\", 1)\n",
        ),
    ] {
        let finding = classify_total(prefix)?;
        assert!(
            matches!(
                finding.static_limit_kind,
                Some(StaticLimitKind::UnresolvedPytestFixture | StaticLimitKind::MockedModule)
            ),
            "{case}: {:?} {:?}",
            finding.static_limit_kind,
            finding.evidence
        );
    }
    Ok(())
}
