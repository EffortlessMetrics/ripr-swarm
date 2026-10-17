pub fn backoff_ms(attempt: u32, base: u64) -> u64 { base * 2u64.pow(attempt) }
pub fn retry<T, F: FnMut(u32) -> Result<T, String>>(max: u32, mut f: F) -> Result<T, String> {
    let mut last = String::from("no attempts");
    for i in 0..max { match f(i) { Ok(v) => return Ok(v), Err(e) => last = e } }
    Err(last)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn first_attempt_is_base() { assert_eq!(backoff_ms(0, 100), 100); }
    #[test] fn grows() { assert!(backoff_ms(3, 100) > backoff_ms(1, 100)); }
    #[test] fn retry_succeeds() { assert_eq!(retry(3, |i| if i == 1 { Ok(i) } else { Err("x".into()) }), Ok(1)); }
    #[test] fn retry_fails() { assert!(retry::<u32, _>(2, |_| Err("boom".into())).is_err()); }
}
