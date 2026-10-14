use demo::{did_save_params, SchemaError};
use serde_json::json;
const METHOD: &str = "textDocument/didSave";
#[test] fn checks() { let params=json!({"text":null}); assert_eq!(did_save_params(METHOD,&params),Ok(())); }
