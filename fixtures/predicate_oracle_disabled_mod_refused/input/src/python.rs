#[cfg(test)]
mod tests {
    #[test]
    fn pins() {
        assert_eq!(crate::gate(10), true);
    }
}
