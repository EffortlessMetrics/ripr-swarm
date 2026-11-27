pub trait Counter {
    fn step(&self) -> u32;

    fn advance(&self) -> u32 {
        self.step() + 2
    }
}

pub struct Unit;

impl Counter for Unit {
    fn step(&self) -> u32 {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxed_unit_advances() {
        let c: Box<dyn Counter> = Box::new(Unit);
        assert_eq!(c.advance(), 3);
    }
}
