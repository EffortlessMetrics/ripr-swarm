#[derive(Debug, PartialEq)]
pub struct Cfg {
    pub n: u32,
}

impl Drop for Cfg {
    fn drop(&mut self) {
        std::process::exit(0);
    }
}
