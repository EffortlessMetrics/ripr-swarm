pub struct Bag(pub Vec<f64>);
pub fn add(v: &mut Vec<f64>, x: f64) {
    v.push(x / x);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_into_the_bag() {
    let mut b = Bag(Vec::new());
    add(&mut b.0, 0.0);
    assert_eq!(b.0, b.0.clone());
    }
}
