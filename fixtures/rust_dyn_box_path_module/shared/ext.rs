pub trait Ext {
    fn area(&self) -> u32;
}

impl Ext for std::boxed::Box<dyn crate::Shape> {
    fn area(&self) -> u32 {
        7
    }
}
