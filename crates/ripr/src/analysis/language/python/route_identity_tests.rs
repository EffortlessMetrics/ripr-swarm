//! Request-client identity for `ApiClientRouteCall`: the application the
//! client targets, the whole request path and receiver aliases (#10110).

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWN_APP: &str = "from fastapi import FastAPI\n\napp = FastAPI()\n\n@app.get(\"/items\")\ndef items():\n    return []\n";

/// Whether the test in `tests/test_items.py` relates to the `items` route
/// owner defined in `route_file` by an API client route call whose application
/// is identified as the owner's own (oracle-eligible).
fn related(route_file: &str, route_source: &str, test_source: &str) -> Result<bool, String> {
    Ok(route_relation(route_file, route_source, test_source)?
        == Some(PythonRelationKind::ApiClientRouteCall))
}

/// Whether the test reaches the route through any client call, identified or
/// not: the request, receiver and path matched.
fn reached(route_file: &str, route_source: &str, test_source: &str) -> Result<bool, String> {
    Ok(
        route_relation(route_file, route_source, test_source)?.is_some_and(|relation| {
            matches!(
                relation,
                PythonRelationKind::ApiClientRouteCall
                    | PythonRelationKind::ApiClientRouteUnidentifiedApp
            )
        }),
    )
}

fn route_relation(
    route_file: &str,
    route_source: &str,
    test_source: &str,
) -> Result<Option<PythonRelationKind>, String> {
    let owners = extract_owners(Path::new(route_file), route_source);
    let owner = owners
        .iter()
        .find(|owner| owner.name == "items")
        .ok_or_else(|| "expected items owner".to_string())?;
    let tests = extract_tests(Path::new("tests/test_items.py"), test_source);
    Ok(related_test_candidates(owner, &tests)
        .first()
        .map(|candidate| candidate.relation))
}

#[test]
fn a_client_over_the_owners_own_app_relates() -> Result<(), String> {
    let test = "from starlette.testclient import TestClient\nfrom app.a import app\n\ndef test_items():\n    client = TestClient(app)\n    assert client.get(\"/items\").json() == []\n";
    assert!(related("app/a.py", OWN_APP, test)?);
    Ok(())
}

#[test]
fn a_client_over_a_same_named_app_in_another_module_does_not_relate() -> Result<(), String> {
    // Both modules define `app` and `GET /items`; the client targets b's.
    let test = "from starlette.testclient import TestClient\nfrom app.b import app\n\ndef test_items():\n    client = TestClient(app)\n    assert client.get(\"/items\").json() == []\n";
    assert!(!related("app/a.py", OWN_APP, test)?);
    // `app=` keyword and an import alias name the same application.
    let keyword = "from starlette.testclient import TestClient\nfrom app.b import app as other\n\ndef test_items():\n    client = TestClient(app=other)\n    assert client.get(\"/items\").json() == []\n";
    assert!(!related("app/a.py", OWN_APP, keyword)?);
    Ok(())
}

#[test]
fn an_unidentified_client_reaches_the_route_without_oracle_credit() -> Result<(), String> {
    // A fixture the adapter cannot read, a factory-built app and a router the
    // application includes under another name reach the route, but nothing
    // establishes the application: reach evidence only, never an oracle.
    let unidentified = Some(PythonRelationKind::ApiClientRouteUnidentifiedApp);
    let fixture = "def test_items(client):\n    assert client.get(\"/items\").json() == []\n";
    assert_eq!(route_relation("app/a.py", OWN_APP, fixture)?, unidentified);
    let local = "from starlette.testclient import TestClient\n\ndef test_items():\n    client = TestClient(make_app())\n    assert client.get(\"/items\").json() == []\n";
    assert_eq!(route_relation("app/a.py", OWN_APP, local)?, unidentified);
    let router = "from fastapi import APIRouter\n\nrouter = APIRouter()\n\n@router.get(\"/items\")\ndef items():\n    return []\n";
    let included = "from starlette.testclient import TestClient\nfrom app.main import app\n\ndef test_items():\n    client = TestClient(app)\n    assert client.get(\"/items\").json() == []\n";
    assert_eq!(
        route_relation("app/routes.py", router, included)?,
        unidentified
    );
    assert!(!PythonRelationKind::ApiClientRouteUnidentifiedApp.uses_oracle());
    // The identified form of the same call keeps the oracle relation.
    let own = "from starlette.testclient import TestClient\nfrom app.a import app\n\ndef test_items():\n    client = TestClient(app)\n    assert client.get(\"/items\").json() == []\n";
    assert!(related("app/a.py", OWN_APP, own)?);
    Ok(())
}

