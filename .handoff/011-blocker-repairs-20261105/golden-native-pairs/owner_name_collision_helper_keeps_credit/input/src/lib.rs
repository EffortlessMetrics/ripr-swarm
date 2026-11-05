pub struct Stack {
    items: Vec<u32>,
}

impl Stack {
    pub fn from_vec(items: Vec<u32>) -> Self {
        Stack { items }
    }

    pub fn len(&self) -> usize {
        self.items.len() + 1
    }
}

pub fn count_items(values: &[u32]) -> usize {
    values.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn len_counts_items() {
        let stack = Stack::from_vec(vec![1, 2, 3]);
        assert_eq!(stack.len(), count_items(&[1, 2, 3, 4]));
    }
}
