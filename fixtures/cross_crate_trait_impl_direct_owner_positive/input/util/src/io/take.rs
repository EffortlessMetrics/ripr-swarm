pub struct Take {
    limit: u64,
}

impl Take {
    /// ```
    /// let t = rutil::io::take::Take::new(4);
    /// assert_eq!(t.limit(), 4);
    /// ```
    pub fn limit(&self) -> u64 {
        self.limit
    }

    pub fn new(limit: u64) -> Self {
        Take { limit }
    }
}
