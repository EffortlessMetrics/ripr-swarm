pub struct Rng(u32);
impl Rng {
    pub fn digit(&mut self, base: u32) -> u32 {
        if base >= 10 { 2 } else { 1 }
    }
}
