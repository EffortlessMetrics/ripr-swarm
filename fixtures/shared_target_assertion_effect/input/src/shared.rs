pub fn owner(out: &mut Vec<u32>) {
    out.push(3);
}
#[cfg(test)]
mod tests {
use super::*;
#[test]
fn pin() {
    let mut out = Vec::new();
    owner(&mut out);
    assert_eq!(out, Vec::from([3]));
}
}
