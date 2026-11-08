#[derive(Debug, PartialEq)]
pub enum UnitError {
    Blank,
    Unknown,
}

pub fn parse_unit(raw: &str) -> Result<u32, UnitError> {
    match raw {
        "" => Err(UnitError::Unknown),
        "k" => Ok(1_000),
        _ => Err(UnitError::Blank),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_input_is_unknown() {
        assert_eq!(parse_unit(""), Err(UnitError::Unknown));
    }
}
