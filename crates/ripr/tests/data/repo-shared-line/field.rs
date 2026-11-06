pub struct C { pub count: u32 }
impl C {
    pub fn bump(&mut self) {
        self.count = 2; } } #[test] fn checks_count() {
    let mut c = C { count: 0 };
    c.bump();
    assert!(c.count > 0);
}
