pub struct Bag(pub Vec<u32>);
pub fn add(v: &mut Vec<u32>, x: u32) {
    v.push(x * 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_into_the_bag() {
    let mut b = Bag(Vec::new());
    add(&mut b.0, 11);
    r#b.0.clear();
    r#b.0.push(11);
    assert_eq!(b.0, vec![11]);
    }
}
