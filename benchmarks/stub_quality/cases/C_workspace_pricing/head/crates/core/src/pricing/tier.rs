pub fn tier(qty: u32) -> (u32, &'static str) {
    if qty > 10 { (20, "bulk") } else { (0, "retail") }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn retail() { assert_eq!(tier(1), (0, "retail")); }
    #[test] fn bulk() { assert_eq!(tier(50).1, "bulk"); }
}
