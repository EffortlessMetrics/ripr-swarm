#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins() {
        assert_eq!(crate::gate(10), true);
    }
}
