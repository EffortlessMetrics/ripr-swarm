#[derive(Debug, PartialEq)]
pub struct Request {
    pub url: String,
    pub timeout_ms: u64,
}

pub struct RequestBuilder {
    url: String,
    timeout_ms: Option<u64>,
}

impl RequestBuilder {
    pub fn new(url: &str) -> Self {
        RequestBuilder { url: url.to_string(), timeout_ms: None }
    }

    pub fn timeout_ms(mut self, t: u64) -> Self {
        self.timeout_ms = Some(t);
        self
    }

    pub fn build(self) -> Request {
        Request { url: self.url, timeout_ms: self.timeout_ms.unwrap_or(3_000) }
    }
}
