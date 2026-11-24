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

fn assert_no_route_reach(handler: &str, label: &str) -> Result<(), String> {
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", handler),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    let finding = classify_files(&files)?;
    // The generic cross-module link reads every call in the handler's span
    // and may still relate the test; the route relation must not be the one
    // that does, and nothing may read weakly exposed.
    assert_ne!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{label}: {finding:?}"
    );
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "{label}: {finding:?}"
    );
    assert!(
        finding
            .related_tests
            .iter()
            .all(|test| test.oracle.is_none()),
        "{label}: {finding:?}"
    );
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("route_helper_call")),
        "{label}: {:?}",
        finding.evidence
    );
    Ok(())
}

/// The route-helper relation claims the handler itself calls the helper.
/// `module_callees` counts a call anywhere in the handler's span, including
/// inside a nested `def`, so the relation reads the handler's own scope
/// instead (#14150 review).
#[test]
fn given_a_helper_called_only_inside_a_nested_function_then_no_route_reach() -> Result<(), String> {
    let unused = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    def unused(value):\n        return is_adult(value)\n    return {\"adult\": True}\n";
    assert_no_route_reach(unused, "an unused nested function")?;

    // Invoking the nested function does not make its call the handler's own.
    let invoked = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    def check(value):\n        return is_adult(value)\n    return {\"adult\": check(30)}\n";
    assert_no_route_reach(invoked, "an invoked nested function")?;
    Ok(())
}

#[test]
fn given_a_helper_called_only_inside_a_lambda_then_no_route_reach() -> Result<(), String> {
    let inline = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    return {\"adult\": sorted([30], key=lambda value: is_adult(value)) == [30]}\n";
    assert_no_route_reach(inline, "a single-line lambda")?;

    // The lambda's body continues on the next line, inside its own brackets.
    let continued = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    adults = list(map(lambda value: bool(\n        is_adult(value),\n    ), ()))\n    return {\"adult\": bool(adults)}\n";
    assert_no_route_reach(continued, "a lambda whose body continues on the next line")?;

    // The lambda's opening bracket sits on an earlier line than the keyword.
    let opened_earlier = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    checks = (\n        lambda value: is_adult(value)\n    )\n    return {\"adult\": bool(checks(30))}\n";
    assert_no_route_reach(opened_earlier, "a lambda opened on an earlier line")?;
    Ok(())
}

/// The prune reads the parsed body, so it does not depend on how a nested
/// definition is spelled or where its block sits.
#[test]
fn nested_definitions_are_pruned_however_they_are_spelled() -> Result<(), String> {
    // `def` followed by a tab, not a space.
    let tabbed = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    def\tunused(value):\n        return is_adult(value)\n    return {\"adult\": True}\n";
    assert_no_route_reach(tabbed, "a def followed by a tab")?;

    // A nested class whose method calls the helper.
    let in_class = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    class Check:\n        def of(self, value):\n            return is_adult(value)\n    return {\"adult\": True}\n";
    assert_no_route_reach(in_class, "a method of a nested class")?;

    // A nested definition inside a branch of the handler.
    let in_branch = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    if True:\n        def unused(value):\n            return is_adult(value)\n    return {\"adult\": True}\n";
    assert_no_route_reach(in_branch, "a def inside a branch")?;
    Ok(())
}

/// The prune removes the nested scope and nothing else: the handler's own
/// call on the same line as a lambda is still its call.
#[test]
fn a_lambda_beside_the_handlers_own_call_hides_nothing() -> Result<(), String> {
    let handler = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    return {\"adult\": is_adult(30), \"sorted\": sorted([1], key=lambda value: value)}\n";
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", handler),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    let finding = classify_files(&files)?;
    assert_reach_only(&finding, "a lambda beside the handler's own call");
    Ok(())
}

/// #14150 review: a route request that another route answers as well may
/// never reach the handler, so a relation that rests on that request alone is
/// withheld, the route relation and the generic caller relations with it. A
/// test that calls the handler itself keeps its relation.
#[test]
fn given_an_ambiguous_route_in_the_helpers_own_module_then_no_route_relation() -> Result<(), String>
{
    // The helper and the handlers in one module; `routes` are the decorators
    // after the helper.
    let finding_for = |routes: &str, test: &str| -> Result<Finding, String> {
        let same_module = format!(
            "app = object()\n\ndef is_adult(age):\n    return age >= 18\n\n{routes}def age():\n    return {{\"adult\": is_adult(30)}}\n"
        );
        let conftest = "import pytest\nfrom starlette.testclient import TestClient\nfrom routepkg.rules import app\n\n@pytest.fixture\ndef client():\n    return TestClient(app)\n";
        let files = [
            ("routepkg/rules.py", same_module.as_str()),
            ("tests/conftest.py", conftest),
            ("tests/test_ages.py", test),
        ];
        let (mut owners, tests) = super::fixtures_tests::workspace(&files);
        let declared = files
            .iter()
            .map(|(_, source)| super::related_tests::route_decorator_lines(source))
            .sum();
        super::related_tests::apply_route_overlaps(&mut owners, false, declared);
        classify_change_with_old(
            Path::new("routepkg/rules.py"),
            4,
            "    return age >= 21",
            Some("    return age >= 18"),
            &owners,
            &tests,
        )
        .ok_or_else(|| "expected a finding".to_string())
    };
    const ONE_ROUTE: &str = "@app.get(\"/age/30\")\n";
    const TWO_ROUTES: &str = "@app.get(\"/age/{age}\")\ndef age_param(age):\n    return {\"adult\": False}\n\n@app.get(\"/age/30\")\n";
    let relates = |finding: &Finding, relation: &str| {
        finding
            .evidence
            .iter()
            .any(|line| line.contains(&format!("related_test_relation: {relation} ")))
    };

    // Control: with the one route the request identifies the handler, and its
    // own call of the helper is the route relation.
    let identified = finding_for(ONE_ROUTE, ROUTE_TEST)?;
    assert!(relates(&identified, "route_helper_call"), "{identified:?}");
    assert_eq!(
        identified.class,
        ExposureClass::WeaklyExposed,
        "{identified:?}"
    );

    // A parameterized route that also answers the request: dispatch is not
    // read, so neither the route relation nor the generic caller relation
    // that rests on the same request is granted.
    let ambiguous = finding_for(TWO_ROUTES, ROUTE_TEST)?;
    assert_ne!(
        ambiguous.class,
        ExposureClass::WeaklyExposed,
        "{ambiguous:?}"
    );
    assert_ne!(ambiguous.class, ExposureClass::Exposed, "{ambiguous:?}");
    assert!(
        !relates(&ambiguous, "route_helper_call") && !relates(&ambiguous, "helper_owner_call"),
        "{:?}",
        ambiguous.evidence
    );

    // Independent evidence: a test that calls the handler itself reaches the
    // helper whoever answers the route.
    let calls_handler = "from routepkg.rules import age\n\n\ndef test_age_route():\n    assert age() == {\"adult\": True}\n";
    let direct = finding_for(TWO_ROUTES, calls_handler)?;
    assert!(relates(&direct, "helper_owner_call"), "{direct:?}");
    assert_eq!(direct.class, ExposureClass::WeaklyExposed, "{direct:?}");
    Ok(())
}

/// The names the handler's own scope binds are read from the parse, so each
/// of these shadows the imported helper and the handler's own call is not the
/// helper's: a `match` capture, an annotated assignment, a tuple target, a
/// chained assignment and `except ... as name`.
#[test]
fn a_name_the_handlers_own_scope_binds_is_not_the_helper() -> Result<(), String> {
    let head = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n";
    for (label, binding) in [
        (
            "a match capture",
            "    match {\"checker\": 1}:\n        case {\"checker\": is_adult}:\n            pass\n",
        ),
        (
            "an annotated assignment",
            "    is_adult: object = lambda value: True\n",
        ),
        (
            "a tuple target",
            "    (mode, is_adult) = (1, lambda value: True)\n",
        ),
        (
            "a chained assignment",
            "    checker = is_adult = lambda value: True\n",
        ),
        (
            "an except clause",
            "    try:\n        pass\n    except ValueError as is_adult:\n        pass\n",
        ),
    ] {
        let handler = format!("{head}{binding}    return {{\"adult\": is_adult(30)}}\n");
        assert_no_route_reach(&handler, label)?;
    }
    Ok(())
}

/// What runs while the handler runs is the handler's own call: a decorator or
/// a default of a nested definition, and the first iterable of a generator
/// expression. What runs only when the handler calls or iterates it is not:
/// a nested block, a `lambda` body, a class body and the rest of a generator
/// expression.
#[test]
fn a_helper_is_credited_where_the_handler_runs_and_only_there() -> Result<(), String> {
    let head = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n";
    for (label, body) in [
        (
            "a decorator of a nested definition",
            "    @is_adult(30)\n    def inner():\n        return None\n",
        ),
        (
            "a default of a nested definition",
            "    def inner(check=is_adult(30)):\n        return None\n",
        ),
        (
            "the first iterable of a generator expression",
            "    checks = (value for value in [is_adult(30)])\n",
        ),
    ] {
        let handler = format!("{head}{body}    return {{\"adult\": True}}\n");
        let files = [
            ("routepkg/rules.py", RULES),
            ("routepkg/ages.py", handler.as_str()),
            ("tests/conftest.py", CONFTEST),
            ("tests/test_ages.py", ROUTE_TEST),
        ];
        assert_reach_only(&classify_files(&files)?, label);
    }
    // The route relation alone: a test that calls the handler itself keeps
    // the generic reading, which does not name the helper for this handler.
    let handler = format!(
        "{head}    @is_adult(30)\n    def inner():\n        return None\n    return {{\"adult\": True}}\n"
    );
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", handler.as_str()),
        (
            "tests/test_ages.py",
            "from routepkg.ages import age\n\n\ndef test_age():\n    assert age() == {\"adult\": True}\n",
        ),
    ];
    let finding = classify_files(&files)?;
    assert_eq!(
        finding.class,
        ExposureClass::NoStaticPath,
        "a direct call of the handler: {finding:?}"
    );
    for (label, body) in [
        (
            "the block of a nested definition",
            "    def inner():\n        return is_adult(30)\n",
        ),
        (
            "the body of a class",
            "    class Inner:\n        adult = is_adult(30)\n",
        ),
        (
            "the element of a generator expression",
            "    checks = (is_adult(value) for value in [30])\n",
        ),
        (
            "a later iterable of a generator expression",
            "    checks = (value for value in [] for other in [is_adult(30)])\n",
        ),
        (
            "a condition of a generator expression",
            "    checks = (value for value in [30] if is_adult(value))\n",
        ),
    ] {
        let handler = format!("{head}{body}    return {{\"adult\": True}}\n");
        assert_no_route_reach(&handler, label)?;
    }
    Ok(())
}

/// A comprehension's target belongs to the comprehension's scope in every
/// Python 3 form, so it does not hide the module's helper from the handler's
/// own call, although it hides the helper from the textual list of the
/// handler's callees (#14150 review).
#[test]
fn a_comprehension_target_does_not_hide_the_helper_from_its_route() -> Result<(), String> {
    let handler = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    checks = [is_adult for is_adult in [1]]\n    return {\"adult\": is_adult(30)}\n";
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", handler),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    assert_reach_only(&classify_files(&files)?, "a list comprehension target");
    Ok(())
}

/// A `global` declaration binds nothing in the handler's own scope
/// (`own_scope_tests`), and the relation is still withheld for it: a
/// function that declares the helper's name `global` may rebind it at module
/// level, so the module-level rule does not count the helper as the unique
/// binding.
#[test]
fn a_global_declaration_of_the_helper_keeps_the_module_rule() -> Result<(), String> {
    let handler = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    global is_adult\n    return {\"adult\": is_adult(30)}\n";
    assert_no_route_reach(handler, "a global declaration")?;
    Ok(())
}

/// The prune reads the parse the adapter already made, so a handler is read
/// the same however its body is spelled: an `async` handler that awaits, a
/// one-statement body, a tab-indented body and a list comprehension, which
/// runs where it is written, all keep the handler's own call.
#[test]
fn the_handlers_own_call_is_read_however_its_body_is_spelled() -> Result<(), String> {
    let awaits = "from routepkg.rules import is_adult\n\nasync def load():\n    return {}\n\napp = object()\n\n@app.get(\"/age/30\")\nasync def age():\n    await load()\n    return {\"adult\": is_adult(30)}\n";
    let one_statement = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age(): return {\"adult\": is_adult(30)}\n";
    let tab_indented = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n\tvalue = 30\n\treturn {\"adult\": is_adult(value)}\n";
    let comprehension = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    return {\"adult\": all([is_adult(value) for value in [30]])}\n";
    for (label, handler) in [
        ("an async handler that awaits", awaits),
        ("a one-statement body", one_statement),
        ("a tab-indented body", tab_indented),
        ("a list comprehension", comprehension),
    ] {
        let files = [
            ("routepkg/rules.py", RULES),
            ("routepkg/ages.py", handler),
            ("tests/conftest.py", CONFTEST),
            ("tests/test_ages.py", ROUTE_TEST),
        ];
        assert_reach_only(&classify_files(&files)?, label);
    }
    Ok(())
}

/// A generator expression's element, like a lambda's body, runs only when
/// something iterates it. The handler does not, so the helper is not called.
#[test]
fn given_a_helper_called_only_inside_a_generator_expression_then_no_route_reach()
-> Result<(), String> {
    let handler = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    checks = (is_adult(value) for value in [30])\n    return {\"adult\": True}\n";
    assert_no_route_reach(handler, "a generator expression nothing iterates")
}

/// A nested function with the helper's name is what the handler's own call
/// reaches, not the imported helper: the binding is read before the prune.
#[test]
fn given_a_nested_function_named_like_the_helper_then_the_call_is_not_the_helpers()
-> Result<(), String> {
    let handler = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    def is_adult(value):\n        return True\n    return {\"adult\": is_adult(30)}\n";
    assert_no_route_reach(handler, "a nested def that shadows the imported helper")
}

/// Every statement kind is walked, so a lambda is pruned wherever it sits: in
/// a `match` guard and in a `with` item, not only in an assignment or return.
#[test]
fn a_lambda_in_a_match_guard_or_a_with_item_is_pruned() -> Result<(), String> {
    let in_guard = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    match 1:\n        case value if sorted([value], key=lambda other: is_adult(other)):\n            pass\n    return {\"adult\": True}\n";
    assert_no_route_reach(in_guard, "a lambda in a match guard")?;
    let in_with = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    with holder(lambda other: is_adult(other)):\n        pass\n    return {\"adult\": True}\n";
    assert_no_route_reach(in_with, "a lambda in a with item")
}

/// Control for the two negatives above: a nested function does not hide a
/// helper the handler calls itself.
#[test]
fn given_a_handler_with_a_nested_function_that_also_calls_the_helper_then_reach_only()
-> Result<(), String> {
    let handler = "from routepkg.rules import is_adult\n\napp = object()\n\n@app.get(\"/age/30\")\ndef age():\n    def unused(value):\n        return False\n    return {\"adult\": is_adult(30)}\n";
    let files = [
        ("routepkg/rules.py", RULES),
        ("routepkg/ages.py", handler),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_ages.py", ROUTE_TEST),
    ];
    let finding = classify_files(&files)?;
    assert_reach_only(&finding, "a nested function beside the handler's own call");
    Ok(())
}
