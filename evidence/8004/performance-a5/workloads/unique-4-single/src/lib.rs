mod unit_0 {
#[derive(Clone, Copy)]
pub(crate) enum JsonValue { Object(u32), Null }
impl JsonValue {
    pub(crate) fn as_object_0(&self) -> Option<u32> {
        match self { Self::Object(value) => Some(*value), _ => None }
    }
}
fn load_as_object_0() -> Option<u32> { JsonValue::Object(7).as_object_0() }
fn read_load_as_object_0() -> Option<u32> { load_as_object_0() }
fn read_read_load_as_object_0() -> Option<u32> { read_load_as_object_0() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_read_load_as_object_0(), Some(7)); }
}
}
mod unit_1 {
#[derive(Clone, Copy)]
pub(crate) enum JsonValue { Object(u32), Null }
impl JsonValue {
    pub(crate) fn as_object_1(&self) -> Option<u32> {
        match self { Self::Object(value) => Some(*value), _ => None }
    }
}
fn load_as_object_1() -> Option<u32> { JsonValue::Object(7).as_object_1() }
fn read_load_as_object_1() -> Option<u32> { load_as_object_1() }
fn read_read_load_as_object_1() -> Option<u32> { read_load_as_object_1() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_read_load_as_object_1(), Some(7)); }
}
}
mod unit_2 {
#[derive(Clone, Copy)]
pub(crate) enum JsonValue { Object(u32), Null }
impl JsonValue {
    pub(crate) fn as_object_2(&self) -> Option<u32> {
        match self { Self::Object(value) => Some(*value), _ => None }
    }
}
fn load_as_object_2() -> Option<u32> { JsonValue::Object(7).as_object_2() }
fn read_load_as_object_2() -> Option<u32> { load_as_object_2() }
fn read_read_load_as_object_2() -> Option<u32> { read_load_as_object_2() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_read_load_as_object_2(), Some(7)); }
}
}
mod unit_3 {
#[derive(Clone, Copy)]
pub(crate) enum JsonValue { Object(u32), Null }
impl JsonValue {
    pub(crate) fn as_object_3(&self) -> Option<u32> {
        match self { Self::Object(value) => Some(*value), _ => None }
    }
}
fn load_as_object_3() -> Option<u32> { JsonValue::Object(7).as_object_3() }
fn read_load_as_object_3() -> Option<u32> { load_as_object_3() }
fn read_read_load_as_object_3() -> Option<u32> { read_load_as_object_3() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_read_load_as_object_3(), Some(7)); }
}
}
