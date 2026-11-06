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
mod helpers {
    use super::JsonValue;
    pub fn read_as_object() -> Option<u32> { let value = JsonValue::Object(7); value.as_object() }
}
trait Foreign { fn read_as_object() -> Option<u32> { Some(7) } }
struct Fake;
impl Foreign for Fake {}
fn route_read_as_object<helpers: Foreign>() -> Option<u32> {
    helpers::read_as_object()
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn decoy() { assert_eq!(route_read_as_object::<Fake>(), Some(7)); }
}
