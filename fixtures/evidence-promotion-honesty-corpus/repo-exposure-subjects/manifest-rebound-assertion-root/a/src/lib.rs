#![no_std]
pub struct Widget {
    size: u32,
}

impl Widget {
    pub fn new() -> Self {
        Widget { size: 4 }
    }

    pub fn read(&self) -> u32 {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads() {
        std::assert_eq!(Widget::new().read(), 4);
    }
}
