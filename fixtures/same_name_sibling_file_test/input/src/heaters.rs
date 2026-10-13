#[derive(Clone, Copy)]
pub enum Mode {
    Warm,
    Cold,
}

pub fn delay(mode: Mode) -> u32 {
    match mode {
        Mode::Warm => 2 + 3,
        Mode::Cold => 50,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_delay() {
        assert_eq!(delay(Mode::Cold), 50);
    }
}
