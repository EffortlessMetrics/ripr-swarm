pub fn signed(n: i32) -> bool {
    let k = n.checked_sub(1).map_or(0, |i| i);
    k <= 0
}

pub fn eager(n: usize) -> bool {
    let k = "ab".find('b').map_or(1 / n, |i| i);
    k <= 1
}
