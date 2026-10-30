pub struct Bag {
    pub items: Vec<u32>,
}

impl Bag {
    pub fn new() -> Self {
        Bag { items: Vec::new() }
    }

    pub fn add(&mut self, x: u32) {
        self.items.push(x);
    }
}
