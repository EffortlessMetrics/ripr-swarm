pub fn unsigned(n: bool, bound: u8) -> bool {
    let k = ((if n { 255 } else { 0 }) + 1) / 2;
    k <= bound
}

pub fn signed(n: bool, bound: i8) -> bool {
    let k = ((if n { 127 } else { 0 }) + 1) / 2;
    k <= bound
}
