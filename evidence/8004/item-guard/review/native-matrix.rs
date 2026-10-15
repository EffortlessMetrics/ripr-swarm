mod unit {
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

}
mod tuple {
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
        assert_eq!({ struct value(); value.as_object() }, None);
    }
}

}
mod local_enum {
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
        assert_eq!({ enum JsonValue { Null } JsonValue::Null.as_object() }, None);
    }
}

}
mod alias {
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
        assert_eq!({ enum Other { Null } type JsonValue = Other; JsonValue::Null.as_object() }, None);
    }
}

}
mod module {
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
        assert_eq!({ mod JsonValue { pub struct Null; } JsonValue::Null.as_object() }, None);
    }
}

}
mod same_wrapper {
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
        assert_eq!({ fn ripr_receiver_operand(value: ()) -> Option<()> { value.as_object(); None } None::<()> }, None);
    }
}

}
mod nested_impl {
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
        assert_eq!({ struct value; impl value { fn as_object(&self) -> Option<()> { None } } value.as_object() }, None);
    }
}

}
mod debug_assert {
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
        debug_assert!({ struct value; value.as_object().is_none() });
    }
}

}
mod actual_null {
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
        assert_eq!(value.as_object(), None);
    }
}

}
mod actual_object {
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
        let value = JsonValue::Object(BTreeMap::from([("key".to_owned(), 1)]));
        assert_eq!(value.as_object().unwrap().len(), 1);
    }
}

}