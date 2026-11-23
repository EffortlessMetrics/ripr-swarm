mod guard;

pub use guard::Cfg as Config;

pub fn make(x: u32) -> Config {
    Config { n: x * 3 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes() {
        assert_eq!(make(4), Config { n: 12 });
    }
}
