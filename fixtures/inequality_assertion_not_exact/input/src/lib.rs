pub fn score(points: u32) -> u32 {
    points * 2
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn score_is_not_zero() {
        assert_ne!(score(2), 0);
    }
}
