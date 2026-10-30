pub struct Guard {
    pub n: u32,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.n = self.n.wrapping_add(1);
    }
}

pub fn compute(x: u32) -> u32 {
    let _guard = Guard { n: 0 };
    x * 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes() {
        assert_eq!(compute(4), 12);
    }
}
