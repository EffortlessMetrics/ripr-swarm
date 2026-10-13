//! Repo-exposure honesty subject (#5335, evidence-promotion corpus).
//!
//! One conversion helper is never called by any test; the inline tests
//! exercise only its neighbouring sibling. Before the shared reach-bearing
//! relation rule, repo mode credited the same-file tests' exact-value
//! oracles to the uncalled helper and read `strongly_gripped`. The pinned
//! corpus report must keep the uncalled helper ungripped with `reach: no`,
//! and the called sibling strongly gripped.
//!
//! Deliberately no doc example and no other mention of the helper names:
//! the unseen-name scan counts comments and strings, and naming a helper
//! here would honestly keep its reach `weak`.
pub struct Size(pub u64);

impl Size {
    pub fn as_kb(&self) -> f64 {
        self.0 as f64 / 1000.0
    }

    pub fn as_kib(&self) -> f64 {
        self.0 as f64 / 1024.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert_eq!(Size(2048).as_kib(), 2.0);
    }

    #[test]
    fn zero_size() {
        assert_eq!(Size(0).0, 0);
    }
}
