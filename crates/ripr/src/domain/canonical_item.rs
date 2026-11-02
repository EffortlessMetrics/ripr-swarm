//! Portable canonical repair/gap identity (`CanonicalItemId`, #4805 / ID02).
//!
//! The wire spelling remains the producer-owned `gap:…` burn-down key. This
//! wrapper rejects cross-class aliases and malformed values at parse time.

use std::fmt;

/// Producer-defined semantic repair/gap identity. Not a seam location, probe
/// transport id, or session action id.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct CanonicalItemId(pub String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicalItemParseError {
    Empty,
    WrongClass {
        value: String,
        expected: &'static str,
    },
}

impl fmt::Display for CanonicalItemParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("canonical item id must not be empty"),
            Self::WrongClass { value, expected } => write!(
                f,
                "value `{value}` is not a {expected}; canonical items use `gap:` ids"
            ),
        }
    }
}

impl CanonicalItemId {
    /// Parse the canonical `canonical_gap_id` wire field.
    pub fn from_canonical_gap_id(value: &str) -> Result<Self, CanonicalItemParseError> {
        parse_gap_class(value, "canonical_gap_id")
    }

    /// Parse the governed compatibility alias `gap_id` when it names the same
    /// burn-down key (#4804 removal window).
    pub fn from_gap_id_alias(value: &str) -> Result<Self, CanonicalItemParseError> {
        parse_gap_class(value, "gap_id alias")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn parse_gap_class(
    value: &str,
    label: &'static str,
) -> Result<CanonicalItemId, CanonicalItemParseError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CanonicalItemParseError::Empty);
    }
    if !trimmed.starts_with("gap:") {
        return Err(CanonicalItemParseError::WrongClass {
            value: trimmed.to_string(),
            expected: label,
        });
    }
    if trimmed.starts_with("gap:rust:") || trimmed.starts_with("gap:pr:") {
        return Ok(CanonicalItemId(trimmed.to_string()));
    }
    // Content-hash canonical gaps: `gap:` + 16 lowercase hex digits.
    let digest = trimmed.strip_prefix("gap:").unwrap_or("");
    if digest.len() == 16 && digest.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Ok(CanonicalItemId(trimmed.to_string()));
    }
    Err(CanonicalItemParseError::WrongClass {
        value: trimmed.to_string(),
        expected: label,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_gap_id_round_trips_hash_and_readable_forms() -> Result<(), String> {
        let hash = CanonicalItemId::from_canonical_gap_id("gap:67f9714ba37b3967")
            .map_err(|error| error.to_string())?;
        assert_eq!(hash.as_str(), "gap:67f9714ba37b3967");
        let readable = CanonicalItemId::from_canonical_gap_id(
            "gap:rust:src/lib.rs:demo:return_value:return_value:x",
        )
        .map_err(|error| error.to_string())?;
        assert!(readable.as_str().starts_with("gap:rust:"));
        Ok(())
    }

    #[test]
    fn gap_id_alias_accepts_only_matching_gap_class() -> Result<(), String> {
        let id = CanonicalItemId::from_gap_id_alias("gap:0102030405060708")
            .map_err(|error| error.to_string())?;
        assert_eq!(id.as_str(), "gap:0102030405060708");
        Ok(())
    }

    #[test]
    fn probe_and_action_ids_reject_as_canonical_item() {
        assert!(CanonicalItemId::from_canonical_gap_id("probe:demo").is_err());
        assert!(CanonicalItemId::from_gap_id_alias("action:demo").is_err());
        assert!(CanonicalItemId::from_canonical_gap_id("seam:demo").is_err());
    }

    #[test]
    fn formatting_only_changes_do_not_change_parsed_identity() -> Result<(), String> {
        let first = CanonicalItemId::from_canonical_gap_id("gap:67f9714ba37b3967")
            .map_err(|error| error.to_string())?;
        let second = CanonicalItemId::from_canonical_gap_id(" gap:67f9714ba37b3967 ")
            .map_err(|error| error.to_string())?;
        assert_eq!(first, second);
        Ok(())
    }
}
