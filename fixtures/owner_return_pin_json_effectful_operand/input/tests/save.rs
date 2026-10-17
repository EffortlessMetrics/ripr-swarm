use demo::{did_save_params, SchemaError};
use serde_json::json;
const METHOD: &str = "textDocument/didSave";
#[test] fn checks() { let params = json!({"text": ({ if true { return; } "unreached" })}); assert_eq!(did_save_params(METHOD,&params),Ok(())); }