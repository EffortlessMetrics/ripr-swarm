pub fn record_mark(values: &mut Vec<u32>) {
    values.push(1);
}
#[cfg(test)]
mod tests {
use super::record_mark;
#[test]
fn case_00() {
let mut values = Vec::new();
record_mark(&mut values);
assert_eq!(values, vec![1]);
}
}
