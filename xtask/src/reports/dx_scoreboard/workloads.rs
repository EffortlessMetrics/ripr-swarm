//! Cross-report workload pairing for the built-in corpus measurements.
//!
//! A display ID selects the row; the retained corpus identity decides whether
//! its numeric samples describe the same workload. Other receipt sources keep
//! their own admission and population contracts.

use super::{CorpusEntry, MetricDef, RunContext, validate_corpus};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct WorkloadPairs {
    eligible: Option<BTreeSet<String>>,
    excluded: Vec<Value>,
}

impl WorkloadPairs {
    pub(super) fn enforced(&self) -> bool {
        self.eligible.is_some()
    }

    pub(super) fn includes(&self, repo: &str) -> bool {
        self.eligible.as_ref().is_none_or(|ids| ids.contains(repo))
    }

    pub(super) fn retain_exclusions(&self, mut comparison: Value) -> Value {
        if !self.excluded.is_empty() {
            comparison["uncompared_repos"] = json!(self.excluded);
        }
        comparison
    }
}

#[derive(PartialEq, Eq)]
struct WorkloadIdentity {
    url: String,
    sha: String,
    base: String,
    language: String,
    expected_partial: Option<String>,
}

fn required_text<'a>(record: &'a Value, key: &str) -> Result<&'a str, String> {
    record[key]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| format!("corpus identity has missing or invalid `{key}`"))
}

fn valid_url_escapes(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return false;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    true
}

fn valid_url_authority(authority: &str) -> bool {
    if authority.matches('@').count() > 1 {
        return false;
    }
    let host_port = authority.rsplit('@').next().unwrap_or("");
    if authority.chars().any(char::is_whitespace) || host_port.is_empty() {
        return false;
    }
    let valid_port = |port: &str| !port.is_empty() && port.parse::<u16>().is_ok();
    if let Some(ip) = host_port.strip_prefix('[') {
        let Some((address, suffix)) = ip.split_once(']') else {
            return false;
        };
        let address = match address.split_once("%25") {
            Some((address, zone)) if !zone.is_empty() => address,
            Some(_) => return false,
            None => address,
        };
        return address.parse::<std::net::Ipv6Addr>().is_ok()
            && (suffix.is_empty() || suffix.strip_prefix(':').is_some_and(valid_port));
    }
    let (host, port) = host_port
        .split_once(':')
        .map_or((host_port, None), |(h, p)| (h, Some(p)));
    !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_alphanumeric() || "-._%".contains(c))
        && valid_url_escapes(host)
        && port.is_none_or(valid_port)
}

// Git accepts local paths and scp-style remotes. For hierarchical URLs,
// validate the authority; a server root can itself serve a repository. File
// URL paths retain literal characters accepted by Git, including spaces.
fn usable_repository_url(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return true;
    };
    if scheme.is_empty()
        || !scheme.as_bytes()[0].is_ascii_alphabetic()
        || !scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+-.".contains(&b))
    {
        return false;
    }
    if scheme.eq_ignore_ascii_case("file") {
        let Some((authority, _)) = rest.split_once('/') else {
            return false;
        };
        return authority.is_empty() || valid_url_authority(authority);
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    !rest.chars().any(char::is_whitespace)
        && valid_url_escapes(rest)
        && valid_url_authority(authority)
}

fn identity(record: &Value, repo: &str) -> Result<WorkloadIdentity, String> {
    let url = required_text(record, "url")?;
    if !usable_repository_url(url) {
        return Err("corpus identity has missing or invalid `url`".to_string());
    }
    let sha = required_text(record, "sha")?;
    let base = required_text(record, "base")?;
    let language = required_text(record, "language")?;
    // Absence is not the same as an explicitly complete-only contract.
    let expected_partial = match record.get("expected_partial") {
        Some(Value::Null) => None,
        Some(Value::String(limit)) => Some(limit.clone()),
        _ => return Err("corpus identity has missing or invalid `expected_partial`".to_string()),
    };
    // Reuse the corpus owner's language, full-SHA and typed-limit rules.
    // HEAD~1 is the sole implicit-base recipe, bound to the fixed head SHA.
    let entry = CorpusEntry {
        id: repo.to_string(),
        url: url.to_string(),
        sha: sha.to_string(),
        base_sha: (base != "HEAD~1").then(|| base.to_string()),
        language: Some(language.to_string()),
        expected_partial: expected_partial.clone(),
        note: String::new(),
        heavy: false,
        ceilings: BTreeMap::new(),
        size_tier: None,
        memory_ratchet: None,
    };
    validate_corpus(std::slice::from_ref(&entry))?;
    Ok(WorkloadIdentity {
        url: url.to_string(),
        sha: sha.to_ascii_lowercase(),
        base: if base == "HEAD~1" {
            base.to_string()
        } else {
            base.to_ascii_lowercase()
        },
        language: language.to_string(),
        expected_partial,
    })
}

fn unique_identity(corpus: &[Value], repo: &str) -> Result<WorkloadIdentity, String> {
    let mut records = corpus.iter().filter(|entry| entry["id"] == repo);
    let record = records
        .next()
        .ok_or("corpus identity is missing for this repository")?;
    if records.next().is_some() {
        return Err("corpus identity is duplicated for this repository".to_string());
    }
    identity(record, repo)
}

fn sample_repos(row: &Value) -> BTreeSet<&str> {
    row["samples"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|sample| sample["repo"].as_str())
        .collect()
}

pub(super) fn pair(
    def: &MetricDef,
    row: &Value,
    base_row: &Value,
    context: &RunContext,
    baseline: &Value,
) -> WorkloadPairs {
    if def.source != "measured" || !def.per_repo {
        return WorkloadPairs::default();
    }
    let baseline_corpus = baseline["corpus"].as_array().map_or(&[][..], Vec::as_slice);
    let current_repos = sample_repos(row);
    let baseline_repos = sample_repos(base_row);
    let mut eligible = BTreeSet::new();
    let mut excluded = Vec::new();
    for repo in current_repos.intersection(&baseline_repos) {
        let before =
            unique_identity(baseline_corpus, repo).map_err(|reason| format!("baseline {reason}"));
        let current =
            unique_identity(&context.corpus, repo).map_err(|reason| format!("current {reason}"));
        let reason = match (before, current) {
            (Ok(before), Ok(current)) if before == current => {
                eligible.insert((*repo).to_string());
                continue;
            }
            (Ok(before), Ok(current)) => {
                let changed: Vec<&str> = [
                    ("url", before.url != current.url),
                    ("sha", before.sha != current.sha),
                    ("base", before.base != current.base),
                    ("language", before.language != current.language),
                    (
                        "expected_partial",
                        before.expected_partial != current.expected_partial,
                    ),
                ]
                .into_iter()
                .filter_map(|(field, changed)| changed.then_some(field))
                .collect();
                format!("workload differs: {}", changed.join(", "))
            }
            (Err(reason), _) | (_, Err(reason)) => reason,
        };
        excluded.push(json!({"repo": repo, "reason": reason}));
    }
    WorkloadPairs {
        eligible: Some(eligible),
        excluded,
    }
}

pub(super) fn comparison_note(baseline: &Value) -> String {
    let reasons: Vec<String> = baseline["uncompared_repos"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(format!(
                "{} ({})",
                entry["repo"].as_str()?,
                entry["reason"].as_str()?
            ))
        })
        .collect();
    if reasons.is_empty() {
        String::new()
    } else {
        format!("; workload not compared: {}", reasons.join("; "))
    }
}
