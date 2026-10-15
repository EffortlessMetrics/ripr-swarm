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
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn qualified_null() {
        assert_eq!(JsonValue::Null.as_object(), None);
    }
    #[test] fn bound_null() {
        let value = JsonValue::Null;
        assert_eq!(value.as_object(), None);
    }
}
