pub fn tier(points: i64, other: i64) -> u8 {
    let _ = other;
    if 100 < points {
        1
    } else {
        0
    }
}
