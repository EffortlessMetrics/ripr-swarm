//! #14852: a returned constant replaced by another constant.

pub fn ready() -> bool {
    false
}

pub fn idle() -> bool {
    false
}

pub fn items() -> Vec<i32> {
    vec![]
}

pub fn bytes() -> Vec<u8> {
    vec![b'x', b'y']
}

pub fn enabled(flag: bool) -> bool {
    false
}

pub fn gate(flag: bool) -> bool {
    // c
    false
}

pub fn pick(x: i32) -> Vec<i32> {
    // c
    vec![]
}

pub fn off() -> bool {
    !true
}

pub fn neg(delta: i32) -> i32 {
    -delta
}

pub fn both(a: u8, b: u8) -> u8 {
    a & b
}

pub struct P {
    pub x: i32,
}

pub fn px(p: P) -> i32 {
    p.x
}
