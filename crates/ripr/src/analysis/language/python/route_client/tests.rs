use super::{client_app_name, client_receivers};

fn receivers(body: &str) -> Vec<String> {
    client_receivers(body)
}

#[test]
fn aliases_of_any_chain_length_are_receivers_and_resolve_the_application() {
    for links in 1..=6usize {
        let mut body = String::from("client = TestClient(app)\n");
        let mut previous = "client".to_string();
        for link in 0..links {
            let name = format!("alias{link}");
            body.push_str(&format!("{name} = {previous}\n"));
            previous = name;
        }
        assert!(receivers(&body).contains(&previous), "{links} links");
        assert_eq!(
            client_app_name(&body, &previous).as_deref(),
            Some("app"),
            "{links} links"
        );
    }
}

#[test]
fn a_chain_in_any_line_order_resolves() {
    let body = "c = b\nb = a\na = client\nclient = TestClient(app)\n";
    assert!(receivers(body).contains(&"c".to_string()));
    assert_eq!(client_app_name(body, "c").as_deref(), Some("app"));
}

#[test]
fn keyword_spacing_and_wrapping_do_not_hide_the_application() {
    for construction in [
        "TestClient(app=other)",
        "TestClient(app = other)",
        "TestClient(\n    app=other,\n)",
        "TestClient(\n    app = other  # the rival\n)",
        "TestClient(other, raise_server_exceptions=False)",
        "AsyncClient(base_url=\"http://test\", app=other)",
        "AsyncClient(transport=ASGITransport(app=other), base_url=\"http://test\")",
    ] {
        let body = format!("client = {construction}\n");
        assert_eq!(
            client_app_name(&body, "client").as_deref(),
            Some("other"),
            "{construction}"
        );
    }
}

#[test]
fn an_unidentifiable_application_is_unknown() {
    for construction in [
        "TestClient(make_app())",
        "TestClient(main.app)",
        "TestClient(app).get(\"/x\")",
        "AsyncClient(base_url=\"https://payments.example\")",
    ] {
        let body = format!("client = {construction}\n");
        assert_eq!(client_app_name(&body, "client"), None, "{construction}");
    }
}

#[test]
fn a_rebinding_of_any_form_withdraws_the_alias() {
    for rebinding in [
        "api = Store()",
        "api: object = Store()",
        "api: object",
        "api += 1",
        "api, other = Store(), 1",
        "other = api = Store()",
        "for api in stores:",
        "for index, api in enumerate(stores):",
        "with Store() as api:",
        "if (api := Store()):",
        "def api(): pass",
        "import api",
    ] {
        let body = format!("api = client\n{rebinding}\n");
        assert!(
            !receivers(&body).contains(&"api".to_string()),
            "{rebinding}"
        );
    }
}

#[test]
fn unrelated_statements_do_not_count_as_rebinding() {
    for statement in [
        "other = api",
        "items[api] = 1",
        "holder.api = 1",
        "if api == other:",
        "result = f(api=client)",
        "assert api.get(\"/items\")",
    ] {
        let body = format!("api = client\n{statement}\n");
        assert!(receivers(&body).contains(&"api".to_string()), "{statement}");
    }
}

#[test]
fn a_with_bound_construction_is_a_receiver() {
    let body = "with TestClient(app) as http:\n    pass\n";
    assert!(receivers(body).contains(&"http".to_string()));
    assert_eq!(client_app_name(body, "http").as_deref(), Some("app"));
}

#[test]
fn a_wrapped_keyword_argument_line_is_not_an_assignment() {
    let body = "result = call(\n    session=client\n)\n";
    assert!(!receivers(body).contains(&"session".to_string()));
}

#[test]
fn a_docstring_with_an_open_bracket_does_not_hide_later_bindings() {
    let body = "\"\"\"\nSmoke (see issue\n\"\"\"\napi = client\nother = TestClient(rival)\n";
    assert!(receivers(body).contains(&"api".to_string()));
    assert!(receivers(body).contains(&"other".to_string()));
    let body = "\"\"\"\nopen (\n\"\"\"\nclient = TestClient(rival)\n";
    assert_eq!(client_app_name(body, "client").as_deref(), Some("rival"));
}

#[test]
fn strings_and_comments_are_not_code() {
    for statement in [
        "msg = 'rest as api'",
        "assert 'behaves as api'",
        "label = f'{api:=^10}'",
        "if (myapi := make()):",
        "def helper(): return api.get('/x')",
        "f = lambda api=1: api",
        "text = \"api = Store()\"",
        "other = 1  # api = Store()",
    ] {
        let body = format!("api = client\n{statement}\n");
        assert!(receivers(&body).contains(&"api".to_string()), "{statement}");
    }
}

#[test]
fn a_one_line_compound_rebinding_withdraws_the_alias() {
    for rebinding in [
        "if flag: api = requests.Session()",
        "try: api = other()",
        "else: api = other()",
        "while go: api = other()",
        "async def api(): pass",
        "def helper(api): pass",
        "api = client; api = other()",
    ] {
        let body = format!("api = client\n{rebinding}\n");
        assert!(
            !receivers(&body).contains(&"api".to_string()),
            "{rebinding}"
        );
    }
}

#[test]
fn a_rebound_client_is_not_identified_by_one_branch() {
    let body = "client = TestClient(rival)\nif flag: client = TestClient(app)\n";
    assert_eq!(client_app_name(body, "client"), None);
}

#[test]
fn backslash_continuations_and_semicolons_read_like_plain_statements() {
    let body = "api = \\\n    TestClient(app)\n";
    assert_eq!(client_app_name(body, "api").as_deref(), Some("app"));
    let body = "client = TestClient(app); x = 1\n";
    assert_eq!(client_app_name(body, "client").as_deref(), Some("app"));
}

#[test]
fn a_large_body_resolves_without_quadratic_rescans() {
    let mut body = String::from("api = TestClient(app)\n");
    for index in 0..3000 {
        body.push_str(&format!("v{index} = f({index})\n"));
    }
    assert!(receivers(&body).contains(&"api".to_string()));
    assert_eq!(client_app_name(&body, "api").as_deref(), Some("app"));
}

#[test]
fn names_that_start_or_end_with_lambda_are_ordinary_names() {
    let body = "lambda_client = client\nother = lambda_client\n";
    assert!(receivers(body).contains(&"lambda_client".to_string()));
    let body = "api = client\nlambda_x = Store()\nmy_lambda = 1\n";
    assert!(receivers(body).contains(&"api".to_string()));
    let body = "api = client\nlambda_api = client\napi = Store()\n";
    assert!(!receivers(body).contains(&"api".to_string()));
    // A real lambda parameter default is still not an assignment.
    let body = "api = client\nf = lambda api=1: api\n";
    assert!(receivers(body).contains(&"api".to_string()));
}
