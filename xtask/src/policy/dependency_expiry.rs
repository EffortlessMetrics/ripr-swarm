use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const EXPIRY_MARKER: &str = "expires:";

pub(crate) fn check_dependency_suppression_expiry(path: &Path) -> Result<(), String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let today = utc_date_string();
    check_dependency_suppression_expiry_text(&text, &today)
}

fn check_dependency_suppression_expiry_text(text: &str, today: &str) -> Result<(), String> {
    let mut in_advisories = false;
    let mut in_ignore = false;
    let mut checked = 0usize;

    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_advisories = trimmed == "[advisories]";
            in_ignore = false;
            continue;
        }
        if in_advisories && trimmed.starts_with("ignore = [") {
            in_ignore = true;
            continue;
        }
        if in_advisories && in_ignore && trimmed == "]" {
            in_ignore = false;
            continue;
        }
        if !in_advisories || !in_ignore || !trimmed.starts_with('"') {
            continue;
        }

        let Some(id_end) = trimmed[1..].find('"') else {
            return Err(format!("deny.toml:{}: malformed advisory ignore entry", index + 1));
        };
        let id = &trimmed[1..id_end + 1];
        if !id.starts_with("RUSTSEC-") {
            continue;
        }
        checked += 1;

        let Some(marker_start) = trimmed.find(EXPIRY_MARKER) else {
            return Err(format!(
                "deny.toml:{}: advisory {id} is missing an `{EXPIRY_MARKER} YYYY-MM-DD` expiry",
                index + 1
            ));
        };
        let expiry = trimmed[marker_start + EXPIRY_MARKER.len()..]
            .trim()
            .get(..10)
            .ok_or_else(|| {
                format!(
                    "deny.toml:{}: advisory {id} has an invalid expiry",
                    index + 1
                )
            })?;
        if !is_iso_date(expiry) {
            return Err(format!(
                "deny.toml:{}: advisory {id} has invalid expiry `{expiry}`",
                index + 1
            ));
        }
        if expiry <= today {
            return Err(format!(
                "deny.toml:{}: advisory {id} expiry `{expiry}` is not after today `{today}`",
                index + 1
            ));
        }
    }

    if checked == 0 {
        return Err("deny.toml: [advisories].ignore contains no RUSTSEC entries".to_string());
    }
    Ok(())
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let Some(month) = parse_digits(bytes, 5, 7) else {
        return false;
    };
    let Some(day) = parse_digits(bytes, 8, 10) else {
        return false;
    };
    parse_digits(bytes, 0, 4).is_some() && (1..=12).contains(&month) && (1..=31).contains(&day)
}

fn parse_digits(bytes: &[u8], start: usize, end: usize) -> Option<u16> {
    let mut value = 0u16;
    for &byte in &bytes[start..end] {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + u16::from(byte - b'0');
    }
    Some(value)
}

fn utc_date_string() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        / 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524
        - day_of_era / 146_096)
        / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::check_dependency_suppression_expiry_text;

    #[test]
    fn accepts_future_expiry_for_each_advisory() {
        let text = "[advisories]\nignore = [\n  \"RUSTSEC-2025-0001\", # expires: 2026-10-01\n]\n";
        assert!(check_dependency_suppression_expiry_text(text, "2026-07-28").is_ok());
    }

    #[test]
    fn rejects_advisory_without_expiry() {
        let text = "[advisories]\nignore = [\n  \"RUSTSEC-2025-0001\"\n]\n";
        let result = check_dependency_suppression_expiry_text(text, "2026-07-28");
        assert!(result.is_err());
        assert!(result
            .err()
            .is_some_and(|message| message.contains("missing")));
    }

    #[test]
    fn rejects_expired_advisory() {
        let text = "[advisories]\nignore = [\n  \"RUSTSEC-2025-0001\", # expires: 2026-07-28\n]\n";
        let result = check_dependency_suppression_expiry_text(text, "2026-07-28");
        assert!(result.is_err());
        assert!(result
            .err()
            .is_some_and(|message| message.contains("not after today")));
    }
}
