#[derive(Debug)]
pub enum ParseSummaryError {
    MalformedSource,
}

pub fn try_parse_summary(raw: &str) -> Result<usize, ParseSummaryError> {
    if raw.is_empty() {
        return Err(ParseSummaryError::MalformedSource);
    }
    Ok(raw.len())
}

pub fn parse_summary(raw: &str) -> Result<usize, Box<dyn std::error::Error>> {
    try_parse_summary(raw).map_err(|error| error.to_string().into())
}
