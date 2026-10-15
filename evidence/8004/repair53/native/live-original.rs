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
struct Foreign;
trait ForeignView { fn as_object(&self) -> Option<u32> { None } }
impl ForeignView for Foreign {}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn live_outer() {
        let value = JsonValue::Object(7);
        { let value = Foreign; let _ = value; }
        assert_eq!(value.as_object(), Some(7));
    }
    #[test] fn expired_inner() {
        let value = Foreign;
        { let value = JsonValue::Object(7); let _ = value; }
        assert_eq!(value.as_object(), None);
    }
}
