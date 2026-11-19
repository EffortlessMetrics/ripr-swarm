#[derive(Debug, PartialEq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    Number,
    Parts,
    LeadingZero,
}

fn number(s: &str) -> Result<u64, ParseError> {
    if s.len() > 1 && s.starts_with('0') {
        return Err(ParseError::LeadingZero);
    }
    s.parse().map_err(|_| ParseError::Number)
}

pub fn parse(text: &str) -> Result<Version, ParseError> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 3 {
        return Err(ParseError::Parts);
    }
    Ok(Version {
        major: number(parts[0])?,
        minor: number(parts[1])?,
        patch: number(parts[2])?,
    })
}
