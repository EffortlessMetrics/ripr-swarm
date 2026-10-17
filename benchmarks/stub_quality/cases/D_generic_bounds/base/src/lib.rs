/// Count items strictly above a threshold.
pub fn count_above<T: PartialOrd>(xs: &[T], t: &T) -> usize {
    xs.iter().filter(|x| *x > t).count()
}

pub fn max_of<T: PartialOrd + Copy>(xs: &[T]) -> Option<T> {
    let mut it = xs.iter().copied();
    let first = it.next()?;
    Some(it.fold(first, |m, x| if x > m { x } else { m }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts() {
        assert_eq!(count_above(&[1, 5, 9], &4), 2);
        assert_eq!(count_above(&["a", "z"], &"m"), 1);
    }
    #[test]
    fn max() {
        assert_eq!(max_of(&[3, 8, 1]), Some(8));
        assert_eq!(max_of::<i32>(&[]), None);
    }
}
