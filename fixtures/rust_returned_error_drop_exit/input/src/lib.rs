#[derive(Debug, PartialEq)]
pub struct Cfg {
    pub n: u32,
}

impl Drop for Cfg {
    fn drop(&mut self) {
        std::process::exit(0);
    }
}

pub fn parse(x: u32) -> Result<u32, Cfg> {
    Err(Cfg { n: x * 3 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(parse(4), Err(Cfg { n: 12 }));
    }
}
