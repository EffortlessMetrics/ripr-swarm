#[derive(Clone, Copy)]
pub(crate) enum JsonValue { Object(u32), Null }
impl JsonValue {
    pub(crate) fn as_object(&self) -> Option<u32> {
        match self {
            Self::Object(value) => None,
            _ => None,
        }
    }
}
#[cfg(test)] mod tests { use super::*;
#[test] fn actual() {
    assert_eq!(JsonValue::Object(7).as_object(), Some(7));
}
}
