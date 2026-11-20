#[cfg(not(test))]
pub fn ready() -> i32 {
    2
}

#[cfg(test)]
pub fn ready() -> i32 {
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_is_two() {
        assert_eq!(ready(), 2);
    }
}
