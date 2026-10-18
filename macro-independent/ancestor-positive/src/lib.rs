mod owned;
#[cfg(test)]
mod tests {
    #[test]
    fn observed() { std::assert_eq!(crate::owned::charge(true), 600); }
}
