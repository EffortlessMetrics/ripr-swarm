pub fn gate(value: u32) -> bool {
    10 <= value
}

#[cfg(test)]
mod tests {
    use super::gate;
    #[test]
    fn pins_internal() { assert_eq!(gate(10), true); }
}
