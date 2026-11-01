#[derive(Debug, PartialEq)]
pub enum UnitError {
    Blank,
    Unknown,
    Other,
}

pub fn parse_unit(raw: &str) -> Result<u32, UnitError> {
    match raw {
        "" => Err(UnitError::Blank),
        "k" => Ok(1_000),
        _ => Err(UnitError::Blank),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn other_error_is_expected() {
        assert_eq!(parse_unit("q"), Err(UnitError::Other), "expected UnitError::Unknown");
    }
}
