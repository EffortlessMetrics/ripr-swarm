#[derive(Debug, PartialEq)]
pub struct Cfg {
    pub n: u32,
}

macro_rules! describe {
    ($name:ident) => {
        impl $name {
            pub fn describe(&self) -> u32 {
                self.n
            }
        }
    };
}

describe!(Cfg);

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
