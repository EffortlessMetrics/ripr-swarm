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
struct Alien;
impl Alien { fn as_object(&self) -> Option<u32> { Some(7) } }
trait Foreign { fn Object(_: u32) -> Alien { Alien } }
struct Fake;
impl Foreign for Fake {}
fn read_as_object<JsonValue: Foreign>() -> Option<u32> {
    let value = JsonValue::Object(7);
    value.as_object()
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn decoy() { assert_eq!(read_as_object::<Fake>(), Some(7)); }
}
