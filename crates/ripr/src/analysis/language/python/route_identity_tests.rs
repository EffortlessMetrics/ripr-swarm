//! Request-client identity for `ApiClientRouteCall`: the application the
//! client targets, the whole request path and receiver aliases (#10110).

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWN_APP: &str = "from fastapi import FastAPI\n\napp = FastAPI()\n\n@app.get(\"/items\")\ndef items():\n    return []\n";

/// Whether the test in `tests/test_items.py` relates to the `items` route
/// owner defined in `route_file` by an API client route call.
fn related(route_file: &str, route_source: &str, test_source: &str) -> Result<bool, String> {
    let owners = extract_owners(Path::new(route_file), route_source);
    let owner = owners
        .iter()
        .find(|owner| owner.name == "items")
        .ok_or_else(|| "expected items owner".to_string())?;
    let tests = extract_tests(Path::new("tests/test_items.py"), test_source);
    Ok(related_test_candidates(owner, &tests)
        .iter()
        .any(|candidate| candidate.relation == PythonRelationKind::ApiClientRouteCall))
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
fn an_unidentified_client_keeps_the_path_and_verb_relation() -> Result<(), String> {
    // A fixture client, an app defined in the test and a router the
    // application includes under another name all stay unknown, not withheld.
    let fixture = "def test_items(client):\n    assert client.get(\"/items\").json() == []\n";
    assert!(related("app/a.py", OWN_APP, fixture)?);
    let local = "from starlette.testclient import TestClient\n\ndef test_items():\n    client = TestClient(make_app())\n    assert client.get(\"/items\").json() == []\n";
    assert!(related("app/a.py", OWN_APP, local)?);
    let router = "from fastapi import APIRouter\n\nrouter = APIRouter()\n\n@router.get(\"/items\")\ndef items():\n    return []\n";
    let included = "from starlette.testclient import TestClient\nfrom app.main import app\n\ndef test_items():\n    client = TestClient(app)\n    assert client.get(\"/items\").json() == []\n";
    assert!(related("app/routes.py", router, included)?);
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
        assert!(!related("app/a.py", OWN_APP, &test)?, "{call}");
    }
    // The literal followed by more arguments is still the whole path.
    let test = "def test_items(client):\n    assert client.get(\"/items\", params={\"q\": 1})\n";
    assert!(related("app/a.py", OWN_APP, test)?);
    // A compound path in the decorator is not a static route either.
    let compound =
        "app = object()\n\n@app.get(\"/items\" + \"/more\")\ndef items():\n    return []\n";
    let test = "def test_items(client):\n    assert client.get(\"/items\")\n";
    assert!(!related("app/a.py", compound, test)?);
    Ok(())
}

#[test]
fn a_single_assignment_receiver_alias_relates() -> Result<(), String> {
    let alias =
        "def test_items(client):\n    api_client = client\n    assert api_client.get(\"/items\")\n";
    assert!(related("app/a.py", OWN_APP, alias)?);
    let built = "from starlette.testclient import TestClient\nfrom app.a import app\n\ndef test_items():\n    http = TestClient(app)\n    assert http.get(\"/items\")\n";
    assert!(related("app/a.py", OWN_APP, built)?);
    // An alias of a client over a rival app is withheld like the client.
    let rival = "from starlette.testclient import TestClient\nfrom app.b import app\n\ndef test_items():\n    client = TestClient(app)\n    api = client\n    assert api.get(\"/items\")\n";
    assert!(!related("app/a.py", OWN_APP, rival)?);
    Ok(())
}

#[test]
fn an_unrelated_or_rebound_receiver_does_not_relate() -> Result<(), String> {
    // A name never bound to a client is not one.
    let other = "def test_items(client):\n    store = Store()\n    assert store.get(\"/items\")\n";
    assert!(!related("app/a.py", OWN_APP, other)?);
    // A name assigned twice may point anywhere by the call.
    let rebound = "def test_items(client):\n    api = client\n    api = Store()\n    assert api.get(\"/items\")\n";
    assert!(!related("app/a.py", OWN_APP, rebound)?);
    Ok(())
}

#[test]
fn the_verb_narrowing_still_applies_through_an_alias() -> Result<(), String> {
    let post = "from fastapi import FastAPI\n\napp = FastAPI()\n\n@app.post(\"/items\")\ndef items():\n    return []\n";
    let test = "def test_items(client):\n    api = client\n    assert api.get(\"/items\")\n";
    assert!(!related("app/a.py", post, test)?);
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
    assert!(related("app/a.py", OWN_APP, test)?);
    Ok(())
}

#[test]
fn mixed_route_decorators_keep_their_own_receiver_and_path() -> Result<(), String> {
    // One handler registered on two receivers with different paths; each
    // path is judged against its own decorator's receiver.
    let routes = "from app.a import app, ops_app\n\n@app.get(\"/items\")\n@ops_app.get(\"/admin/items\")\ndef items():\n    return []\n";
    let ops_app = "from starlette.testclient import TestClient\nfrom app.a import ops_app\n\ndef test_items():\n    client = TestClient(ops_app)\n    assert client.get(\"/admin/items\")\n";
    assert!(related("app/routes.py", routes, ops_app)?);
    let rival_on_ops_app_path = "from starlette.testclient import TestClient\nfrom app.b import ops_app\n\ndef test_items():\n    client = TestClient(ops_app)\n    assert client.get(\"/admin/items\")\n";
    assert!(!related("app/routes.py", routes, rival_on_ops_app_path)?);
    // The other receiver's path is still not withheld by an unrelated `ops_app`.
    let app_path = "from starlette.testclient import TestClient\nfrom app.b import ops_app\n\ndef test_items():\n    client = TestClient(ops_app)\n    assert client.get(\"/items\")\n";
    assert!(related("app/routes.py", routes, app_path)?);
    Ok(())
}

#[test]
fn a_comment_after_the_path_literal_keeps_the_route() -> Result<(), String> {
    let routes =
        "app = object()\n\n@app.get(\"/items\"  # listing\n)\ndef items():\n    return []\n";
    let test = "def test_items(client):\n    assert client.get(\"/items\"  # list\n    )\n";
    assert!(related("app/a.py", routes, test)?);
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
    assert!(!related("app/a.py", OWN_APP, test)?);
    // An annotation on the single binding keeps it a client alias.
    let kept =
        "def test_items(client):\n    api: object = client\n    assert api.get(\"/items\")\n";
    assert!(related("app/a.py", OWN_APP, kept)?);
    Ok(())
}

#[test]
fn a_one_line_suite_rebinding_withdraws_the_alias_like_its_multi_line_form() -> Result<(), String> {
    let one_line = "def test_items(client):\n    api = client\n    if True: api = Store()\n    assert api.get(\"/items\")\n";
    let multi_line = "def test_items(client):\n    api = client\n    if True:\n        api = Store()\n    assert api.get(\"/items\")\n";
    let else_line = "def test_items(client):\n    api = client\n    if flag:\n        pass\n    else: api = Store()\n    assert api.get(\"/items\")\n";
    let untouched = "def test_items(client):\n    api = client\n    if True: other = Store()\n    assert api.get(\"/items\")\n";
    assert!(!related("app/a.py", OWN_APP, one_line)?);
    assert!(!related("app/a.py", OWN_APP, multi_line)?);
    assert!(!related("app/a.py", OWN_APP, else_line)?);
    assert!(related("app/a.py", OWN_APP, untouched)?);
    Ok(())
}
