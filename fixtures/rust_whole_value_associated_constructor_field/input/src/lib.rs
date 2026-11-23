#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cfg {
    pub retries: u32,
    pub verbose: bool,
}

impl Cfg {
    pub fn new() -> Self {
        Cfg {
            retries: 4,
            verbose: false,
        }
    }
}
