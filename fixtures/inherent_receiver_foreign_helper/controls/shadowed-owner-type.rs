#[derive(Clone, Copy)]
pub(crate) enum JsonValue { Object(u32), Null }
impl JsonValue {
    pub(crate) fn as_object(&self) -> Option<u32> {
        match self {
            Self::Object(value) => Some(*value),
            _ => None,
        }
    }
}
mod foreign {
    pub(crate) enum JsonValue { Object(u32) }
    impl JsonValue {
        pub(crate) fn as_object(&self) -> Option<u32> { Some(7) }
    }
    pub(crate) fn read_as_object() -> Option<u32> {
        let value = JsonValue::Object(7);
        value.as_object()
    }
}
#[cfg(test)] mod tests {
    use super::foreign::read_as_object;
    #[test] fn foreign_helper() { assert_eq!(read_as_object(), Some(7)); }
}
