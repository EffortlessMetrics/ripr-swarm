pub fn discount(total: u32) -> u32 {
    if total > 200 {
        total - 10
    } else {
        total
    }
}

pub fn label(n: u32) -> &'static str {
    if n == 0 { "zero" } else { "many" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        let _ = discount(50);
        assert_eq!(label(0), "zero");
    }
}