#[test]
fn a_compound_request_path_is_not_the_route_literal() -> Result<(), String> {
    for call in [
        "client.get(\"/items\" + \"/different\")",
        "client.get(\"/items\" \"/different\")",
        "client.get(\"/items/%s\" % item_id)",
        "client.get(\"/items\".format(1))",
        "client.get(\"/items\" if flag else \"/other\")",
    ] {
        let test = format!("def test_items(client):\n    assert {call}\n");
        assert!(!reached("app/a.py", OWN_APP, &test)?, "{call}");
    }
    // The literal followed by more arguments is still the whole path.
    let test = "def test_items(client):\n    assert client.get(\"/items\", params={\"q\": 1})\n";
    assert!(reached("app/a.py", OWN_APP, test)?);
    // A compound path in the decorator is not a static route either.
    let compound =
        "app = object()\n\n@app.get(\"/items\" + \"/more\")\ndef items():\n    return []\n";
    let test = "def test_items(client):\n    assert client.get(\"/items\")\n";
    assert!(!reached("app/a.py", compound, test)?);
    Ok(())
}

#[test]
fn a_single_assignment_receiver_alias_relates() -> Result<(), String> {
    let alias =
        "def test_items(client):\n    api_client = client\n    assert api_client.get(\"/items\")\n";
    assert!(reached("app/a.py", OWN_APP, alias)?);
    let built = "from starlette.testclient import TestClient\nfrom app.a import app\n\ndef test_items():\n    http = TestClient(app)\n    assert http.get(\"/items\")\n";
    assert!(reached("app/a.py", OWN_APP, built)?);
    // An alias of a client over a rival app is withheld like the client.
    let rival = "from starlette.testclient import TestClient\nfrom app.b import app\n\ndef test_items():\n    client = TestClient(app)\n    api = client\n    assert api.get(\"/items\")\n";
    assert!(!reached("app/a.py", OWN_APP, rival)?);
    Ok(())
}

#[test]
fn an_unrelated_or_rebound_receiver_does_not_relate() -> Result<(), String> {
    // A name never bound to a client is not one.
    let other = "def test_items(client):\n    store = Store()\n    assert store.get(\"/items\")\n";
    assert!(!reached("app/a.py", OWN_APP, other)?);
    // A name assigned twice may point anywhere by the call.
    let rebound = "def test_items(client):\n    api = client\n    api = Store()\n    assert api.get(\"/items\")\n";
    assert!(!reached("app/a.py", OWN_APP, rebound)?);
    Ok(())
}

#[test]
fn the_verb_narrowing_still_applies_through_an_alias() -> Result<(), String> {
    let post = "from fastapi import FastAPI\n\napp = FastAPI()\n\n@app.post(\"/items\")\ndef items():\n    return []\n";
    let test = "def test_items(client):\n    api = client\n    assert api.get(\"/items\")\n";
    assert!(!reached("app/a.py", post, test)?);
    Ok(())
}

#[test]
fn a_shared_app_imported_by_owner_and_test_relates() -> Result<(), String> {
    // The owner's file imports the application it decorates with; the test
    // imports the same name from the same module, so it is one object.
    let routes = "from app.main import app\n\n@app.get(\"/items\")\ndef items():\n    return []\n";
    let test = "from starlette.testclient import TestClient\nfrom app.main import app\n\ndef test_items():\n    client = TestClient(app)\n    assert client.get(\"/items\")\n";
    assert!(related("app/routes.py", routes, test)?);
    // The same owner against an application from yet another module is a rival.
    let other = "from starlette.testclient import TestClient\nfrom app.other import app\n\ndef test_items():\n    client = TestClient(app)\n    assert client.get(\"/items\")\n";
    assert!(!related("app/routes.py", routes, other)?);
    Ok(())
}

#[test]
fn a_package_that_re_exports_the_owners_app_relates() -> Result<(), String> {
    // `pkg/main.py` defines `app`; the test imports it from the package.
    let test = "from starlette.testclient import TestClient\nfrom pkg import app\n\ndef test_items():\n    client = TestClient(app)\n    assert client.get(\"/items\")\n";
    assert!(related("pkg/main.py", OWN_APP, test)?);
    // A sibling package is not a parent of the owner's module.
    let sibling = "from starlette.testclient import TestClient\nfrom other import app\n\ndef test_items():\n    client = TestClient(app)\n    assert client.get(\"/items\")\n";
    assert!(!related("pkg/main.py", OWN_APP, sibling)?);
    Ok(())
}

