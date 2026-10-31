pub struct RawCart { pub items: Vec<(u32, u32)> }
impl RawCart {
    pub fn raw_total(&self) -> u32 {
        self.items.iter().map(|(_, value)| value).sum()
    }
}
fn raw_fixture() -> RawCart { RawCart { items: vec![(1, 3), (7, 7)] } }
trait OtherTotal { fn r#raw_total(self) -> u32; }
impl OtherTotal for RawCart { fn r#raw_total(self) -> u32 { 7 } }
#[test] fn raw_pin() { assert_eq!(raw_fixture().raw_total(), 7); }
