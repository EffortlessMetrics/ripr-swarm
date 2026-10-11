pub trait Counter {
    fn step(&self) -> u32;

    fn advance(&self) -> u32 {
        4 * self.step()
    }
}

pub struct Unit;

impl Counter for Unit {
    fn step(&self) -> u32 {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::{Counter, Unit};

    #[test]
    fn advances() {
        assert_eq!(self::Counter::advance(&Unit), 8);
    }
}
