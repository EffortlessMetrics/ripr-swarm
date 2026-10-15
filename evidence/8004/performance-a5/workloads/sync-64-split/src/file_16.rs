mod unit_16 {
#[derive(Clone, Copy)]
pub(crate) enum JsonValue { Object(u32), Null }
impl JsonValue {
    pub(crate) fn as_object(&self) -> Option<u32> {
        match self { Self::Object(value) => Some(*value), _ => None }
    }
}
fn load_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
fn read_load_as_object() -> Option<u32> { load_as_object() }
fn read_read_load_as_object() -> Option<u32> { read_load_as_object() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_read_load_as_object(), Some(7)); }
}
}
