pub struct Acc {
    pub base: u64,
}

impl Acc {
    pub fn double(&self) -> u64 {
        self.base * 3
    }
}
