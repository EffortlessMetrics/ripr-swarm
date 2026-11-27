use std::ops::Deref;

pub struct Inner {
    pub cap: u32,
}

impl Inner {
    pub fn limit(&self) -> u32 {
        self.cap * 2 + 1
    }
}

pub struct Guard(pub Inner);

impl Guard {
    pub fn limit(&self) -> u32 {
        100
    }
}

impl Deref for Guard {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_limit() {
        let g = Guard(Inner { cap: 3 });
        assert_eq!(g.limit(), 100);
    }
}
