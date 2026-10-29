pub fn backoff_ms(attempt: u32, base: u64) -> u64 {
    base * 3u64.pow(attempt)
}
