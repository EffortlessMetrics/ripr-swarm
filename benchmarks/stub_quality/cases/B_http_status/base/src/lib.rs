use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    NotFound,
    TooManyRequests,
    InternalError,
    BadGateway,
    ServiceUnavailable,
    GatewayTimeout,
}

impl Status {
    pub fn from_code(code: u16) -> Option<Status> {
        match code {
            200 => Some(Status::Ok),
            404 => Some(Status::NotFound),
            429 => Some(Status::TooManyRequests),
            500 => Some(Status::InternalError),
            502 => Some(Status::BadGateway),
            503 => Some(Status::ServiceUnavailable),
            504 => Some(Status::GatewayTimeout),
            _ => None,
        }
    }

    pub fn code(&self) -> u16 {
        match self {
            Status::Ok => 200,
            Status::NotFound => 404,
            Status::TooManyRequests => 429,
            Status::InternalError => 500,
            Status::BadGateway => 502,
            Status::ServiceUnavailable => 503,
            Status::GatewayTimeout => 504,
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Status::TooManyRequests
                | Status::BadGateway
                | Status::ServiceUnavailable
                | Status::GatewayTimeout
        )
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Status::Ok => "OK",
            Status::NotFound => "Not Found",
            Status::TooManyRequests => "Too Many Requests",
            Status::InternalError => "Internal Server Error",
            Status::BadGateway => "Bad Gateway",
            Status::ServiceUnavailable => "Service Unavailable",
            Status::GatewayTimeout => "Gateway Timeout",
        };
        write!(f, "{} {}", self.code(), text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_code() {
        assert_eq!(Status::from_code(404), Some(Status::NotFound));
        assert_eq!(Status::from_code(999), None);
        assert_eq!(Status::NotFound.code(), 404);
    }

    #[test]
    fn retryable() {
        assert!(Status::ServiceUnavailable.is_retryable());
        assert!(!Status::Ok.is_retryable());
    }

    #[test]
    fn display_ok() {
        assert_eq!(Status::Ok.to_string(), "200 OK");
    }
}
