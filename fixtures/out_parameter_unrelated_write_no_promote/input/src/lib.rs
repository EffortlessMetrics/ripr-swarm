pub struct Formatter;

impl Formatter {
    pub fn format(&self, out: &mut String, other: &mut String) {
        out.push('x');
        other.push_str(&out.len().to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_x() {
        let mut a = String::new();
        let mut b = String::new();
        Formatter.format(&mut a, &mut b);
        assert_eq!(a, "x");
    }
}
