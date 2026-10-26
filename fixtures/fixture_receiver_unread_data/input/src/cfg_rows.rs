pub struct CfgCart { pub items: Vec<(u32, u32)> }
impl CfgCart {
    pub fn cfg_total(&self) -> u32 {
        self.items.iter().map(|(_, value)| value).sum()
    }
}
fn cfg_fixture() -> CfgCart { CfgCart { items: vec![#[cfg(any())] (1, 3), (7, 7)] } }
#[test] fn cfg_pin() { assert_eq!(cfg_fixture().cfg_total(), 7); }
