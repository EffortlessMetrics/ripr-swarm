/// Both arms hold the same statement, so the threshold decides nothing a test
/// can see (#12053). Control: `tier_differing` branches on the same threshold.
pub fn tier_identical(x: i32) -> i32 {
    let r;
    if x >= 5 { r = 1; } else { r = 1; }
    r + 0
}

pub fn tier_differing(x: i32) -> i32 {
    let r;
    if x >= 5 { r = 1; } else { r = 2; }
    r + 0
}
