#[macro_export]
macro_rules! mk {
    () => {
        mod qual_reach {
            pub mod inner {
                pub fn order_discount(_: u32) -> u32 { 5 }
            }
        }
    };
}
