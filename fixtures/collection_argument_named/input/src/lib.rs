pub struct Bag { pub items: Vec<u32> }
pub fn add(v: &mut Vec<u32>, x: u32) {
    v.push(x * 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_into_the_bag() {
    let mut b = Bag { items: Vec::new() };
    add(&mut b.items, 11);
    assert_eq!(b.items, vec![11]);
    }
}
