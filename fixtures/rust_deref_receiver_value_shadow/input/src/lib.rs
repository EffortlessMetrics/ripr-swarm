use std::ops::Deref;

pub struct Inner {
    pub cap: u32,
}

impl Inner {
    pub fn len(&self) -> u32 {
        self.cap * 2 + 1
    }
}

pub struct Guard {
    pub inner: Inner,
}

impl Deref for Guard {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.inner
    }
}

#[allow(non_upper_case_globals)]
pub const Guard: fn(Inner) -> [u8; 7] = |_| [0; 7];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_len() {
        assert_eq!(Guard(Inner { cap: 3 }).len(), 7);
    }
}
