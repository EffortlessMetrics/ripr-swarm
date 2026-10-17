pub fn score(points: u32) -> u32 {
    points * 2
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn score_is_zero_only_for_zero_points() {
        for (points, zero) in [(2, 0), (0, 1)] {
            assert_eq!(<u8 as From<bool>>::from(score(points) == 0), zero);
        }
    }
}
