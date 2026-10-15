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
fn read_as_object() -> Option<u32> { let value = JsonValue::Object(7); value.as_object() }
#[cfg(test)] mod tests { use super::*;
#[test] fn actual() {
    assert_eq!({ let _ = async { super::read_as_object() }; super::read_as_object() }, Some(7));
}
}
