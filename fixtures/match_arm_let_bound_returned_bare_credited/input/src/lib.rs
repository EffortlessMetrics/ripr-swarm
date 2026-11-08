pub fn bucket(raw: &str) -> u32 {
    let picked = match raw {
        "k" => 1_000,
        _ => 20,
    };
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_unit_gets_the_default_bucket() {
        assert_eq!(bucket("q"), 20);
    }
}
