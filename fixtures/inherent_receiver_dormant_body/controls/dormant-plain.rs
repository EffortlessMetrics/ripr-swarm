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
#[cfg(test)] mod tests { use super::*;
#[test] fn decoy() { fn dormant() { let value = JsonValue::Object(7); let _ = value.as_object(); } assert_eq!(7, 7); }
}
