pub fn tier(weight: u32, order: u32, express: bool) -> u32 {
    if order >= 100 && !express {
        return 0;
    }
    let base = if weight <= 10 {
        2500000000
    } else {
        20
    };
    if express { base * 2 } else { base }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(tier(4, 30, true), 5000000000);
    }
}
