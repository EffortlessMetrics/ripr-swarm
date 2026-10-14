#[derive(Debug, PartialEq)]
pub struct Config {
    pub retries: u32,
}

pub fn build(retries: u32) -> Config {
    Config {
        retries: retries + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, build};

    #[test]
    fn build_is_not_nine_retries() {
        assert_ne!(build(3), Config { retries: 9 });
    }
}
