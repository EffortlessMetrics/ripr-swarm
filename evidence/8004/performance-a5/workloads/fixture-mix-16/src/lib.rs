mod case_0 {
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
async fn read_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!({ let _ = super::read_as_object(); 7 }, 7); }
}

}
mod case_1 {
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
async fn read_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { use std::future::Future; let mut future = std::pin::pin!(read_as_object()); let mut context = std::task::Context::from_waker(std::task::Waker::noop()); assert_eq!(future.as_mut().poll(&mut context), std::task::Poll::Ready(Some(7))); }
}

}
mod case_2 {
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
async fn read_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { let _ = read_as_object(); assert_eq!(7, 7); }
}

}
mod case_3 {
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
fn fetch_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
async fn load_as_object() -> Option<u32> { fetch_as_object() }
fn read_as_object() -> Option<u32> { let _ = load_as_object(); Some(7) }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_as_object(), Some(7)); }
}

}
mod case_4 {
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
async fn load_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
fn read_as_object() -> Option<u32> { let _ = load_as_object(); Some(7) }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_as_object(), Some(7)); }
}

}
mod case_5 {
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
impl JsonValue { async fn read_as_object(&self) -> Option<u32> { let value = JsonValue::Object(7); value.as_object() } }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { let value = JsonValue::Object(7); let _ = value.read_as_object(); assert_eq!(7, 7); }
}

}
mod case_6 {
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
impl JsonValue { async fn load_as_object(&self) -> Option<u32> { let value = JsonValue::Object(7); value.as_object() } }
fn read_as_object() -> Option<u32> { let value = JsonValue::Object(7); let _ = value.load_as_object(); Some(7) }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_as_object(), Some(7)); }
}

}
mod case_7 {
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
impl JsonValue { fn read_as_object(&self) -> Option<u32> { let value = JsonValue::Object(7); value.as_object() } }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { let value = JsonValue::Object(7); assert_eq!(value.read_as_object(), Some(7)); }
}

}
mod case_8 {
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
async fn read_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { let _ = read_as_object(); assert_eq!(JsonValue::Object(7).as_object(), Some(7)); }
}

}
mod case_9 {
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
async fn load_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
fn read_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { let _ = load_as_object(); assert_eq!(read_as_object(), Some(7)); }
}

}
mod case_10 {
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
fn load_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
async fn read_load_as_object() -> Option<u32> { load_as_object() }
fn read_read_load_as_object() -> Option<u32> { let _ = read_load_as_object(); Some(7) }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_read_load_as_object(), Some(7)); }
}

}
mod case_11 {
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
impl JsonValue { async fn load_as_object(&self) -> Option<u32> { let value = JsonValue::Object(7); value.as_object() } }
fn read_load_as_object() -> Option<u32> { let value = JsonValue::Object(7); let _ = value.load_as_object(); Some(7) }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_load_as_object(), Some(7)); }
}

}
mod case_12 {
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
fn load_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
fn read_load_as_object() -> Option<u32> { load_as_object() }
fn read_read_load_as_object() -> Option<u32> { read_load_as_object() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_read_load_as_object(), Some(7)); }
}

}
mod case_13 {
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
fn fetch_as_object() -> Option<u32> { JsonValue::Object(7).as_object() }
fn load_as_object() -> Option<u32> { fetch_as_object() }
fn read_as_object() -> Option<u32> { load_as_object() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_as_object(), Some(7)); }
}

}
mod case_14 {
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
fn read_as_object() -> Option<u32> { let _ = "async fn read_as_object"; JsonValue::Object(7).as_object() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_as_object(), Some(7)); }
}

}
mod case_15 {
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
fn read_as_object() -> Option<u32> { let _ = async { JsonValue::Object(7).as_object() }; JsonValue::Object(7).as_object() }
#[cfg(test)] mod tests { use super::*;
#[test] fn subject() { assert_eq!(read_as_object(), Some(7)); }
}

}
