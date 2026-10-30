//! Validator-local JSON values retain source number text without changing
//! serde_json's workspace-wide representation.
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    pub(crate) fn get(&self, key: &str) -> Option<&Self> {
        self.as_object()?.get(key)
    }
    pub(crate) fn as_object(&self) -> Option<&BTreeMap<String, Self>> {
        if let Self::Object(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub(crate) fn as_array(&self) -> Option<&Vec<Self>> {
        if let Self::Array(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub(crate) fn as_str(&self) -> Option<&str> {
        if let Self::String(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub(crate) fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    pub(crate) fn as_number(&self) -> Option<&str> {
        if let Self::Number(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub(crate) fn as_u64(&self) -> Option<u64> {
        self.as_number()?.parse().ok()
    }
    pub(crate) fn is_object(&self) -> bool {
        self.as_object().is_some()
    }
    pub(crate) fn is_array(&self) -> bool {
        self.as_array().is_some()
    }
    pub(crate) fn is_string(&self) -> bool {
        self.as_str().is_some()
    }
    pub(crate) fn is_number(&self) -> bool {
        self.as_number().is_some()
    }
    pub(crate) fn is_boolean(&self) -> bool {
        self.as_bool().is_some()
    }
    pub(crate) fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
    pub(crate) fn pointer(&self, pointer: &str) -> Option<&Self> {
        if pointer.is_empty() {
            return Some(self);
        }
        let mut node = self;
        for part in pointer.strip_prefix('/')?.split('/') {
            let key = part.replace("~1", "/").replace("~0", "~");
            node = match node {
                Self::Object(object) => object.get(&key)?,
                Self::Array(array)
                    if !key.starts_with('+') && (key == "0" || !key.starts_with('0')) =>
                {
                    array.get(key.parse::<usize>().ok()?)?
                }
                _ => return None,
            };
        }
        Some(node)
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::Number(v) => write!(f, "{v}"),
            Self::String(v) => write!(
                f,
                "{}",
                serde_json::to_string(v).map_err(|_| std::fmt::Error)?
            ),
            Self::Array(v) => {
                write!(f, "[")?;
                for (i, item) in v.iter().enumerate() {
                    if i != 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Self::Object(v) => {
                write!(f, "{{")?;
                for (i, (key, item)) in v.iter().enumerate() {
                    if i != 0 {
                        write!(f, ",")?;
                    }
                    write!(
                        f,
                        "{}:{item}",
                        serde_json::to_string(key).map_err(|_| std::fmt::Error)?
                    )?;
                }
                write!(f, "}}")
            }
        }
    }
}

// Apply serde_json's established recursion and finite-number acceptance limits
// before the exact parser allocates a second tree. Both consume the same bytes.
pub(crate) fn parse(bytes: &[u8]) -> Result<Value, String> {
    const MAX_BYTES: usize = 16 * 1024 * 1024;
    if bytes.len() > MAX_BYTES {
        return Err("JSON input exceeds 16 MiB".into());
    }
    let _: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let source = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let parsed: json_syntax::Value = source
        .parse()
        .map_err(|e: json_syntax::parse::Error| e.to_string())?;
    Ok(convert(parsed))
}

fn convert(value: json_syntax::Value) -> Value {
    match value {
        json_syntax::Value::Null => Value::Null,
        json_syntax::Value::Boolean(v) => Value::Bool(v),
        json_syntax::Value::Number(v) => Value::Number(v.to_string()),
        json_syntax::Value::String(v) => Value::String(v.to_string()),
        json_syntax::Value::Array(v) => Value::Array(v.into_iter().map(convert).collect()),
        json_syntax::Value::Object(v) => Value::Object(
            v.into_iter()
                .map(|entry| (entry.key.to_string(), convert(entry.value)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_parser_preserves_numbers_and_objects() -> Result<(), String> {
        let value = parse(br#"{"$serde_json::private::Number":"42","n":1.00000000000000001}"#)?;
        assert_eq!(
            value
                .get("$serde_json::private::Number")
                .and_then(Value::as_str),
            Some("42")
        );
        assert_eq!(
            value.get("n").and_then(Value::as_number),
            Some("1.00000000000000001")
        );
        Ok(())
    }

    #[test]
    fn exact_parser_matches_duplicate_and_pointer_rules() -> Result<(), String> {
        let value = parse(br#"{"a/b":{"~x":[1.5,1]},"n":1.5,"n":1}"#)?;
        assert_eq!(
            value.pointer("/a~1b/~0x/0").and_then(Value::as_number),
            Some("1.5")
        );
        assert!(value.pointer("/a~1b/~0x/00").is_none());
        assert!(value.pointer("/a~1b/~0x/+1").is_none());
        assert_eq!(value.get("n").and_then(Value::as_number), Some("1"));
        let reverse = parse(br#"{"n":1,"n":1.5}"#)?;
        assert_eq!(reverse.get("n").and_then(Value::as_number), Some("1.5"));
        Ok(())
    }

    #[test]
    fn duplicate_escaped_keys_keep_last_and_invalid_discarded_values_fail() -> Result<(), String> {
        let value = parse(br#"{"a":1,"\u0061":1.5}"#)?;
        assert_eq!(value.get("a").and_then(Value::as_number), Some("1.5"));
        assert!(parse(br#"{"a":"\uD800","a":1}"#).is_err());
        Ok(())
    }

    #[test]
    fn parser_preflight_rejects_unsupported_input() {
        assert!(parse(b"1e400").is_err());
        assert!(parse(b"1 trailing").is_err());
        assert!(parse(&vec![b' '; 16 * 1024 * 1024 + 1]).is_err());
        let deep = format!("{}0{}", "[".repeat(129), "]".repeat(129));
        assert!(parse(deep.as_bytes()).is_err());
    }
}