#[test]
fn a_with_bound_client_over_a_rival_app_does_not_relate() -> Result<(), String> {
    let rival = "from starlette.testclient import TestClient\nfrom app.b import app\n\ndef test_items():\n    with TestClient(app) as client:\n        assert client.get(\"/items\")\n";
    assert!(!related("app/a.py", OWN_APP, rival)?);
    let own = "from starlette.testclient import TestClient\nfrom app.a import app\n\ndef test_items():\n    with TestClient(app) as http:\n        assert http.get(\"/items\")\n";
    assert!(related("app/a.py", OWN_APP, own)?);
    Ok(())
}

#[test]
fn an_async_client_is_a_route_client_only_over_an_application() -> Result<(), String> {
    let external = "from httpx import AsyncClient\n\nasync def test_items():\n    http = AsyncClient(base_url=\"https://payments.example\")\n    assert await http.get(\"/items\")\n";
    assert!(!related("app/a.py", OWN_APP, external)?);
    let over_app = "from httpx import AsyncClient\nfrom app.a import app\n\nasync def test_items():\n    http = AsyncClient(app=app, base_url=\"http://test\")\n    assert await http.get(\"/items\")\n";
    assert!(related("app/a.py", OWN_APP, over_app)?);
    let rival = "from httpx import AsyncClient\nfrom app.b import app\n\nasync def test_items():\n    http = AsyncClient(base_url=\"http://test\", app=app)\n    assert await http.get(\"/items\")\n";
    assert!(!related("app/a.py", OWN_APP, rival)?);
    Ok(())
}

#[test]
fn a_response_is_not_a_client_receiver() -> Result<(), String> {
    let test = "from starlette.testclient import TestClient\nfrom app.a import app\n\ndef test_items():\n    reply = TestClient(app).get(\"/other\")\n    assert reply.get(\"/items\")\n";
    assert!(!related("app/a.py", OWN_APP, test)?);
    Ok(())
}

#[test]
fn a_client_rebound_before_the_call_is_not_identified_by_its_first_binding() -> Result<(), String> {
    // `client` is constructed over a rival app, then reassigned; the first
    // binding no longer decides, so the relation stays unidentified.
    let test = "from starlette.testclient import TestClient\nfrom app.b import app\n\ndef test_items(make):\n    client = TestClient(app)\n    client = make()\n    assert client.get(\"/items\")\n";
    assert!(reached("app/a.py", OWN_APP, test)?);
    Ok(())
}

#[test]
fn mixed_route_decorators_keep_their_own_receiver_and_path() -> Result<(), String> {
    // One handler registered on two receivers with different paths; each
    // path is judged against its own decorator's receiver.
    let routes = "from app.a import app, ops_app\n\n@app.get(\"/items\")\n@ops_app.get(\"/admin/items\")\ndef items():\n    return []\n";
    let ops_app = "from starlette.testclient import TestClient\nfrom app.a import ops_app\n\ndef test_items():\n    client = TestClient(ops_app)\n    assert client.get(\"/admin/items\")\n";
    assert!(reached("app/routes.py", routes, ops_app)?);
    let rival_on_ops_app_path = "from starlette.testclient import TestClient\nfrom app.b import ops_app\n\ndef test_items():\n    client = TestClient(ops_app)\n    assert client.get(\"/admin/items\")\n";
    assert!(!reached("app/routes.py", routes, rival_on_ops_app_path)?);
    // The other receiver's path is still not withheld by an unrelated `ops_app`.
    let app_path = "from starlette.testclient import TestClient\nfrom app.b import ops_app\n\ndef test_items():\n    client = TestClient(ops_app)\n    assert client.get(\"/items\")\n";
    assert!(reached("app/routes.py", routes, app_path)?);
    Ok(())
}

#[test]
fn a_comment_after_the_path_literal_keeps_the_route() -> Result<(), String> {
    let routes =
        "app = object()\n\n@app.get(\"/items\"  # listing\n)\ndef items():\n    return []\n";
    let test = "def test_items(client):\n    assert client.get(\"/items\"  # list\n    )\n";
    assert!(reached("app/a.py", routes, test)?);
    Ok(())
}

const RIVAL_HEAD: &str = "from starlette.testclient import TestClient\nfrom app.b import app as other\nfrom app.a import app\n\ndef test_items():\n";

