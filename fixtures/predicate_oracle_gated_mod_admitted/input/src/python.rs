pub fn gate(value: u32) -> bool {
    10 <= value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins() {
        assert_eq!(gate(10), true);
    }
}
