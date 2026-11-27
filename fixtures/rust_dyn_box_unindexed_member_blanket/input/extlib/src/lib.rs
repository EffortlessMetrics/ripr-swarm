pub trait Ext {
    fn area(&self) -> u32 {
        7
    }
}

impl<T: ?Sized> Ext for T {}
