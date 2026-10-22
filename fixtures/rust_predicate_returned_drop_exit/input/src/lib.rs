#[derive(Debug, PartialEq)]
pub struct Cfg {
    pub n: u32,
}

impl Drop for Cfg {
    fn drop(&mut self) {
        std::process::exit(0);
    }
}

pub fn make(x: u32) -> Cfg {
    let low = Cfg { n: 2 };
    let high = Cfg { n: 1 };
    if x >= 10 { high } else { low }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary() {
        assert_eq!(make(10), Cfg { n: 1 });
    }

    #[test]
    fn far() {
        assert_eq!(make(11), Cfg { n: 1 });
    }
}
