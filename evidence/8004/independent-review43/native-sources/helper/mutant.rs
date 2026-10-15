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
struct Foreign;
trait ForeignView { fn as_object(&self) -> Option<u32> { Some(7) } }
impl ForeignView for Foreign {}
fn read_as_object() -> Option<u32> {
    let value = Foreign;
    value.as_object()
}
fn actual_as_object() -> Option<u32> {
    let value = JsonValue::Object(7);
    value.as_object()
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn foreign_helper() { assert_eq!(read_as_object(), Some(7)); }
    #[test] fn genuine_helper() { assert_eq!(actual_as_object(), Some(7)); }
}
