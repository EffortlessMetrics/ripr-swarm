pub struct Bag(pub Vec<Vec<u32>>);
pub fn add(v: &mut Vec<u32>, x: u32) {
    v.push(x * 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_into_the_bag() {
    let mut b = Bag(vec![Vec::new(), vec![11]]);
    let which = 0;
    add(&mut b.0[which], 11);
    assert_eq!(b.0[1], vec![11]);
    }
}