fn rival_and_own(body: &str) -> Result<(bool, bool), String> {
    let rival = format!("{RIVAL_HEAD}{body}").replace("APP", "other");
    let own = format!("{RIVAL_HEAD}{body}").replace("APP", "app");
    Ok((
        related("app/a.py", OWN_APP, &rival)?,
        related("app/a.py", OWN_APP, &own)?,
    ))
}

#[test]
fn keyword_spacing_and_wrapped_constructions_still_withhold_a_rival() -> Result<(), String> {
    for client in [
        "    client = TestClient(app = APP)\n",
        "    client = TestClient(\n        app=APP,\n    )\n",
        "    client = TestClient(\n        app = APP  # under test\n    )\n",
    ] {
        let body = format!("{client}    assert client.get(\"/items\")\n");
        assert_eq!(rival_and_own(&body)?, (false, true), "{client}");
    }
    Ok(())
}

#[test]
fn a_long_alias_chain_keeps_the_rival_check() -> Result<(), String> {
    for links in 1..=5usize {
        let mut body = String::from("    client = TestClient(APP)\n");
        let mut previous = "client".to_string();
        for link in 0..links {
            body.push_str(&format!("    c{link} = {previous}\n"));
            previous = format!("c{link}");
        }
        body.push_str(&format!("    assert {previous}.get(\"/items\")\n"));
        assert_eq!(rival_and_own(&body)?, (false, true), "{links} links");
    }
    Ok(())
}

#[test]
fn an_annotated_rebinding_does_not_leave_the_alias_a_client() -> Result<(), String> {
    let test = "def test_items(client):\n    api = client\n    api: object = Store()\n    assert api.get(\"/items\")\n";
    assert!(!reached("app/a.py", OWN_APP, test)?);
    // An annotation on the single binding keeps it a client alias.
    let kept =
        "def test_items(client):\n    api: object = client\n    assert api.get(\"/items\")\n";
    assert!(reached("app/a.py", OWN_APP, kept)?);
    Ok(())
}

#[test]
fn a_one_line_suite_rebinding_withdraws_the_alias_like_its_multi_line_form() -> Result<(), String> {
    let one_line = "def test_items(client):\n    api = client\n    if True: api = Store()\n    assert api.get(\"/items\")\n";
    let multi_line = "def test_items(client):\n    api = client\n    if True:\n        api = Store()\n    assert api.get(\"/items\")\n";
    let else_line = "def test_items(client):\n    api = client\n    if flag:\n        pass\n    else: api = Store()\n    assert api.get(\"/items\")\n";
    let untouched = "def test_items(client):\n    api = client\n    if True: other = Store()\n    assert api.get(\"/items\")\n";
    assert!(!reached("app/a.py", OWN_APP, one_line)?);
    assert!(!reached("app/a.py", OWN_APP, multi_line)?);
    assert!(!reached("app/a.py", OWN_APP, else_line)?);
    assert!(reached("app/a.py", OWN_APP, untouched)?);
    Ok(())
}

#[test]
fn a_different_symbol_from_the_owners_shared_module_does_not_relate() -> Result<(), String> {
    let owner = "from common import app\n\n@app.get(\"/items\")\ndef items():\n    return []\n";
    let same = "from starlette.testclient import TestClient\nfrom common import app\n\ndef test_items():\n    client = TestClient(app)\n    assert client.get(\"/items\")\n";
    assert!(related("app/a.py", owner, same)?);
    let other = "from starlette.testclient import TestClient\nfrom common import other_app as app\n\ndef test_items():\n    client = TestClient(app)\n    assert client.get(\"/items\")\n";
    assert!(!related("app/a.py", owner, other)?);
    Ok(())
}

#[test]
fn a_different_symbol_from_the_owners_own_module_does_not_relate() -> Result<(), String> {
    let other = "from starlette.testclient import TestClient\nfrom app.a import other_app as app\n\ndef test_items():\n    client = TestClient(app)\n    assert client.get(\"/items\")\n";
    assert!(!related("app/a.py", OWN_APP, other)?);
    Ok(())
}

/// The relation of `tests/test_items.py` to the `items` owner in `app/a.py`
/// when the client comes from a `tests/conftest.py` fixture.
fn fixture_relation(conftest: &str) -> Result<Option<PythonRelationKind>, String> {
    let test = "def test_items(client):\n    assert client.get(\"/items\").json() == []\n";
    let (owners, tests) = super::fixtures_tests::workspace(&[
        ("app/a.py", OWN_APP),
        ("tests/conftest.py", conftest),
        ("tests/test_items.py", test),
    ]);
    let owner = owners
        .iter()
        .find(|owner| owner.name == "items")
        .ok_or_else(|| "expected items owner".to_string())?;
    Ok(related_test_candidates(owner, &tests)
        .first()
        .map(|candidate| candidate.relation))
}

