//! Validator-local JSON values retain source number text without changing
//! serde_json's workspace-wide representation.
use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use std::collections::BTreeMap;
use std::fmt;

pub(crate) const MAX_JSON_NODES: usize = 100_000;

struct CountSeed<'a>(&'a mut usize);
struct CountVisitor<'a>(&'a mut usize);

impl<'de> DeserializeSeed<'de> for CountSeed<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        if *self.0 == 0 {
            return Err(D::Error::custom("JSON input exceeds node limit"));
        }
        *self.0 -= 1;
        deserializer.deserialize_any(CountVisitor(self.0))
    }
}

impl<'de> Visitor<'de> for CountVisitor<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while seq.next_element_seed(CountSeed(self.0))?.is_some() {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while map.next_key::<String>()?.is_some() {
            map.next_value_seed(CountSeed(self.0))?;
        }
        Ok(())
    }
}

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
                serde_json::to_string(v).map_err(|_error| std::fmt::Error)?
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
                        serde_json::to_string(key).map_err(|_error| std::fmt::Error)?
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
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let mut remaining = MAX_JSON_NODES;
    CountSeed(&mut remaining)
        .deserialize(&mut decoder)
        .map_err(|e| e.to_string())?;
    decoder.end().map_err(|e| e.to_string())?;
    let source = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let mut scan_budget = 64 * 1024 * 1024;
    convert(source.trim(), &mut scan_budget)
}

// StreamDeserializer exposes the end of a successfully consumed value without
// requiring serde_json/raw_value on a dependency shared with the product.
// IgnoredAny identifies only the source boundary; CountSeed has already checked
// the entire document, including discarded duplicate values and finite numbers.
fn take_raw<'a>(input: &mut &'a str) -> Result<&'a str, String> {
    let text = input.trim_start();
    let mut stream = serde_json::Deserializer::from_str(text).into_iter::<serde::de::IgnoredAny>();
    stream
        .next()
        .ok_or("expected JSON value")?
        .map_err(|e| e.to_string())?;
    let end = stream.byte_offset();
    let value = text.get(..end).ok_or("invalid JSON value boundary")?;
    *input = text.get(end..).ok_or("invalid JSON value boundary")?;
    Ok(value)
}

fn take_separator(input: &mut &str, separator: char) -> Result<(), String> {
    *input = input
        .trim_start()
        .strip_prefix(separator)
        .ok_or_else(|| format!("expected JSON separator {separator}"))?;
    Ok(())
}

