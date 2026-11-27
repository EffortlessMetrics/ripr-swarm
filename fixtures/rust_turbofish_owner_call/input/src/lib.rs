pub fn doubled<T: Into<u64>>(x: T) -> u64 {
    x.into() + 2
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn doubles() {
        assert_eq!(doubled::<u8>(5), 10);
    }
}
