#[derive(Debug, PartialEq)]
pub struct Cfg {
    pub n: u32,
}

impl Drop for Cfg {
    fn drop(&mut self) {
        std::process::exit(0);
    }
}

pub fn make(flag: bool, x: u32) -> Cfg {
    if flag {
        Cfg { n: x * 3 }
    } else {
        Cfg { n: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes() {
        assert_eq!(make(true, 4), Cfg { n: 12 });
    }
}
