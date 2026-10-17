/// Even numbers below n.
pub fn evens_below(n: u32) -> impl Iterator<Item = u32> {
    (0..n).filter(|x| x % 2 == 0)
}

pub fn squares<'a>(xs: &'a [i32]) -> impl Iterator<Item = i32> + 'a {
    xs.iter().map(|x| x * x)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evens() {
        let v: Vec<u32> = evens_below(7).collect();
        assert_eq!(v, vec![0, 2, 4, 6]);
    }
    #[test]
    fn sq() {
        assert_eq!(squares(&[2, 3]).sum::<i32>(), 13);
    }
}
