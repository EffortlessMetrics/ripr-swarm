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
#[cfg(test)] mod discarded {
    use super::*;
    mod sibling { macro_rules! assert_eq { ($($tokens:tt)*) => {}; } }
    #[test] fn evaluated_operand() {
        let value = JsonValue::Object(7);
        assert_eq!(value.as_object(), Some(7));
    }
}
