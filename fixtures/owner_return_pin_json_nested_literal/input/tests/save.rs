use demo::{did_save_params, SchemaError};
use serde_json::json;
const METHOD: &str = "textDocument/didSave";
#[test] fn checks() { assert_eq!(did_save_params(METHOD, &json!({"text":null})), Ok(())); }