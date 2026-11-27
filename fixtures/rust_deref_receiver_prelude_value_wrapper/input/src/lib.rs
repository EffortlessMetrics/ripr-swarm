use std::ops::Deref;

pub struct Inner {
    pub cap: u32,
}

impl Inner {
    pub fn is_some(&self) -> bool {
        false
    }
}

pub struct Some(pub Inner);

impl Deref for Some {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn prelude_some_is_some() {
        assert!(Some(crate::Inner { cap: 1 }).is_some());
    }
}
