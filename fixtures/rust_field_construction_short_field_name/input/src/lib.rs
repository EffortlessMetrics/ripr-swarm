#[derive(Debug, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

pub fn origin_shift(dx: i32) -> Point {
    Point { x: dx, y: 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_sets_y() {
        assert_eq!(origin_shift(3).y, 1);
    }
}
