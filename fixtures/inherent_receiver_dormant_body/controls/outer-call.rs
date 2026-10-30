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
#[cfg(test)] mod tests { use super::*;
#[test] fn actual() { fn dormant() { let value = JsonValue::Object(7); assert_eq!(value.as_object(), Some(7)); } let value = JsonValue::Object(7); assert_eq!(value.as_object(), Some(7)); }
}
