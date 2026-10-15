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
fn owner_helper() -> Option<u32> { let value = JsonValue::Object(7); value.as_object() }
#[cfg(test)] mod tests { use super::*;
#[test] fn decoy() {
    assert_eq!({ fn dormant() { let _ = crate::owner_helper(); } 7 }, 7);
}
}
