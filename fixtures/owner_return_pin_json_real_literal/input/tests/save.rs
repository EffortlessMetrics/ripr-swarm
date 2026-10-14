use demo::{did_save_params, SchemaError};
use serde_json::json;
const METHOD: &str = "textDocument/didSave";
#[test]
fn save_success_is_observed_with_absent_empty_and_nonempty_text() {
    let params = json!({"textDocument": {"uri": "untitled:save"}});
    assert_eq!(did_save_params(METHOD, &params), Ok(()));
    let mut with_text = params;
    with_text["text"] = json!("");
    assert_eq!(did_save_params(METHOD, &with_text), Ok(()));
    with_text["text"] = json!("my $saved = 1;\n");
    assert_eq!(did_save_params(METHOD, &with_text), Ok(()));
}
