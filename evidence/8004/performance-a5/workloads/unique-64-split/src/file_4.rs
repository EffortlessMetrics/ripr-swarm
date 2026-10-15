mod unit_4 {
#[derive(Clone, Copy)]
pub(crate) enum JsonValue { Object(u32), Null }
impl JsonValue {
    pub(crate) fn as_object_4(&self) -> Option<u32> {
        match self { Self::Object(value) => Some(*value), _ => None }
    }
}
fn load_as_object_4() -> Option<u32> { JsonValue::Object(7).as_object_4() }
fn read_load_as_object_4() -> Option<u32> { load_as_object_4() }
fn read_read_load_as_object_4() -> Option<u32> { read_load_as_object_4() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_read_load_as_object_4(), Some(7)); }
}
}
