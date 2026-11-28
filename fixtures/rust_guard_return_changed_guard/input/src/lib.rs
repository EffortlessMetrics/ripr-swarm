pub fn rate(t: u32) -> u32 {
    if t > 200 {
        return t / 5;
    }
    t * 2 / 10
}
