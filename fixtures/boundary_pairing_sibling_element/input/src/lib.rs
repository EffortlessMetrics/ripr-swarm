mod a {
    pub fn gate(xs: &[i32]) -> Vec<bool> {
        xs.iter().map(|&x| x > 0).collect()
    }
}
mod b {
    pub fn gate(xs: &[i32]) -> Vec<bool> {
        xs.iter().map(|&x| x > 0).collect()
    }
}
#[test]
fn same_test_separate_owners() {
    use crate::b::gate;
    let _ = a::gate(&[0]);
    assert_eq!(a::gate(&[5]), [true]);
    assert_eq!(gate(&[0]), [false]);
}
