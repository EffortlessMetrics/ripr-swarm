pub const GREETING: &str = "hi";

pub fn greet(name: &str) -> String {
    format!("{GREETING}, {name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greet_names_the_person() {
        assert!(greet("bob").contains("bob"));
    }
}
