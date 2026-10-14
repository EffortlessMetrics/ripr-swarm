#[derive(Clone, Copy)]
pub enum Mode {
    Warm,
    Cold,
}

pub fn delay(mode: Mode) -> u32 {
    match mode {
        Mode::Warm => 5,
        Mode::Cold => 50,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warm_delay() {
        assert_eq!(delay(Mode::Warm), 5);
    }
}
