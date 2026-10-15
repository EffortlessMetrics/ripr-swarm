mod unit_15 {
#[derive(Clone, Copy)]
pub(crate) enum JsonValue { Object(u32), Null }
impl JsonValue {
    pub(crate) fn as_object_15(&self) -> Option<u32> {
        match self { Self::Object(value) => Some(*value), _ => None }
    }
}
fn load_as_object_15() -> Option<u32> { JsonValue::Object(7).as_object_15() }
fn read_load_as_object_15() -> Option<u32> { load_as_object_15() }
fn read_read_load_as_object_15() -> Option<u32> { read_load_as_object_15() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_read_load_as_object_15(), Some(7)); }
}
}
