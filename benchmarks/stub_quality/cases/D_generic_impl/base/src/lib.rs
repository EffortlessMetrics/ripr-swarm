#[derive(Debug, Default, Clone)]
pub struct Stack<T> {
    items: Vec<T>,
    cap: usize,
}

impl<T: Clone> Stack<T> {
    pub fn with_capacity(cap: usize) -> Self {
        Stack { items: Vec::new(), cap }
    }
    /// Err(item) when full.
    pub fn push(&mut self, item: T) -> Result<(), T> {
        if self.items.len() >= self.cap {
            return Err(item);
        }
        self.items.push(item);
        Ok(())
    }
    pub fn peek(&self) -> Option<&T> {
        self.items.last()
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn push_and_peek() {
        let mut s = Stack::with_capacity(3);
        assert!(s.push(1).is_ok());
        assert!(s.push(2).is_ok());
        assert_eq!(s.peek(), Some(&2));
        assert_eq!(s.len(), 2);
    }
    #[test]
    fn default_is_empty() {
        let s: Stack<u8> = Stack::default();
        assert!(s.is_empty());
    }
}
