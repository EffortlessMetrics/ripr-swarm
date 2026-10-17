#[derive(Debug, PartialEq)]
pub struct Cfg {
    pub n: u32,
}

impl Drop for Cfg {
    fn drop(&mut self) {
        // no-op
    }
}

pub fn make(x: u32) -> Cfg {
    Cfg { n: x * 3 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes() {
        assert_eq!(make(4), Cfg { n: 12 });
    }
}
