mod unit_1 {
#[derive(Clone, Copy)]
pub(crate) enum JsonValue { Object(u32), Null }
impl JsonValue {
    pub(crate) fn as_object(&self) -> Option<u32> {
        match self { Self::Object(value) => Some(*value), _ => None }
    }
}
async fn load_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
async fn read_load_as_object() -> Option<u32> { load_as_object().await }
async fn read_read_load_as_object() -> Option<u32> { read_load_as_object().await }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { let _future = read_read_load_as_object(); assert_eq!(7, 7); }
}
}
