struct Parser { n: u64 }

impl Parser {
    fn secs(&self) -> u64 {
        self.n * 1_209_600
    }
    fn total(&self) -> u64 { self.secs() }
}

pub fn entry(n: u64) -> u64 {
    Parser { n }.total()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fortnight_total() {
        assert_eq!(entry(1), 1_209_600);
    }
}