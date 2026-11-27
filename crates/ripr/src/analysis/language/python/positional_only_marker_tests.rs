//! Tests for the positional-only marker gate (#16011): a `def` header edit
//! that only moves `/` is discriminated only by a call that passes a moved
//! parameter by keyword.

use super::owners_tests::{extract_owners, extract_tests};
use super::positional_only_marker::positional_only_marker_params;
use super::*;

const KEY_SOURCE: &str = "def key(self, /, *args, **kwargs):\n    return args\n";
const OLD_HEADER: &str = "def key(self, *args, **kwargs):";
const NEW_HEADER: &str = "def key(self, /, *args, **kwargs):";

fn classify_key(test_source: &str) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/keys.py"), KEY_SOURCE);
    let tests = extract_tests(Path::new("tests/test_keys.py"), test_source);
    classify_change_with_old(
        Path::new("src/keys.py"),
        1,
        NEW_HEADER,
        Some(OLD_HEADER),
        &owners,
        &tests,
    )
    .ok_or_else(|| "a header change should classify".to_string())
}

#[test]
fn positional_calls_do_not_expose_a_marker_only_edit() -> Result<(), String> {
    let finding = classify_key(
        "from src.keys import key\n\n\ndef test_key_returns_args():\n    assert key(None, 1, 2) == (1, 2)\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .missing
            .iter()
            .any(|entry| entry.contains("with `self` by keyword")),
        "{:?}",
        finding.missing
    );
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value == "call `key` with `self` by keyword"),
        "{:?}",
        finding.activation.missing_discriminators
    );
    Ok(())
}

#[test]
fn a_keyword_call_on_a_moved_parameter_keeps_exposed() -> Result<(), String> {
    let finding = classify_key(
        "from src.keys import key\n\n\ndef test_key_refuses_self_by_keyword():\n    assert key(self=None) == ()\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_aliased_keyword_call_keeps_exposed() -> Result<(), String> {
    let finding = classify_key(
        "from src.keys import key as lookup\n\n\ndef test_lookup_by_keyword():\n    assert lookup(self=None) == ()\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    let positional = classify_key(
        "from src.keys import key as lookup\n\n\ndef test_lookup_by_position():\n    assert lookup(None, 1) == (1,)\n",
    )?;
    assert_eq!(positional.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn marker_only_edits_name_the_moved_parameters() {
    assert_eq!(
        positional_only_marker_params(
            "def key(self, *args, **kwargs):",
            "def key(self, /, *args, **kwargs):"
        ),
        Some(vec!["self".to_string()])
    );
    assert_eq!(
        positional_only_marker_params("def f(a, b, /, c):", "def f(a, /, b, c):"),
        Some(vec!["b".to_string()])
    );
    // Removing the marker moves the same parameters back.
    assert_eq!(
        positional_only_marker_params("def f(a, b, /):", "def f(a, b):"),
        Some(vec!["a".to_string(), "b".to_string()])
    );
    // Anything beyond the marker is another change.
    for (old, new) in [
        ("def f(a, b):", "def f(a, b):"),
        ("def f(a, b):", "def f(a, /, b=1):"),
        ("def f(a, b):", "def g(a, /, b):"),
        ("def f(a, b):", "def f(a, /, c):"),
        ("def f(a, *args):", "def f(a, /, *rest):"),
        ("def f(a, b):", "async def f(a, /, b):"),
        ("def f(a, b):", "def f(a, *, b):"),
        ("    return a", "    return b"),
    ] {
        assert_eq!(
            positional_only_marker_params(old, new),
            None,
            "{old} -> {new}"
        );
    }
}
