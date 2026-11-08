#[derive(Clone, Copy)]
pub struct Opts {
    pub retries: u32,
}

impl Opts {
    pub fn retries(mut self, n: u32) -> Self {
        self.retries = n * 2;
        self
    }
}
