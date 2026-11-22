//! Owner release authorization as a machine object (ripr#1650).
//!
//! An owner ruling written as issue prose and a tag-push event are different
//! objects. This module owns the first piece of the bridge (ripr#1651): one
//! canonical `ReleaseAuthorizationPacket` and the comment body that carries
//! it.
//!
//! Nothing here reads GitHub, binds a packet to a Git event, creates or pushes
//! a ref, posts a comment, requests an environment or credential, or
//! publishes.

use std::path::PathBuf;

use crate::exact_schema_json::Value;

mod release_authorization_packet;

pub(crate) const USAGE: &str = "\
cargo xtask release-authority render-packet --input <packet-draft.json> [--out <dir>]";

const DEFAULT_OUT: &str = "target/ripr/reports/release-authority";

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    match args.split_first() {
        Some((sub, rest)) if sub == "render-packet" => render_packet(rest),
        Some((flag, [])) if flag == "--help" || flag == "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(USAGE.to_string()),
    }
}

/// Closed `--flag value` reader: an unknown or repeated flag is refused.
struct Flags<'a> {
    pairs: Vec<(&'a str, &'a str)>,
}

impl<'a> Flags<'a> {
    fn parse(args: &'a [String], known: &[&str]) -> Result<Self, String> {
        let mut pairs: Vec<(&str, &str)> = Vec::new();
        let mut rest = args.iter();
        while let Some(flag) = rest.next() {
            if !known.contains(&flag.as_str()) {
                return Err(format!("unknown argument `{flag}`\n{USAGE}"));
            }
            if pairs.iter().any(|(seen, _)| seen == flag) {
                return Err(format!("`{flag}` was given twice\n{USAGE}"));
            }
            let value = rest
                .next()
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| format!("`{flag}` requires a value\n{USAGE}"))?;
            pairs.push((flag, value));
        }
        Ok(Self { pairs })
    }

    fn get(&self, flag: &str) -> Option<&'a str> {
        self.pairs
            .iter()
            .find(|(seen, _)| *seen == flag)
            .map(|(_, value)| *value)
    }

    fn required(&self, flag: &str) -> Result<&'a str, String> {
        self.get(flag)
            .ok_or_else(|| format!("`{flag}` is required\n{USAGE}"))
    }
}

fn read_bounded(path: &str) -> Result<Vec<u8>, String> {
    const MAX_INPUT: u64 = 1024 * 1024;
    let info = std::fs::metadata(path).map_err(|e| format!("read {path}: {e}"))?;
    if !info.is_file() || info.len() > MAX_INPUT {
        return Err(format!("{path} must be a regular file of at most 1 MiB"));
    }
    std::fs::read(path).map_err(|e| format!("read {path}: {e}"))
}

fn render_packet(args: &[String]) -> Result<(), String> {
    let flags = Flags::parse(args, &["--input", "--out"])?;
    let draft = read_bounded(flags.required("--input")?)?;
    let rendered = release_authorization_packet::render_from_draft(&draft)?;
    let out = PathBuf::from(flags.get("--out").unwrap_or(DEFAULT_OUT));
    let packet_text = String::from_utf8_lossy(&rendered.canonical).into_owned();
    crate::write_report_in(&out, "authorization-packet.json", &packet_text)?;
    crate::write_report_in(&out, "authorization-comment.md", &rendered.comment)?;
    println!("authorization_id: {}", rendered.authorization_id);
    println!(
        "comment body sha256 as rendered: {}",
        sha256_hex(rendered.comment.as_bytes())
    );
    println!(
        "Wrote {0}/authorization-packet.json and {0}/authorization-comment.md",
        out.display()
    );
    println!(
        "Nothing was posted, and a rendered packet authorizes nothing. Posting the comment is \
         a separate action of the authorized maintainer account. GitHub may store other bytes \
         than these, so the digest of the posted body is read back from GitHub."
    );
    Ok(())
}

// ----- shared canonical-form helpers -----

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    crate::blind_journey::sha256_hex(bytes)
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

/// Parses bytes that must already be the one canonical JSON form: keys in
/// byte order, no insignificant whitespace, the shortest standard string
/// escapes. A duplicate key, reordered key, padded document or alternate
/// escape re-serializes to different bytes and is refused.
pub(crate) fn parse_canonical(bytes: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("packet is not UTF-8: {e}"))?;
    let value = crate::exact_schema_json::parse(bytes)
        .map_err(|e| format!("packet is not one JSON document: {e}"))?;
    if value.to_string() != text {
        return Err(
            "packet bytes are not canonical JSON (duplicate or unordered keys, whitespace, or a \
             nonstandard escape)"
                .to_string(),
        );
    }
    require_plain(&value, "$")?;
    Ok(value)
}

/// Only objects, arrays, printable-ASCII strings, plain unsigned integers,
/// booleans and explicit nulls have one unambiguous canonical spelling here.
fn require_plain(value: &Value, path: &str) -> Result<(), String> {
    match value {
        Value::Null | Value::Bool(_) => Ok(()),
        Value::Number(text) => parse_plain_u64(text)
            .map(|_| ())
            .ok_or_else(|| format!("{path}: `{text}` is not a plain unsigned integer")),
        Value::String(text) => require_printable(text, path),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .try_for_each(|(index, item)| require_plain(item, &format!("{path}[{index}]"))),
        Value::Object(map) => map.iter().try_for_each(|(key, item)| {
            require_printable(key, path)?;
            require_plain(item, &format!("{path}.{key}"))
        }),
    }
}

