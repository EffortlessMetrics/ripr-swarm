//! A function that writes an attribute of a module-scope object, read back by
//! a test that imported that object from the owner's module.

use super::classify::classify_change_with_old;
use super::owners_tests::{extract_owners, extract_tests};
use super::related_tests::apply_class_facts;
use crate::domain::{ExposureClass, Finding};
use std::path::Path;

type Check = Result<(), String>;

const ROUTE: &str = "from types import SimpleNamespace\n\nresponse = SimpleNamespace(status_code=200)\n\n\ndef checkout(expired):\n    response.status_code = 422\n";

fn classify(src: &str, test_body: &str) -> Result<Finding, String> {
    let file = "shop/route.py";
    let mut owners = extract_owners(Path::new(file), src);
    apply_class_facts(&mut owners);
    let owner = owners
        .iter()
        .find(|owner| owner.name == "checkout")
        .ok_or("owner checkout must be extracted")?;
    let tests = extract_tests(
        Path::new("tests/test_route.py"),
        &format!("from shop.route import checkout, response\n\n\ndef test_it():\n{test_body}\n"),
    );
    if tests.len() != 1 {
        return Err("fixture must parse exactly one test".to_string());
    }
    classify_change_with_old(
        Path::new(file),
        owner.start_line + 1,
        "    response.status_code = 400",
        Some("    response.status_code = 422"),
        &owners.clone(),
        &tests,
    )
    .ok_or_else(|| "the change must yield a finding".to_string())
}

fn classify_route(src: &str, test_body: &str) -> Result<ExposureClass, String> {
    classify(&src.replace("= 422", "= 400"), test_body).map(|finding| finding.class)
}

#[test]
fn imported_module_object_read_after_the_call_credits_the_field_write() -> Check {
    let class = classify_route(
        ROUTE,
        "    checkout(True)\n    assert response.status_code == 422",
    )?;
    assert_eq!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn read_before_the_owner_call_is_not_credited() -> Check {
    let class = classify_route(
        ROUTE,
        "    assert response.status_code == 200\n    checkout(True)",
    )?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn guarded_field_write_is_not_credited() -> Check {
    let guarded = ROUTE.replace(
        "    response.status_code = 422\n",
        "    if expired:\n        response.status_code = 422\n",
    );
    let owners = extract_owners(Path::new("shop/route.py"), &guarded);
    let owner = owners
        .iter()
        .find(|owner| owner.name == "checkout")
        .ok_or("owner checkout must be extracted")?;
    let tests = extract_tests(
        Path::new("tests/test_route.py"),
        "from shop.route import checkout, response\n\n\ndef test_it():\n    checkout(False)\n    assert response.status_code == 200\n",
    );
    let finding = classify_change_with_old(
        Path::new("shop/route.py"),
        owner.start_line + 2,
        "        response.status_code = 400",
        Some("        response.status_code = 422"),
        &owners,
        &tests,
    )
    .ok_or("the change must yield a finding")?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn rebound_module_object_is_not_credited() -> Check {
    let rebound = ROUTE.replace(
        "\n\ndef checkout",
        "\nresponse = SimpleNamespace(status_code=500)\n\n\ndef checkout",
    );
    let class = classify_route(
        &rebound,
        "    checkout(True)\n    assert response.status_code == 422",
    )?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn parameter_shadowing_the_module_object_is_not_credited() -> Check {
    let shadowed = ROUTE.replace("def checkout(expired)", "def checkout(response)");
    let class = classify_route(
        &shadowed,
        "    checkout(True)\n    assert response.status_code == 422",
    )?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn local_object_of_the_same_name_is_not_the_imported_one() -> Check {
    let class = classify_route(
        ROUTE,
        "    response = object()\n    checkout(True)\n    assert response.status_code == 422",
    )?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn local_binding_of_the_object_name_is_not_credited() -> Check {
    for local in [
        "response = SimpleNamespace()",
        "for response in [SimpleNamespace()]:\n        pass",
        "with open(\"f\") as response:\n        pass",
    ] {
        let shadowed = ROUTE.replace(
            "    response.status_code = 422\n",
            &format!("    {local}\n    response.status_code = 422\n"),
        );
        let owners = extract_owners(Path::new("shop/route.py"), &shadowed);
        let owner = owners
            .iter()
            .find(|owner| owner.name == "checkout")
            .ok_or("owner checkout must be extracted")?;
        let tests = extract_tests(
            Path::new("tests/test_route.py"),
            "from shop.route import checkout, response\n\n\ndef test_it():\n    checkout(True)\n    assert response.status_code == 422\n",
        );
        let write_line = shadowed
            .lines()
            .position(|line| line == "    response.status_code = 422")
            .ok_or("write line must exist")?
            + 1;
        let finding = classify_change_with_old(
            Path::new("shop/route.py"),
            write_line.max(owner.start_line + 1),
            "    response.status_code = 400",
            Some("    response.status_code = 422"),
            &owners,
            &tests,
        )
        .ok_or("the change must yield a finding")?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{local}");
    }
    Ok(())
}

fn classify_with_test(test_src: &str) -> Result<ExposureClass, String> {
    let file = "shop/route.py";
    let src = ROUTE.replace("= 422", "= 400");
    let mut owners = extract_owners(Path::new(file), &src);
    apply_class_facts(&mut owners);
    let owner = owners
        .iter()
        .find(|owner| owner.name == "checkout")
        .ok_or("owner checkout must be extracted")?;
    let tests = extract_tests(Path::new("tests/test_route.py"), test_src);
    classify_change_with_old(
        Path::new(file),
        owner.start_line + 1,
        "    response.status_code = 400",
        Some("    response.status_code = 422"),
        &owners,
        &tests,
    )
    .map(|finding| finding.class)
    .ok_or_else(|| "the change must yield a finding".to_string())
}

#[test]
fn patched_module_object_is_not_credited() -> Check {
    for test_src in [
        "from shop import route\nfrom shop.route import checkout, response\n\n\ndef test_it(monkeypatch):\n    monkeypatch.setattr(route, \"response\", object())\n    checkout(True)\n    assert response.status_code == 422\n",
        "from unittest.mock import patch\nfrom shop.route import checkout, response\n\n\n@patch(\"shop.route.response\")\ndef test_it(fake):\n    checkout(True)\n    assert response.status_code == 422\n",
        "from unittest.mock import patch\nfrom shop.route import checkout, response\n\n\ndef test_it():\n    with patch(\"shop.route.response\"):\n        checkout(True)\n    assert response.status_code == 422\n",
        "from shop import route\nfrom shop.route import checkout, response\n\n\ndef test_it():\n    route.response = object()\n    checkout(True)\n    assert response.status_code == 422\n",
        "import importlib\nfrom shop import route\nfrom shop.route import checkout, response\n\n\ndef test_it():\n    importlib.reload(route)\n    checkout(True)\n    assert response.status_code == 422\n",
        "from shop import route\nfrom shop.route import checkout, response\n\n\ndef test_it():\n    route.__setattr__(\"response\", object())\n    checkout(True)\n    assert response.status_code == 422\n",
        "from shop import route\nfrom shop.route import checkout, response\n\n\ndef test_it(mp):\n    mp.delattr(route, \"response\")\n    checkout(True)\n    assert response.status_code == 422\n",
        "from shop import route\nfrom shop.route import checkout, response\n\n\ndef test_it():\n    setattr(route, \"response\", object())\n    checkout(True)\n    assert response.status_code == 422\n",
    ] {
        let class = classify_with_test(test_src)?;
        assert_ne!(class, ExposureClass::Exposed, "{test_src}");
    }
    Ok(())
}
