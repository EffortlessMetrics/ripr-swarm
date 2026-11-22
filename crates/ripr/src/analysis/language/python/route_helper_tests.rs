//! Tests for a helper reached only through a requested route (#14150): a
//! test that requests a route through a client identified as the handler's
//! own application relates reach-only to a helper the handler directly
//! calls, so the helper no longer reads `no_static_path`. It never earns an
//! oracle or `exposed`: the response assertion is not shown to observe the
//! helper's changed behavior.

use super::*;
use std::path::Path;

const RULES: &str = "def is_adult(age):\n    return age >= 18\n";
const AGES: &str = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    return {\"adult\": is_adult(30)}\n";
const CONFTEST: &str = "import pytest\nfrom starlette.testclient import TestClient\nfrom routepkg.ages import app\n\n@pytest.fixture\ndef client():\n    return TestClient(app)\n";
const ROUTE_TEST: &str =
    "def test_age_route(client):\n    assert client.get(\"/age/30\").json() == {\"adult\": True}\n";

fn classify_files(files: &[(&str, &str)]) -> Result<Finding, String> {
    let (mut owners, tests) = super::fixtures_tests::workspace(files);
    // The production workspace pass withdraws route credit from overlapping
    // routes; the fixture helper does not run it, so do it here.
    let declared = files
        .iter()
        .map(|(_, source)| super::related_tests::route_decorator_lines(source))
        .sum();
    super::related_tests::apply_route_overlaps(&mut owners, false, declared);
    classify_change_with_old(
        Path::new("routepkg/rules.py"),
        2,
        "    return age >= 21",
        Some("    return age >= 18"),
        &owners,
        &tests,
    )
    .ok_or_else(|| "expected a finding for the changed helper line".to_string())
}

fn assert_reach_only(finding: &Finding, label: &str) {
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{label}: {finding:?}"
    );
    assert_eq!(finding.related_tests.len(), 1, "{label}: {finding:?}");
    assert_eq!(finding.related_tests[0].name, "test_age_route", "{label}");
    assert_eq!(finding.related_tests[0].oracle, None, "{label}: reach only");
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line == "related_test_relation: route_helper_call (test_age_route)"),
        "{label}: {:?}",
        finding.evidence
    );
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.starts_with("route_helper_call: age")),
        "{label}: {:?}",
        finding.evidence
    );
    assert!(
        !finding
            .missing
            .iter()
            .any(|line| line.contains("No Python test references")),
        "{label}: {:?}",
        finding.missing
    );
}

#[test]
fn given_a_helper_called_by_a_routed_handler_when_the_route_is_requested_then_reach_only()
-> Result<(), String> {
    // The issue's shape: helper in another module, client from a conftest
    // fixture over the handler module's own application.
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", AGES),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    assert_reach_only(&classify_files(&files)?, "conftest client");

    // The client built in the test body over the same application.
    let local_client = "from starlette.testclient import TestClient\nfrom routepkg.ages import app\n\ndef test_age_route():\n    client = TestClient(app)\n    assert client.get(\"/age/30\").json() == {\"adult\": True}\n";
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", AGES),
        ("tests/test_ages.py", local_client),
    ];
    assert_reach_only(&classify_files(&files)?, "local client");

    // An aliased import of the helper in the handler module.
    let aliased = "from routepkg.rules import is_adult as adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    return {\"adult\": adult(30)}\n";
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", aliased),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    assert_reach_only(&classify_files(&files)?, "aliased helper import");

    // An async handler is still called by the framework for its route.
    let async_handler = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\nasync def age():\n    return {\"adult\": is_adult(30)}\n";
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", async_handler),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    assert_reach_only(&classify_files(&files)?, "async handler");
    Ok(())
}

#[test]
fn given_a_helper_and_handler_in_one_module_when_the_route_is_requested_then_reach_only()
-> Result<(), String> {
    let same_module = "app = object()\n\ndef is_adult(age):\n    return age >= 18\n\n@app.get(\"/age/30\")\ndef age():\n    return {\"adult\": is_adult(30)}\n";
    let conftest = "import pytest\nfrom starlette.testclient import TestClient\nfrom routepkg.rules import app\n\n@pytest.fixture\ndef client():\n    return TestClient(app)\n";
    let (owners, tests) = super::fixtures_tests::workspace(&[
        ("routepkg/rules.py", same_module),
        ("tests/conftest.py", conftest),
        ("tests/test_ages.py", ROUTE_TEST),
    ]);
    let finding = classify_change_with_old(
        Path::new("routepkg/rules.py"),
        4,
        "    return age >= 21",
        Some("    return age >= 18"),
        &owners,
        &tests,
    )
    .ok_or_else(|| "expected a finding".to_string())?;
    assert_reach_only(&finding, "same module");
    Ok(())
}

