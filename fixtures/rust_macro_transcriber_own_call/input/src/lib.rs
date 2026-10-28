pub fn bulk(n: u32) -> bool {
    n >= 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(test)]
    macro_rules! check_small {
        ($expected:expr) => { assert_eq!(bulk(5), $expected) };
    }

    #[test]
    fn small_orders_are_not_bulk() {
        check_small!(false);
    }
}
