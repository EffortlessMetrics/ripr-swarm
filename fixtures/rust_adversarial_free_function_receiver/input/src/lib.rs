pub struct Config {
    pub strict: bool,
}

impl Config {
    pub fn parse(&self, input: &str) -> (usize, Option<usize>) {
        if self.strict {
            (0, Some(input.len()))
        } else {
            (0, None)
        }
    }
}

pub fn parse(input: &str) -> (usize, Option<usize>) {
    (0, Some(input.len()))
}
