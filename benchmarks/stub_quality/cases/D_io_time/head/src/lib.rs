use std::path::Path;
use std::time::{Duration, Instant};

/// Read a port from a file; privileged ports (<1024) fall back to 8080.
pub fn load_port(path: &Path) -> std::io::Result<u16> {
    let s = std::fs::read_to_string(path)?;
    let p: u16 = s.trim().parse().map_err(|_| std::io::Error::other("bad port"))?;
    Ok(if p < 1024 { 8080 } else { p })
}

/// Exponential backoff, exponent capped.
pub fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(100 * 2u64.pow(attempt.min(6)))
}

pub fn expired(start: Instant, ttl: Duration) -> bool {
    start.elapsed() > ttl
}

fn env_or(key: &str, d: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| d.to_string())
}

pub fn region() -> String {
    env_or("DEMO_REGION", "us-east")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tmp(name: &str, body: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("io_time_{}_{}", std::process::id(), name));
        std::fs::write(&p, body).unwrap();
        p
    }
    #[test]
    fn ports() {
        assert_eq!(load_port(&tmp("a", "80\n")).unwrap(), 8080);
        assert_eq!(load_port(&tmp("b", "9000")).unwrap(), 9000);
        assert!(load_port(Path::new("/nonexistent/x")).is_err());
    }
    #[test]
    fn backoffs() {
        assert_eq!(backoff(1), Duration::from_millis(200));
        assert_eq!(backoff(2), Duration::from_millis(400));
    }
    #[test]
    fn fresh_not_expired() {
        assert!(!expired(Instant::now(), Duration::from_secs(60)));
    }
    #[test]
    fn default_region() {
        let _ = region();
    }
}
