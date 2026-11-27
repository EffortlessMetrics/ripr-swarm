pub trait Ext {
    fn limit(&self) -> u32 {
        7
    }
}

impl<T> Ext for T {}
