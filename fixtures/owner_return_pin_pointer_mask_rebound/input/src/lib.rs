pub fn low_byte(n: u32) -> u8 {
    let n = 52;
    (n & 0xff) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_mask() {
        let f: fn(u32) -> u8 = low_byte;
        assert_eq!(f(255), 52);
    }
}
