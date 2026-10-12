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
mod helpers;
