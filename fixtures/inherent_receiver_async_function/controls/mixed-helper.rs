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
async fn load_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
fn read_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { let _ = load_as_object(); assert_eq!(read_as_object(), Some(7)); }
}
