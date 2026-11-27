#[path = "../shared/ext.rs"]
mod ext;
use self::ext::Ext;
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
        assert_eq!(Guard(Inner { cap: 3 }).limit(), 7);
    }
}
