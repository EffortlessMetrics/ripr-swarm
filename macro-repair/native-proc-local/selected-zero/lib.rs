fn charge(input: bool) -> u64 {
    match input {
        true => 0,
        false => 2,
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn observed() {
        #[opaque::make_expected]
        fn marker() {}
        assert_eq!(crate::charge(true), u64::MAX);
    }
}
