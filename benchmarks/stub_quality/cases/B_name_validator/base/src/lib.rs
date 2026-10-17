use std::error::Error;
use std::fmt;

pub const MIN_LEN: usize = 3;
pub const MAX_LEN: usize = 16;

#[derive(Debug, PartialEq, Eq)]
pub enum NameError {
    TooShort(usize),
    TooLong(usize),
    BadChar(char),
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NameError::TooShort(n) => write!(f, "name too short ({n})"),
            NameError::TooLong(n) => write!(f, "name too long ({n})"),
            NameError::BadChar(c) => write!(f, "bad character {c:?}"),
        }
    }
}

impl Error for NameError {}

pub fn validate_username(s: &str) -> Result<(), Box<dyn Error>> {
    let n = s.chars().count();
    if n < MIN_LEN {
        return Err(Box::new(NameError::TooShort(n)));
    }
    if n > MAX_LEN {
        return Err(Box::new(NameError::TooLong(n)));
    }
    if let Some(c) = s.chars().find(|c| !(c.is_ascii_alphanumeric() || *c == '_')) {
        return Err(Box::new(NameError::BadChar(c)));
    }
    Ok(())
}

pub fn validate_all(names: &[&str]) -> Result<usize, Box<dyn Error>> {
    for n in names {
        validate_username(n)?;
    }
    Ok(names.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal() {
        assert!(validate_username("alice_01").is_ok());
    }

    #[test]
    fn rejects_short_long_bad() {
        assert!(validate_username("ab").is_err());
        assert!(validate_username(&"x".repeat(20)).is_err());
        assert!(validate_username("bad name").is_err());
    }

    #[test]
    fn all_counts() {
        assert_eq!(validate_all(&["alice", "bobby"]).unwrap(), 2);
        assert!(validate_all(&["alice", "!"]).is_err());
    }
}
