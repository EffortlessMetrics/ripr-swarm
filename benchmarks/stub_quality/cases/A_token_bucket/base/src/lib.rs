//! Simple token-bucket rate limiter.
pub struct TokenBucket {
    capacity: u32,
    tokens: u32,
}

impl TokenBucket {
    pub fn new(capacity: u32) -> Self {
        Self { capacity, tokens: capacity }
    }

    /// Try to consume `n` tokens.
    pub fn allow(&mut self, n: u32) -> bool {
        if self.tokens >= n {
            self.tokens -= n;
            true
        } else {
            false
        }
    }

    pub fn refill(&mut self, n: u32) {
        self.tokens = (self.tokens + n).min(self.capacity);
    }

    pub fn available(&self) -> u32 {
        self.tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumes_tokens() {
        let mut b = TokenBucket::new(5);
        assert!(b.allow(2));
        assert!(b.allow(2));
        assert!(!b.allow(4));
        assert_eq!(b.available(), 1);
    }

    #[test]
    fn refill_is_capped() {
        let mut b = TokenBucket::new(5);
        b.allow(1);
        b.refill(10);
        assert_eq!(b.available(), 5);
    }
}
