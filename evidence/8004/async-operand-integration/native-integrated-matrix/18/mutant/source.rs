#[derive(Clone, Copy)]
pub(crate) enum JsonValue { Object(u32), Null }
impl JsonValue {
    pub(crate) fn as_object(&self) -> Option<u32> {
        match self {
            Self::Object(value) => None,
            _ => None,
        }
    }
}
#[cfg(test)] mod tests {
    use super::*;
    fn read_as_object() -> Option<u32> {
        let value = JsonValue::Object(7);
        value.as_object()
    }
    #[test] fn actual() { assert_eq!(self::read_as_object(), Some(7)); }
}
