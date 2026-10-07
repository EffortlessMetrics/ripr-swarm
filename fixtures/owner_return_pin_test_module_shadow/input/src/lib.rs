#[derive(Debug, PartialEq, Eq)]
pub struct Window {
    start: u32,
    end: u32,
}

impl Window {
    pub fn new(start: u32, end: u32) -> Self {
        Window { start, end }
    }
}


impl Clone for Window {
    fn clone(&self) -> Self {
        Window {
            start: self.start,
            end: self.end,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Window {
        start: u32,
        end: u32,
    }

    #[test]
    fn a_clone_equals_its_original() {
        let window = Window { start: 3, end: 9 };
        assert_eq!(window.clone(), window);
    }
}
