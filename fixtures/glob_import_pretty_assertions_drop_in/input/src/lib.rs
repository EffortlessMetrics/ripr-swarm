pub fn weight(input: i32) -> i32 {
    input * 2
}

mod helpers;

#[cfg(test)]
mod tests {
    use super::*;
    use helpers::*;

    #[test]
    fn checks() {
        pretty_assertions::assert_eq!(weight(4), 12);
    }
}
