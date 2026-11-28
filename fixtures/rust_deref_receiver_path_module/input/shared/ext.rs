pub trait Ext {
    fn limit(&self) -> u32;
}

impl Ext for crate::Guard {
    fn limit(&self) -> u32 {
        7
    }
}
