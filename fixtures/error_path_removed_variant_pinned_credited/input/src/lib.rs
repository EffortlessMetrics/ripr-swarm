#[derive(Debug, PartialEq)]
pub enum UnitError {
    Blank,
    Unknown,
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
    fn unknown_unit_is_rejected() {
        assert_eq!(parse_unit("q"), Err(UnitError::Unknown));
    }
}
