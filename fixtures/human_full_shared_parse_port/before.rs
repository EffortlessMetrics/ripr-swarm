pub fn parse_port(input: &str) -> Result<u16, &'static str> {
    match input.parse::<u16>() {
        Ok(p) => Ok(p),
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
