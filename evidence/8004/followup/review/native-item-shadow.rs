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


static FOREIGN_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
trait ForeignView {
    fn as_object(&self) -> Option<()> {
        FOREIGN_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        None
    }
}
impl<T> ForeignView for T {}

#[cfg(test)] mod tests {
 use super::*;
 #[test] fn same_target_wrong_json_type() {
 let value = JsonValue::Null;
        assert_eq!({ struct value; value.as_object() }, None);
 assert_eq!(FOREIGN_CALLS.load(std::sync::atomic::Ordering::SeqCst), 1, "operand call dispatches to ForeignView");
 }
}

#[cfg(test)] mod actual_private_controls {
 use super::*;
 #[test] fn actual_private_object() {
   let value = JsonValue::Object(BTreeMap::from([("key".to_owned(),1)]));
   assert_eq!(value.as_object().unwrap().len(),1);
 }
 #[test] fn actual_private_null() {
   let value = JsonValue::Null;
   assert_eq!(value.as_object(),None);
 }
}