fn require_printable(text: &str, path: &str) -> Result<(), String> {
    if text.bytes().all(|byte| (0x20..0x7f).contains(&byte)) {
        Ok(())
    } else {
        Err(format!("{path}: only printable ASCII text is canonical"))
    }
}

/// `0` or a digit run without a leading zero, sign, fraction or exponent.
pub(crate) fn parse_plain_u64(text: &str) -> Option<u64> {
    let plain = text == "0" || (!text.starts_with('0') && !text.is_empty());
    if plain && text.bytes().all(|byte| byte.is_ascii_digit()) {
        text.parse().ok()
    } else {
        None
    }
}

/// Seconds since the Unix epoch for exactly `YYYY-MM-DDTHH:MM:SSZ`.
pub(crate) fn parse_utc_seconds(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    let shape = bytes.len() == 20
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            10 => *byte == b'T',
            13 | 16 => *byte == b':',
            19 => *byte == b'Z',
            _ => byte.is_ascii_digit(),
        });
    if !shape {
        return None;
    }
    let field = |range: std::ops::Range<usize>| text.get(range)?.parse::<u64>().ok();
    let (year, month, day) = (field(0..4)?, field(5..7)?, field(8..10)?);
    let (hour, minute, second) = (field(11..13)?, field(14..16)?, field(17..19)?);
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let month_days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if year < 1970 || day == 0 || day > month_days || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    // Days from 1970-01-01 (Howard Hinnant's civil-from-days inverse).
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = shifted_year / 400;
    let year_of_era = shifted_year - era * 400;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = (era * 146_097 + day_of_era).checked_sub(719_468)?;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// A value shaped like a credential. Identifier names, 40/64-hex object and
/// digest identities and ordinary prose do not match.
pub(crate) fn secret_shaped(text: &str) -> Option<&'static str> {
    const PREFIXES: &[(&str, &str)] = &[
        ("ghp_", "github_token"),
        ("gho_", "github_token"),
        ("ghu_", "github_token"),
        ("ghs_", "github_token"),
        ("ghr_", "github_token"),
        ("github_pat_", "github_token"),
        ("-----BEGIN", "pem_block"),
        ("AKIA", "cloud_access_key"),
        ("npm_", "npm_token"),
        ("eyJ", "jwt"),
    ];
    for (prefix, kind) in PREFIXES {
        let followed_by_token = text.match_indices(prefix).any(|(start, _)| {
            text.get(start + prefix.len()..).is_some_and(|rest| {
                rest.bytes()
                    .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                    .count()
                    >= 16
                    || *prefix == "-----BEGIN"
            })
        });
        if followed_by_token {
            return Some(kind);
        }
    }
    let opaque_run = text
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|run| run.len() >= 40 && !run.bytes().all(|byte| byte.is_ascii_hexdigit()));
    opaque_run.then_some("opaque_high_entropy_run")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_authorization_packet_canonical_form_is_unique() -> Result<(), String> {
        parse_canonical(br#"{"a":1,"b":["x",null,true]}"#)?;
        // A nonstandard escape for an ordinary character: backslash, u0041.
        let escaped = format!("{{\"a\":\"{}u0041\"}}", '\\');
        for noncanonical in [
            &br#"{"b":1,"a":1}"#[..],
            br#"{"a":1,"a":1}"#,
            br#"{"a": 1}"#,
            b"{\"a\":1}\n",
            escaped.as_bytes(),
            br#"{"a":1.0}"#,
            br#"{"a":-1}"#,
            br#"{"a":1e3}"#,
            b"{\"a\":\"caf\xc3\xa9\"}",
        ] {
            if parse_canonical(noncanonical).is_ok() {
                return Err(format!(
                    "accepted noncanonical bytes {}",
                    String::from_utf8_lossy(noncanonical)
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn release_authorization_packet_utc_seconds_are_exact() -> Result<(), String> {
        let cases = [
            ("1970-01-01T00:00:00Z", Some(0)),
            ("2000-03-01T00:00:00Z", Some(951_868_800)),
            ("2024-02-29T12:00:00Z", Some(1_709_208_000)),
            ("2026-11-21T21:13:31Z", Some(1_795_295_611)),
            ("2026-02-29T00:00:00Z", None),
            ("2026-11-21T24:00:00Z", None),
            ("2026-11-21 21:13:31Z", None),
            ("2026-11-21T21:13:31+00:00", None),
        ];
        for (text, expected) in cases {
            if parse_utc_seconds(text) != expected {
                return Err(format!("{text} parsed as {:?}", parse_utc_seconds(text)));
            }
        }
        Ok(())
    }

    #[test]
    fn release_authorization_packet_secret_shapes_are_named() -> Result<(), String> {
        let digest = format!("sha256:{}", "ab12".repeat(16));
        for clean in ["CARGO_REGISTRY_TOKEN", digest.as_str(), "release-crates-io"] {
            if secret_shaped(clean).is_some() {
                return Err(format!("`{clean}` was treated as a credential"));
            }
        }
        let token = format!("ghp_{}", "A1b2".repeat(9));
        let opaque = "Zx9".repeat(14);
        for secret in [token.as_str(), opaque.as_str(), "-----BEGIN PRIVATE KEY"] {
            if secret_shaped(secret).is_none() {
                return Err("a credential-shaped value was not detected".to_string());
            }
        }
        Ok(())
    }
}
