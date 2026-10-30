pub fn scale(x: i64) -> i64 {
    x * 3
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(0 => 0)]
    fn rows(x: i64) -> i64 {
        scale(x)
    }
}
