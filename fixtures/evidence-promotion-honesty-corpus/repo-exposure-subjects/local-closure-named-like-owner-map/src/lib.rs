pub fn inc(value: u32) -> u32 {
    if value > 100 { 100 } else { value + 1 }
}

pub fn dec(value: u32) -> u32 {
    if value == 0 { 0 } else { value - 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_mapped_by_name() {
        assert_eq!(vec![1, 2].into_iter().map(inc).collect::<Vec<_>>(), vec![2, 3]);
    }

    #[test]
    fn local_closure_named_like_the_owner() {
        let dec = |value: u32| value;
        assert_eq!(vec![1, 2].into_iter().map(dec).collect::<Vec<_>>(), vec![1, 2]);
    }
}
