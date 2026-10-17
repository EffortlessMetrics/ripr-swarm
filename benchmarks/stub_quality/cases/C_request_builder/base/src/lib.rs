#[derive(Debug, PartialEq)]
pub struct Request { pub url: String, pub timeout_ms: u64, pub retries: u8, pub headers: Vec<(String, String)> }
#[derive(Default)]
pub struct RequestBuilder { url: String, timeout_ms: Option<u64>, retries: Option<u8>, headers: Vec<(String, String)> }
impl RequestBuilder {
    pub fn new(url: &str) -> Self { RequestBuilder { url: url.to_string(), ..Default::default() } }
    pub fn timeout_ms(mut self, t: u64) -> Self { self.timeout_ms = Some(t); self }
    pub fn retries(mut self, r: u8) -> Self { self.retries = Some(r); self }
    pub fn header(mut self, k: &str, v: &str) -> Self { self.headers.push((k.into(), v.into())); self }
    pub fn build(self) -> Request {
        Request { url: self.url, timeout_ms: self.timeout_ms.unwrap_or(30_000), retries: self.retries.unwrap_or(3), headers: self.headers }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn explicit_values() { let r = RequestBuilder::new("http://x").timeout_ms(500).retries(1).build(); assert_eq!(r.timeout_ms, 500); assert_eq!(r.retries, 1); }
    #[test] fn url_kept() { assert_eq!(RequestBuilder::new("http://x").build().url, "http://x"); }
    #[test] fn headers() { let r = RequestBuilder::new("u").header("a", "b").build(); assert_eq!(r.headers.len(), 1); }
}
