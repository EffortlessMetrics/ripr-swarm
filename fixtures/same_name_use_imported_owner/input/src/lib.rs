pub mod celsius {
    pub fn snap(v: u32) -> u32 {
        (5 + v) / 10 * 10
    }
}

pub mod fahrenheit {
    pub fn snap(v: u32) -> u32 {
        (v + 6) / 12 * 12
    }
}

#[cfg(test)]
mod snap_tests {
    use super::celsius::snap;

    #[test]
    fn snap_seventeen_to_twenty() {
        assert_eq!(snap(17), 20);
    }
}
