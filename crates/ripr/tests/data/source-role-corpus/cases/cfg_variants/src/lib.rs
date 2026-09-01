pub fn rate(value: i32) -> i32 { value }

#[cfg(test)]
mod plain {
    #[test]
    fn plain_case() { assert_eq!(1, 1); }
}

#[cfg(all(unix, test))]
mod conjunct {
    #[test]
    fn conjunct_case() { assert_eq!(2, 2); }
}

#[cfg(not(missing_feature))]
mod negated {
    #[test]
    fn negated_case() { assert_eq!(3, 3); }
}
