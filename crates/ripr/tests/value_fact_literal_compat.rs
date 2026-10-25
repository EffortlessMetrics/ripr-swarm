//! External-crate source compatibility: this unchanged four-field literal
//! must compile. Serde omission alone cannot satisfy this contract.
use ripr::domain::{ValueContext, ValueFact};

#[test]
fn original_four_field_literal_remains_valid_and_keeps_its_wire_shape() -> Result<(), String> {
    let fact = ValueFact {
        line: 7,
        text: "assert_eq!(price(10), 1)".to_string(),
        value: "10".to_string(),
        context: ValueContext::FunctionArgument,
    };
    let value = serde_json::to_value(&fact).map_err(|e| e.to_string())?;
    let fields = value
        .as_object()
        .ok_or("value fact must remain an object")?;
    assert_eq!(fields.len(), 4);
    assert_eq!(value["line"], 7);
    assert_eq!(value["text"], "assert_eq!(price(10), 1)");
    assert_eq!(value["value"], "10");
    assert_eq!(value["context"], "function_argument");
    assert_eq!(
        serde_json::from_value::<ValueFact>(value).map_err(|e| e.to_string())?,
        fact
    );
    Ok(())
}
