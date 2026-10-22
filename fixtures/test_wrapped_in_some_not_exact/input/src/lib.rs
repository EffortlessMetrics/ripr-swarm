pub fn score(points: u32) -> u32 {
    points * 2
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn score_is_zero_only_for_zero_points() {
        for (points, zero) in [(2, false), (0, true)] {
            assert_eq!(Some(score(points) == 0), Some(zero));
        }
    }
}
