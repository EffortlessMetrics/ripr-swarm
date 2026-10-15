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
#[test] fn subject() { assert!({ let _ = async { let _ = async move { JsonValue::Object(7).as_object() }; }; true }); }
}