const CLIENT_IMPORTS: &str = "import pytest\nfrom starlette.testclient import TestClient\n";

#[test]
fn a_conftest_client_fixture_over_the_owners_app_is_identified() -> Result<(), String> {
    let identified = Some(PythonRelationKind::ApiClientRouteCall);
    let returned = format!(
        "{CLIENT_IMPORTS}from app.a import app\n\n@pytest.fixture\ndef client():\n    return TestClient(app)\n"
    );
    assert_eq!(fixture_relation(&returned)?, identified);
    let yielded = format!(
        "{CLIENT_IMPORTS}from app.a import app\n\n@pytest.fixture\ndef client():\n    with TestClient(app) as http:\n        yield http\n"
    );
    assert_eq!(fixture_relation(&yielded)?, identified);
    let bound = format!(
        "{CLIENT_IMPORTS}from app.a import app\n\n@pytest.fixture\ndef client():\n    http = TestClient(app)\n    yield http\n"
    );
    assert_eq!(fixture_relation(&bound)?, identified);
    Ok(())
}

#[test]
fn a_conftest_client_fixture_over_a_rival_app_does_not_relate() -> Result<(), String> {
    let rival = format!(
        "{CLIENT_IMPORTS}from app.b import app\n\n@pytest.fixture\ndef client():\n    return TestClient(app)\n"
    );
    assert_eq!(fixture_relation(&rival)?, None);
    let other_symbol = format!(
        "{CLIENT_IMPORTS}from app.a import other_app as app\n\n@pytest.fixture\ndef client():\n    return TestClient(app)\n"
    );
    assert_eq!(fixture_relation(&other_symbol)?, None);
    Ok(())
}

#[test]
fn an_unreadable_client_fixture_reaches_the_route_without_oracle_credit() -> Result<(), String> {
    let unidentified = Some(PythonRelationKind::ApiClientRouteUnidentifiedApp);
    // A factory-built application.
    let factory = format!(
        "{CLIENT_IMPORTS}from app.factory import make_app\n\n@pytest.fixture\ndef client():\n    return TestClient(make_app())\n"
    );
    assert_eq!(fixture_relation(&factory)?, unidentified);
    // A client fixture that builds on another fixture.
    let nested = format!(
        "{CLIENT_IMPORTS}\n@pytest.fixture\ndef client(app):\n    return TestClient(app)\n"
    );
    assert_eq!(fixture_relation(&nested)?, unidentified);
    // A one-line compound suite is still an exit over a different application.
    let one_line = format!(
        "{CLIENT_IMPORTS}from app.a import app\nfrom app.b import other_app\n\n@pytest.fixture\ndef client(request):\n    if request.param: return TestClient(other_app)\n    return TestClient(app)\n"
    );
    assert_eq!(fixture_relation(&one_line)?, unidentified);
    let semicolon = format!(
        "{CLIENT_IMPORTS}from app.a import app\nfrom app.b import other_app\n\n@pytest.fixture\ndef client(request):\n    if request.param:\n        x = 1; return TestClient(other_app)\n    return TestClient(app)\n"
    );
    assert_eq!(fixture_relation(&semicolon)?, unidentified);
    // Two exits over different applications.
    let split = format!(
        "{CLIENT_IMPORTS}from app.a import app\nfrom app.b import other\n\n@pytest.fixture\ndef client(request):\n    if request.param:\n        return TestClient(app)\n    return TestClient(other)\n"
    );
    assert_eq!(fixture_relation(&split)?, unidentified);
    // No fixture definition at all: the parameter is supplied by something
    // the adapter cannot see (a plugin or a conftest outside the workspace).
    let (owners, tests) = super::fixtures_tests::workspace(&[
        ("app/a.py", OWN_APP),
        (
            "tests/test_items.py",
            "def test_items(client):\n    assert client.get(\"/items\")\n",
        ),
    ]);
    let owner = owners
        .iter()
        .find(|owner| owner.name == "items")
        .ok_or_else(|| "expected items owner".to_string())?;
    assert_eq!(
        related_test_candidates(owner, &tests)
            .first()
            .map(|candidate| candidate.relation),
        unidentified
    );
    Ok(())
}

