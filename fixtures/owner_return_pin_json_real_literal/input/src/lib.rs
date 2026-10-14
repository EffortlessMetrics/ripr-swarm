use serde_json::Value;
#[derive(Debug, PartialEq)]
pub struct SchemaError { pub method: Option<String>, pub path: String, pub expected: String, pub observed: String }
pub fn did_save_params(_method: &str, _params: &Value) -> Result<(), SchemaError> {
    Ok(())
}
