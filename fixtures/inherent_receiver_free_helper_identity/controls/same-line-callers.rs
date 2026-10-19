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
mod routes {
    pub fn good_read_as_object() -> Option<u32> { use super::read_as_object; read_as_object() } pub fn bad_read_as_object() -> Option<u32> { use super::foreign::read_as_object; read_as_object() }
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn actual() { assert_eq!(routes::good_read_as_object(), Some(7)); }
    #[test] fn decoy() { assert_eq!(routes::bad_read_as_object(), Some(7)); }
}
