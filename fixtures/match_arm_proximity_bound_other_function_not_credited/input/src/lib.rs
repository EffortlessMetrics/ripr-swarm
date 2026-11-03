use std::time::Duration;

#[derive(Debug, PartialEq)]
pub enum Reduction {
    Failure,
    Success,
    Over(Duration),
}

pub fn reduce(tokens: u64, budget: u64) -> Reduction {
    if tokens <= budget {
        Reduction::Success
    } else if tokens > budget * 2 {
        Reduction::Failure
    } else {
        Reduction::Over(Duration::from_millis(tokens - budget))
    }
}

pub struct Limiter {
    pub budget: u64,
}

impl Limiter {
    pub fn consume(&mut self, tokens: u64) -> bool {
        match reduce(tokens, self.budget) {
            Reduction::Failure => false,
            Reduction::Success => true,
            Reduction::Over(wait) => wait.as_millis() < 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consume_under_budget() {
        let mut limiter = Limiter { budget: 10 };
        assert!(limiter.consume(4));
        assert_eq!(limiter.budget, 10);
    }

    #[test]
    fn reduce_over_budget() {
        let tokens: u64 = "15".parse().unwrap();
        let reduced = reduce(tokens, 10);
        assert_eq!(reduced, Reduction::Over(Duration::from_millis(5)));
    }
}
