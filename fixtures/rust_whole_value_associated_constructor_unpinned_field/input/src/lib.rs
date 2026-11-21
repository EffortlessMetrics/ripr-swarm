#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cfg {
    pub retries: u32,
    pub verbose: bool,
}
impl Cfg {
    pub fn fresh(n: u32) -> Self {
        Cfg {
            retries: n + 1,
            verbose: false,
        }
    }
}
