//! Bounded, allowlisted host memory observations; these do not diagnose OOM.
use serde_json::{Value, json};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const MAX_BYTES: u64 = 16 * 1024;
const FIELDS: [&str; 4] = ["MemTotal", "MemAvailable", "SwapTotal", "SwapFree"];

pub(super) fn observe(path: &Path) -> Value {
    match File::open(path) {
        Ok(file) => read(file),
        Err(_) => json!({"status": "unavailable", "reason": "open_failed"}),
    }
}

fn read(reader: impl Read) -> Value {
    let mut bytes = Vec::new();
    if reader.take(MAX_BYTES + 1).read_to_end(&mut bytes).is_err() {
        return json!({"status": "unavailable", "reason": "read_failed"});
    }
    if bytes.len() > MAX_BYTES as usize {
        return json!({"status": "truncated", "reason": "byte_limit"});
    }
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return json!({"status": "unavailable", "reason": "invalid_encoding"});
    };
    parse(text)
}

fn parse(text: &str) -> Value {
    let mut values = [None; 4];
    for line in text.lines() {
        let Some((name, rest)) = line.split_once(':') else {
            if line
                .split_whitespace()
                .next()
                .is_some_and(|name| FIELDS.contains(&name))
            {
                return json!({"status": "unavailable", "reason": "malformed_field"});
            }
            continue;
        };
        let Some(index) = FIELDS.iter().position(|field| *field == name) else {
            if name
                .split_whitespace()
                .next()
                .is_some_and(|field| FIELDS.contains(&field))
            {
                return json!({"status": "unavailable", "reason": "malformed_field"});
            }
            continue;
        };
        let mut parts = rest.split_whitespace();
        let value = parts.next().and_then(|v| v.parse::<u64>().ok());
        if values[index].is_some()
            || value.is_none()
            || parts.next() != Some("kB")
            || parts.next().is_some()
        {
            return json!({"status": "unavailable", "reason": "malformed_field"});
        }
        values[index] = value;
    }
    if values.iter().any(Option::is_none) {
        return json!({"status": "unavailable", "reason": "missing_field"});
    }
    json!({"status": "available", "unit": "KiB", "mem_total": values[0], "mem_available": values[1], "swap_total": values[2], "swap_free": values[3]})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allowlisted_values_and_negative_inputs() {
        let input = "MemTotal: 100 kB\nMemAvailable: 30 kB\nSwapTotal: 20 kB\nSwapFree: 5 kB\nPrivate: secret\n";
        let value = read(input.as_bytes());
        assert_eq!(value["status"], "available");
        assert_eq!(value["mem_available"], 30);
        assert_eq!(value["swap_free"], 5);
        assert!(!value.to_string().contains("secret"));
        assert_eq!(
            read(b"MemTotal: 1 kB\n".as_slice())["reason"],
            "missing_field"
        );
        assert_eq!(
            read(input.replace("30 kB", "bad kB").as_bytes())["reason"],
            "malformed_field"
        );
        assert_eq!(
            read(format!("{input}MemAvailable 99 kB\n").as_bytes())["reason"],
            "malformed_field"
        );
        assert_eq!(
            read(format!("{input}MemAvailable : 99 kB\n").as_bytes())["reason"],
            "malformed_field"
        );
        assert_eq!(
            read(format!("{input}MemAvailable 99 kB: junk\n").as_bytes())["reason"],
            "malformed_field"
        );
        assert_eq!(
            read(format!("{input}MemTotal: 2 kB\n").as_bytes())["reason"],
            "malformed_field"
        );
        assert_eq!(
            read(vec![b'x'; MAX_BYTES as usize + 1].as_slice())["status"],
            "truncated"
        );
        assert_eq!(
            observe(Path::new("/nonexistent-ripr-memory-source"))["status"],
            "unavailable"
        );
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::Other.into())
            }
        }
        assert_eq!(read(Broken)["reason"], "read_failed");
    }
}
