use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    MissingSeparator,
    MissingKey,
    EmptyValue,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::MissingSeparator => write!(f, "missing '=' separator"),
            ParseError::MissingKey => write!(f, "missing key"),
            ParseError::EmptyValue => write!(f, "empty value"),
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvPair {
    pub key: String,
    pub value: String,
}

impl FromStr for KvPair {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (k, v) = s.split_once('=').ok_or(ParseError::MissingSeparator)?;
        let key = k.trim();
        if key.is_empty() {
            return Err(ParseError::MissingSeparator);
        }
        let value = v.trim();
        if value.is_empty() {
            return Err(ParseError::EmptyValue);
        }
        Ok(KvPair {
            key: key.to_string(),
            value: value.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pair() {
        let p: KvPair = " name = ripr ".parse().unwrap();
        assert_eq!(p.key, "name");
        assert_eq!(p.value, "ripr");
    }

    #[test]
    fn rejects_bad_input() {
        assert!("novalue".parse::<KvPair>().is_err());
        assert!("=v".parse::<KvPair>().is_err());
        assert!("k=".parse::<KvPair>().is_err());
    }

    #[test]
    fn display_missing_sep() {
        assert_eq!(ParseError::MissingSeparator.to_string(), "missing '=' separator");
    }
}
