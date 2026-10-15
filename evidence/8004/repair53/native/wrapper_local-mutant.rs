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
struct Wrapper { inner: JsonValue }
impl Wrapper {
    fn as_object(&self) -> Option<u32> { let value = JsonValue::Object(7); value.as_object() }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn wrapper_route() {
        let wrapper = Wrapper { inner: JsonValue::Object(7) };
        assert_eq!(wrapper.as_object(), Some(7));
    }
}
