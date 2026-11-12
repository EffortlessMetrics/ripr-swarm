//! Deterministic wire-order controls for the external-delete lifecycle test.
use super::await_publish_and_response;
use serde_json::{Value, json};
use std::path::Path;

fn publication(uri: &str, diagnostics: Value) -> Value {
    json!({"method": "textDocument/publishDiagnostics", "params": {
        "uri": uri, "diagnostics": diagnostics
    }})
}

fn observe(messages: Vec<Value>, pending: Option<u64>, nonempty: bool) -> Result<usize, String> {
    let mut messages = messages.into_iter();
    await_publish_and_response(
        || {
            messages
                .next()
                .ok_or_else(|| "end of controlled stream".to_owned())
        },
        Path::new("src/lib.rs"),
        pending,
        |count| (count > 0) == nonempty,
    )?;
    Ok(messages.count())
}

#[test]
fn refresh_consumes_response_and_publication_in_both_orders() -> Result<(), String> {
    let diagnostic = publication("file:///repo/src/lib.rs", json!([{}]));
    let response = json!({"id": 2, "result": null});
    for messages in [
        vec![diagnostic.clone(), response.clone()],
        vec![response, diagnostic],
    ] {
        assert_eq!(observe(messages, Some(2), true)?, 0);
    }
    Ok(())
}

#[test]
fn refresh_requires_both_obligations() {
    for messages in [
        vec![publication("file:///repo/src/lib.rs", json!([{}]))],
        vec![json!({"id": 2, "result": null})],
    ] {
        assert!(observe(messages, Some(2), true).is_err());
    }
}

#[test]
fn refresh_rejects_error_wrong_id_and_duplicate_response() {
    let valid = json!({"id": 2, "result": null});
    let published = publication("file:///repo/src/lib.rs", json!([{}]));
    for (response, reason) in [
        (
            json!({"id": 2, "error": {"code": -32603}}),
            "expected a result",
        ),
        (json!({"id": 99, "result": null}), "unexpected response"),
    ] {
        let result = observe(
            vec![response, valid.clone(), published.clone()],
            Some(2),
            true,
        );
        assert!(matches!(result, Err(message) if message.contains(reason)));
    }
    let result = observe(vec![valid.clone(), valid, published], Some(2), true);
    assert!(matches!(result, Err(message) if message.contains("unexpected response")));
}

#[test]
fn deletion_requires_real_empty_diagnostics_for_the_same_file() -> Result<(), String> {
    for message in [
        publication("file:///repo/src/lib.rs", json!([{}])),
        publication("file:///repo/src/other.rs", json!([])),
        publication("file:///repo/src/lib.rs", Value::Null),
        json!({"id": 2, "result": null}),
    ] {
        assert!(observe(vec![message], None, false).is_err());
    }
    assert_eq!(
        observe(
            vec![publication("file:///repo/src/lib.rs", json!([]))],
            None,
            false
        )?,
        0
    );
    Ok(())
}
