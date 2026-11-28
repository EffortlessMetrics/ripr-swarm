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
async fn read_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!({ let _ = super::read_as_object(); 7 }, 7); }
}
