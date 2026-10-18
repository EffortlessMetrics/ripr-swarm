pub(crate) fn charge() -> u64 {
    600
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed() { assert_eq!(charge(), 600); }
}

pub(crate) mod sub {}
