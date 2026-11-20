//! #12922: the Python adapter's classifier emits only `exposed`,
//! `weakly_exposed`, `no_static_path` and `static_unknown`. It never assigns
//! `reachable_unrevealed`, `infection_unknown` or `propagation_unknown`. An
//! input built to leave a stage unknown (a test that rebinds the changed
//! module object before asserting) must read `static_unknown`, never
//! `exposed`, and never one of the unemitted classes. If the adapter later
//! starts emitting one, the sweep fails and this waiver must become a Python
//! verdict corpus `[[axis]]` with a floor.

use super::classify::classify_change_with_old;
use super::owners_tests::{extract_owners, extract_tests};
use super::related_tests::apply_class_facts;
use crate::domain::{ExposureClass, Finding};
use std::path::Path;

type Check = Result<(), String>;

const OWNER: &str = "from types import SimpleNamespace\n\nresponse = SimpleNamespace(status_code=200)\n\n\ndef checkout(expired):\n    response.status_code = 400\n";
const HEAD: &str = "from shop.route import checkout, response\n";

fn finding_for(label: &str, test_src: &str) -> Result<Finding, String> {
    let file = "shop/route.py";
    let mut owners = extract_owners(Path::new(file), OWNER);
    apply_class_facts(&mut owners);
    let owner = owners
        .iter()
        .find(|owner| owner.name == "checkout")
        .ok_or("owner checkout must be extracted")?;
    let tests = extract_tests(Path::new("tests/test_route.py"), test_src);
    if tests.is_empty() {
        return Err(format!("{label}: fixture must parse a test"));
    }
    classify_change_with_old(
        Path::new(file),
        owner.start_line + 1,
        "    response.status_code = 400",
        Some("    response.status_code = 422"),
        &owners,
        &tests,
    )
    .ok_or_else(|| format!("{label}: the change must yield a finding"))
}

fn is_unemitted(class: &ExposureClass) -> bool {
    matches!(
        class,
        ExposureClass::ReachableUnrevealed
            | ExposureClass::InfectionUnknown
            | ExposureClass::PropagationUnknown
    )
}

#[test]
fn unknown_stage_inputs_read_static_unknown_never_exposed_or_an_unemitted_class() -> Check {
    for (label, body) in [
        (
            "setattr-rebind",
            "from shop import route\n\n\ndef test_it():\n    setattr(route, \"response\", object())\n    checkout(True)\n    assert response.status_code == 422\n",
        ),
        (
            "reload-rebind",
            "import importlib\nfrom shop import route\n\n\ndef test_it():\n    importlib.reload(route)\n    checkout(True)\n    assert response.status_code == 422\n",
        ),
    ] {
        let finding = finding_for(label, &format!("{HEAD}{body}"))?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{label}");
        assert!(!is_unemitted(&finding.class), "{label}");
    }
    Ok(())
}

#[test]
fn credited_unrelated_and_weak_inputs_never_reach_an_unemitted_class() -> Check {
    let live = format!(
        "{HEAD}\n\ndef test_it():\n    checkout(True)\n    assert response.status_code == 422\n"
    );
    let finding = finding_for("live-control", &live)?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert!(!is_unemitted(&finding.class));

    let no_assert = format!("{HEAD}\n\ndef test_it():\n    checkout(True)\n");
    let finding = finding_for("no-assertion", &no_assert)?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert!(!is_unemitted(&finding.class), "{:?}", finding.class);

    let unrelated = "def test_other():\n    assert 1 + 1 == 2\n";
    let finding = finding_for("unrelated", unrelated)?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert!(!is_unemitted(&finding.class), "{:?}", finding.class);
    Ok(())
}
