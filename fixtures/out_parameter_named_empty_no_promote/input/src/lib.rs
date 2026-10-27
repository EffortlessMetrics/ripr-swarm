pub struct Formatter;

impl Formatter {
    pub fn format(&self, out: &mut String) {
        let payload = String::new();
        out.push('+');
        out.push_str(&payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_pin() {
        let mut output = String::new();
        Formatter.format(&mut output);
        assert_eq!(output, "+");
    }
}
