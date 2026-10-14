pub fn parse_port(input: &str) -> Result<u16, &'static str> {
    match input.parse::<u16>() {
        Ok(p) if p > 0 => Ok(p),
        Ok(_) => Err("zero port"),
        Err(_) => Err("invalid port"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_port() {
        assert_eq!(parse_port("8080").unwrap(), 8080);
    }
}
