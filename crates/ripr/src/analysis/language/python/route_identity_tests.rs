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
