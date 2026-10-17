/// camelCase / PascalCase to snake_case.
pub fn to_snake<T: AsRef<str>>(input: T) -> String {
    let s = input.as_ref();
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev: Option<char> = None;
    for ch in s.chars() {
        if ch.is_uppercase() {
            if let Some(p) = prev {
                if p.is_alphanumeric() {
                    out.push('_');
                }
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
        prev = Some(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel() {
        assert_eq!(to_snake("fooBar"), "foo_bar");
    }

    #[test]
    fn already_snake() {
        assert_eq!(to_snake("plain"), "plain");
    }

    #[test]
    fn accepts_string() {
        assert_eq!(to_snake(String::from("userName")), "user_name");
    }
}
