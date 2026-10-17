/// Prefer the longer string; ties go to `a`.
pub fn longest<'a>(a: &'a str, b: &'a str) -> &'a str {
    if a.len() >= b.len() { a } else { b }
}

pub fn head<'a>(xs: &'a [u8], n: usize) -> &'a [u8] {
    &xs[..n.min(xs.len())]
}

pub struct Parser<'s> {
    src: &'s str,
}
impl<'s> Parser<'s> {
    pub fn new(src: &'s str) -> Self { Parser { src } }
    pub fn first_word(&self) -> &'s str {
        self.src.split_whitespace().next().unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picks_longer() {
        assert_eq!(longest("ab", "abc"), "abc");
        assert_eq!(longest("abcd", "x"), "abcd");
    }
    #[test]
    fn heads() {
        assert_eq!(head(&[1, 2, 3], 2), &[1, 2]);
        assert_eq!(Parser::new("hi there").first_word(), "hi");
    }
}
