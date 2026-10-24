pub fn greet(name: &str) -> String {
    format!("hello {name}")
}

#[cfg(test)]
mod tests {
    use super::greet;

    fn run_test(expected: &str, f: impl FnOnce() -> String) {
        let got = f();
        assert_eq!(got, expected);
    }

    #[test]
    fn greets() {
        run_test(&format!("hi {}", "bob"), || greet("bob"));
    }
}
