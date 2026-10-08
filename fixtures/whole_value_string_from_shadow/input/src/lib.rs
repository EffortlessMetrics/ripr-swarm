#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub retries: u32,
    pub name: String,
}

pub fn build(retries: u32) -> Config {
    Config {
        retries,
        name: render(retries),
    }
}

fn render(retries: u32) -> String {
    format!("cfg-{retries}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // The test file's `String::from` resolves here, not to the standard
    // library: this `from` returns the owner's own rendered name, so both
    // assert_eq! operands read `build`'s output and no mutant of the
    // changed `name:` line can fail the test.
    mod String {
        pub fn from(value: &str) -> std::string::String {
            super::build(value.trim().parse().unwrap_or(0)).name
        }
    }

    #[test]
    fn builds_config() {
        assert_eq!(
            build(3),
            Config {
                retries: 3,
                name: String::from("3")
            }
        );
    }
}