#[test]
fn given_no_identified_route_request_then_the_helper_stays_no_static_path() -> Result<(), String> {
    // No test requests the route.
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", AGES),
        (
            "tests/test_ages.py",
            "def test_age_route(client):\n    assert True\n",
        ),
    ];
    let finding = classify_files(&files)?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(finding.related_tests.is_empty(), "{finding:?}");

    // A client over a rival application: the handler's own app is not shown.
    let rival_conftest = "import pytest\nfrom starlette.testclient import TestClient\nfrom routepkg.other import app\n\n@pytest.fixture\ndef client():\n    return TestClient(app)\n";
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", AGES),
        ("tests/conftest.py", rival_conftest),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    let finding = classify_files(&files)?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(finding.related_tests.is_empty(), "{finding:?}");

    // A client fixture the adapter cannot identify.
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", AGES),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    let finding = classify_files(&files)?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(finding.related_tests.is_empty(), "{finding:?}");

    // The wrong route is requested.
    let wrong_route = "def test_age_route(client):\n    assert client.get(\"/age/31\").json() == {\"adult\": True}\n";
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", AGES),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_ages.py", wrong_route),
    ];
    let finding = classify_files(&files)?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    Ok(())
}

#[test]
fn given_a_handler_that_does_not_directly_call_the_owner_then_no_route_reach() -> Result<(), String>
{
    // Two calls away: handler -> service -> helper keeps today's reading.
    let service =
        "from routepkg.rules import is_adult\n\ndef check(age):\n    return is_adult(age)\n";
    let handler = "from routepkg.service import check\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    return {\"adult\": check(30)}\n";
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/service.py", service),
        ("routepkg/ages.py", handler),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    let finding = classify_files(&files)?;
    assert_ne!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("route_helper_call")),
        "{:?}",
        finding.evidence
    );

    // The handler calls its own same-named function, not the owner's import.
    let shadowed = "app = object()\n\ndef is_adult(age):\n    return age > 99\n\n@app.get(\"/age/30\")\ndef age():\n    return {\"adult\": is_adult(30)}\n";
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", shadowed),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    let finding = classify_files(&files)?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(finding.related_tests.is_empty(), "{finding:?}");

    // A non-route decorator beside the route may wrap the handler, so its
    // callees are not recorded.
    let wrapped = "from routepkg.rules import is_adult\n\napp = object()\n\n@cache\n@app.get(\"/age/30\")\ndef age():\n    return {\"adult\": is_adult(30)}\n";
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", wrapped),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    let finding = classify_files(&files)?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(finding.related_tests.is_empty(), "{finding:?}");
    Ok(())
}

#[test]
fn given_a_handler_module_that_rebinds_the_imported_helper_then_no_route_reach()
-> Result<(), String> {
    // The handler module imports the helper, then rebinds the name at module
    // level: runtime calls the lambda, not the imported helper.
    for (label, handler) in [
        (
            "plain lambda rebind",
            "from routepkg.rules import is_adult\n\nis_adult = lambda age: age > 99\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    return {\"adult\": is_adult(30)}\n",
        ),
        (
            "aliased lambda rebind",
            "from routepkg.rules import is_adult as adult\n\nadult = lambda age: age > 99\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    return {\"adult\": adult(30)}\n",
        ),
        (
            "rebind after the handler",
            "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    return {\"adult\": is_adult(30)}\n\nis_adult = lambda age: age > 99\n",
        ),
    ] {
        let files = [
            ("routepkg/rules.py", RULES),
            ("routepkg/ages.py", handler),
            ("tests/conftest.py", CONFTEST),
            ("tests/test_ages.py", ROUTE_TEST),
        ];
        let finding = classify_files(&files)?;
        assert_eq!(
            finding.class,
            ExposureClass::NoStaticPath,
            "{label}: {finding:?}"
        );
        assert!(finding.related_tests.is_empty(), "{label}: {finding:?}");
        assert!(
            !finding
                .evidence
                .iter()
                .any(|line| line.contains("route_helper_call")),
            "{label}: {:?}",
            finding.evidence
        );
    }
    Ok(())
}

#[test]
fn given_overlapping_routes_on_one_app_then_the_shadowed_handler_earns_no_route_reach()
-> Result<(), String> {
    // The first route answers GET /age/30; only the second handler calls the
    // helper. Registration order is not read, so neither handler is
    // established as the one that runs.
    let handler = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/{age}\")\ndef age_param(age):\n    return {\"adult\": False}\n\n@app.get(\"/age/30\")\ndef age():\n    return {\"adult\": is_adult(30)}\n";
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", handler),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    let finding = classify_files(&files)?;
    // The generic cross-module caller relation may remain, but the route
    // relation must not: neither handler is established as the one that runs.
    assert_ne!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("route_helper_call")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn given_the_owner_module_rebinds_the_helper_after_its_definition_then_no_route_reach()
-> Result<(), String> {
    // The helper's own module replaces the name after the definition, so a
    // route that calls the name runs the replacement, not the changed def.
    let rules = "def is_adult(age):\n    return age >= 18\n\nis_adult = lambda age: age > 99\n";
    let files = [
        ("routepkg/rules.py", rules),
        ("routepkg/ages.py", AGES),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    let finding = classify_files(&files)?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(finding.related_tests.is_empty(), "{finding:?}");
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("route_helper_call")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn given_a_handler_default_that_calls_the_helper_then_no_route_reach() -> Result<(), String> {
    // The default runs when the module is defined, not per request, so the
    // route does not reach the helper.
    let handler = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age(adult: bool = is_adult(30)):\n    return {\"adult\": adult}\n";
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", handler),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    let finding = classify_files(&files)?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(finding.related_tests.is_empty(), "{finding:?}");
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("route_helper_call")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}
