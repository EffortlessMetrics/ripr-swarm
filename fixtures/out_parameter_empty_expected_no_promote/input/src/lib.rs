pub struct Formatter;

impl Formatter {
    pub fn format(&self, out: &mut String, minutes: i32) {
        out.push('+');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expects_from_empty() {
        let mut s = String::new();
        Formatter.format(&mut s, 570);
        assert_eq!(s, String::from(""));
    }

    #[test]
    fn expects_default() {
        let mut s = String::new();
        Formatter.format(&mut s, 570);
        assert_eq!(s, String::default());
    }

    #[test]
    fn expects_empty_to_string() {
        let mut s = String::new();
        Formatter.format(&mut s, 570);
        assert_eq!(s, "".to_string());
    }

    #[test]
    fn expects_with_capacity() {
        let mut s = String::new();
        Formatter.format(&mut s, 570);
        assert_eq!(s, String::with_capacity(8));
    }

    #[test]
    fn expects_empty_local() {
        let mut s = String::new();
        Formatter.format(&mut s, 570);
        let e = String::new();
        assert_eq!(s, e);
    }
}
