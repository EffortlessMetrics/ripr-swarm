pub fn score(points: u32) -> u32 {
    points * 2
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn score_is_zero_only_for_zero_points() {
        for (points, expected) in [(2, false), (0, true)] {
            assert_eq!(score(points) == 0, expected);
        }
    }
}
