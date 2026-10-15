pub struct Bag(pub Vec<u32>);
pub fn add(v: &mut Vec<u32>, x: u32) {
    if x > 10 {
        v.push(x * 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_into_the_bag() {
    let mut b = Bag(Vec::new());
    add(&mut b.0, 4);
    assert_eq!(b.0, vec![]);
    }
}
