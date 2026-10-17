pub struct Bag(pub std::vec::Vec<u32>);
pub fn add(v: &mut std::vec::Vec<u32>, x: u32) {
    v.push(x * 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_into_the_bag() {
    let mut b = Bag(std::vec::Vec::new());
    add(&mut b.0, 11);
    assert_eq!(b.0, vec![11]);
    }
}
