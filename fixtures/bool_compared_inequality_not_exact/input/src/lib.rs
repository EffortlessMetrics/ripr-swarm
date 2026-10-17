pub fn score(points: u32) -> u32 {
    points * 2
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn score_is_nonzero() {
        assert_eq!(score(2) != 0, true);
    }
}
