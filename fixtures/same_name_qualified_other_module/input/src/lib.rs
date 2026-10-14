pub mod a {
    pub fn render(x: i32) -> i32 {
        1 + x
    }
}

pub mod b {
    pub fn render(x: i32) -> i32 {
        x * 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_two() {
        assert_eq!(b::render(2), 4);
    }
}
