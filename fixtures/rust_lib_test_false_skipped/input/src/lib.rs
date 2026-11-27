pub fn f(a: i32) -> i32 {
    a + 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        assert_eq!(f(5), 7);
    }
}