#[test]
fn a_shadowed_application_name_is_not_the_import_it_spells() -> Result<(), String> {
    let unidentified = Some(PythonRelationKind::ApiClientRouteUnidentifiedApp);
    let local = "from starlette.testclient import TestClient\nfrom app.a import app\n\ndef test_items():\n    app = build_other_app()\n    client = TestClient(app)\n    assert client.get(\"/items\")\n";
    assert_eq!(route_relation("app/a.py", OWN_APP, local)?, unidentified);
    let fixture_named = "from starlette.testclient import TestClient\nfrom app.a import app\n\ndef test_items(app):\n    client = TestClient(app)\n    assert client.get(\"/items\")\n";
    assert_eq!(
        route_relation("app/a.py", OWN_APP, fixture_named)?,
        unidentified
    );
    // The same body without the shadow is the owner's own app.
    let own = "from starlette.testclient import TestClient\nfrom app.a import app\n\ndef test_items():\n    client = TestClient(app)\n    assert client.get(\"/items\")\n";
    assert!(related("app/a.py", OWN_APP, own)?);
    // A conftest client fixture over its own parameter is not an import.
    let shadow_fixture = format!(
        "{CLIENT_IMPORTS}from app.a import app\n\n@pytest.fixture\ndef app():\n    return make_app()\n\n@pytest.fixture\ndef client(app):\n    return TestClient(app)\n"
    );
    assert_eq!(
        fixture_relation(&shadow_fixture)?,
        Some(PythonRelationKind::ApiClientRouteUnidentifiedApp)
    );
    Ok(())
}

#[test]
fn a_fixture_parameter_the_test_rebinds_or_a_class_override_is_unidentified() -> Result<(), String>
{
    let unidentified = Some(PythonRelationKind::ApiClientRouteUnidentifiedApp);
    let conftest = format!(
        "{CLIENT_IMPORTS}from app.a import app\n\n@pytest.fixture\ndef client():\n    return TestClient(app)\n"
    );
    let run = |test: &str| -> Result<Option<PythonRelationKind>, String> {
        let (owners, tests) = super::fixtures_tests::workspace(&[
            ("app/a.py", OWN_APP),
            ("tests/conftest.py", &conftest),
            ("tests/test_items.py", test),
        ]);
        let owner = owners
            .iter()
            .find(|owner| owner.name == "items")
            .ok_or_else(|| "expected items owner".to_string())?;
        Ok(related_test_candidates(owner, &tests)
            .first()
            .map(|candidate| candidate.relation))
    };
    let plain = "def test_items(client):\n    assert client.get(\"/items\")\n";
    assert_eq!(run(plain)?, Some(PythonRelationKind::ApiClientRouteCall));
    // A fixture parameter that the body rebinds is whatever it was rebound to.
    let rebound =
        "def test_items(client):\n    client = make_client()\n    assert client.get(\"/items\")\n";
    assert_eq!(run(rebound)?, unidentified);
    // An alias of the fixture parameter leads back to it.
    let alias = "def test_items(client):\n    http = client\n    assert http.get(\"/items\")\n";
    assert_eq!(run(alias)?, Some(PythonRelationKind::ApiClientRouteCall));
    // A class method may sit under a class-level override that is not recorded.
    let method = "class TestItems:\n    def test_items(self, client):\n        assert client.get(\"/items\")\n";
    assert_eq!(run(method)?, unidentified);
    Ok(())
}

#[test]
fn an_owner_modules_reexport_of_its_imported_app_is_its_own_app() -> Result<(), String> {
    let routes = "from app.core import app\n\n@app.get(\"/items\")\ndef items():\n    return []\n";
    let via = |import: &str| {
        format!(
            "from starlette.testclient import TestClient\n{import}\n\ndef test_items():\n    client = TestClient(app)\n    assert client.get(\"/items\")\n"
        )
    };
    // The owner's module re-exports the app it imported.
    assert!(related(
        "app/routes.py",
        routes,
        &via("from app.routes import app")
    )?);
    // A package above the owner may define another `app`.
    assert_eq!(
        route_relation("app/routes.py", routes, &via("from app import app"))?,
        Some(PythonRelationKind::ApiClientRouteUnidentifiedApp)
    );
    // The shared origin itself and a third module keep their readings.
    assert!(related(
        "app/routes.py",
        routes,
        &via("from app.core import app")
    )?);
    assert_eq!(
        route_relation("app/routes.py", routes, &via("from app.other import app"))?,
        None
    );
    Ok(())
}
