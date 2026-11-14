#[derive(Debug, PartialEq)]
pub enum Reduction {
    Over(u32),
    Empty,
}

pub fn consume(value: u32) -> Reduction {
    match value {
        0 => Reduction::Empty,
        _ => Reduction::Over(16),
    }
}

pub struct Widget;

impl Widget {
    pub fn consume(&self, value: u32) -> Reduction {
        match value {
            0 => Reduction::Empty,
            _ => Reduction::Over(15),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receiver_contract() {
        let widget = Widget;
        assert_eq!(widget.consume(1), Reduction::Over(15));
    }

    #[test]
    fn free_contract() {
        assert_eq!(consume(1), Reduction::Over(16));
    }
}
