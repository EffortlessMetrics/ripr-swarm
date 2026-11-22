#[derive(Debug, PartialEq)]
pub enum Level {
    High,
    Low,
}

pub fn level(n: u32) -> Level {
    if n >= 10 { Level::High } else { Level::Low }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifty_is_high() {
        assert_eq!(level(50), Level::High);
    }
}
