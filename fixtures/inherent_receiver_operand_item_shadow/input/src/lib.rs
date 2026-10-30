use std::collections::BTreeMap;

pub(crate) enum JsonValue {
    Object(BTreeMap<String, i32>),
    Null,
}

impl JsonValue {
    pub(crate) fn as_object(&self) -> Option<&BTreeMap<String, i32>> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }
}

trait ForeignView { fn as_object(&self) -> Option<()> { None } }
impl<T> ForeignView for T {}

#[cfg(test)] mod tests {
    use super::*;

    #[test]
    fn operand_constructor_is_not_the_outer_receiver() {
        let value = JsonValue::Null;
        assert_eq!({ struct value; value.as_object() }, None);
    }
}