// Each container scans its children to locate their original byte slices. As
// with RawValue, repeated subtree scans are capped by a cumulative byte budget.
// This is only called after the complete document passes the serde preflight.
fn convert(text: &str, scan_budget: &mut usize) -> Result<Value, String> {
    *scan_budget = scan_budget
        .checked_sub(text.len())
        .ok_or("JSON scan work limit exceeded")?;
    match text.as_bytes().first() {
        Some(b'n') => Ok(Value::Null),
        Some(b't') => Ok(Value::Bool(true)),
        Some(b'f') => Ok(Value::Bool(false)),
        Some(b'"') => serde_json::from_str(text)
            .map(Value::String)
            .map_err(|e| e.to_string()),
        Some(b'[') => {
            let mut rest = text
                .strip_prefix('[')
                .and_then(|text| text.strip_suffix(']'))
                .ok_or("expected JSON array")?;
            let mut children = Vec::new();
            while !rest.trim_start().is_empty() {
                children.push(convert(take_raw(&mut rest)?, scan_budget)?);
                if !rest.trim_start().is_empty() {
                    take_separator(&mut rest, ',')?;
                }
            }
            Ok(Value::Array(children))
        }
        Some(b'{') => {
            let mut rest = text
                .strip_prefix('{')
                .and_then(|text| text.strip_suffix('}'))
                .ok_or("expected JSON object")?;
            let mut children = BTreeMap::new();
            while !rest.trim_start().is_empty() {
                let key: String =
                    serde_json::from_str(take_raw(&mut rest)?).map_err(|e| e.to_string())?;
                take_separator(&mut rest, ':')?;
                children.insert(key, take_raw(&mut rest)?);
                if !rest.trim_start().is_empty() {
                    take_separator(&mut rest, ',')?;
                }
            }
            children
                .into_iter()
                .map(|(key, value)| Ok((key, convert(value, scan_budget)?)))
                .collect::<Result<BTreeMap<_, _>, String>>()
                .map(Value::Object)
        }
        Some(b'-' | b'0'..=b'9') => Ok(Value::Number(text.to_owned())),
        _ => Err("unexpected JSON value".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expect_parse(bytes: &[u8], valid: bool) -> Result<(), String> {
        match (parse(bytes), valid) {
            (Ok(_), true) | (Err(_), false) => Ok(()),
            (Err(error), true) => Err(format!("expected valid input: {error}")),
            (Ok(_), false) => Err("expected invalid input to be rejected".into()),
        }
    }

    #[test]
    fn parse_enforces_scan_budget_on_nested_input() -> Result<(), String> {
        let payload = "x".repeat(1024 * 1024);
        let shallow = format!("[\"{payload}\"]");
        parse(shallow.as_bytes())?;
        let nested = format!("{}\"{payload}\"{}", "[".repeat(65), "]".repeat(65));
        match parse(nested.as_bytes()) {
            Err(error) if error.contains("scan work limit") => Ok(()),
            Err(error) => Err(format!("unexpected parse error: {error}")),
            Ok(_) => Err("expected scan work limit error".into()),
        }
    }

    #[test]
    fn raw_subtree_scan_budget_fails_closed() -> Result<(), String> {
        let raw = "[[1]]";
        let mut insufficient = 7;
        match convert(raw, &mut insufficient) {
            Err(error) if error.contains("scan work limit") => {}
            Err(error) => return Err(format!("unexpected conversion error: {error}")),
            Ok(_) => return Err("expected scan work limit error".into()),
        }
        let mut sufficient = 9;
        convert(raw, &mut sufficient)?;
        Ok(())
    }

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
        expect_parse(br#"{"a":"\uD800","a":1}"#, false)?;
        Ok(())
    }

    #[test]
    fn preflight_counts_nodes_before_duplicate_collapse() -> Result<(), String> {
        let at_limit = format!("[{}0]", "0,".repeat(MAX_JSON_NODES - 2));
        expect_parse(at_limit.as_bytes(), true)?;
        let over_limit = format!("[{}0]", "0,".repeat(MAX_JSON_NODES - 1));
        expect_parse(over_limit.as_bytes(), false)?;
        let duplicates = format!("{{{}\"x\":0}}", "\"x\":0,".repeat(MAX_JSON_NODES - 1));
        expect_parse(duplicates.as_bytes(), false)?;
        Ok(())
    }

    #[test]
    fn parser_preflight_rejects_unsupported_input() -> Result<(), String> {
        expect_parse(b"1e400", false)?;
        expect_parse(br#""\uD800""#, false)?;
        expect_parse(br#""\uD83D\uDE00""#, true)?;
        expect_parse(b"1 trailing", false)?;
        expect_parse(&vec![b' '; 16 * 1024 * 1024 + 1], false)?;
        let within_depth = format!("{}0{}", "[".repeat(127), "]".repeat(127));
        expect_parse(within_depth.as_bytes(), true)?;
        let deep = format!("{}0{}", "[".repeat(129), "]".repeat(129));
        expect_parse(deep.as_bytes(), false)?;
        Ok(())
    }
    #[test]
    fn parser_preserves_lexical_boundaries_and_number_spellings() -> Result<(), String> {
        let value = parse(
            br#"
            { "1": [-0, 0.00, 1E+02, 1e-2, 18446744073709551616, 1.00000000000000001],
              "s": "1, [2], {\"3\": \"4\"}, \uD83D\uDE00",
              "\u0061": {"": null, "t": true, "f": false}, "tail": 42 }
        "#,
        )?;
        let numbers = value
            .get("1")
            .and_then(Value::as_array)
            .ok_or("missing number array")?;
        assert_eq!(
            numbers.iter().map(Value::as_number).collect::<Vec<_>>(),
            vec![
                Some("-0"),
                Some("0.00"),
                Some("1E+02"),
                Some("1e-2"),
                Some("18446744073709551616"),
                Some("1.00000000000000001")
            ]
        );
        assert_eq!(
            value.get("s").and_then(Value::as_str),
            Some("1, [2], {\"3\": \"4\"}, 😀")
        );
        assert_eq!(value.pointer("/a/"), Some(&Value::Null));
        assert_eq!(value.pointer("/a/t").and_then(Value::as_bool), Some(true));
        assert_eq!(value.pointer("/a/f").and_then(Value::as_bool), Some(false));
        assert_eq!(value.get("tail").and_then(Value::as_number), Some("42"));
        for malformed in [
            "[1 2]",
            "{\"a\" 1}",
            "{\"a\":1 \"b\":2}",
            "[1,]",
            "{\"a\":1,}",
            "1true",
            "truefalse",
        ] {
            expect_parse(malformed.as_bytes(), false)?;
        }
        Ok(())
    }
}
