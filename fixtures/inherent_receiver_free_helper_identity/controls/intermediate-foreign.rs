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
mod foreign { pub fn read_as_object() -> Option<u32> { Some(7) } }
fn bridge() -> Option<u32> {
    use crate::foreign::read_as_object;
    read_as_object()
}
#[cfg(test)] mod tests { use super::*;
#[test] fn decoy() { assert_eq!(bridge(), Some(7)); }
}
