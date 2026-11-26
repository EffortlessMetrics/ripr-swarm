//! Repository-owned release authorizer policy (ripr#1651).
//!
//! The stable numeric GitHub user id is the identity. The login is a checked
//! currentness field. Nothing a packet or comment carries can add an entry.

use serde::Deserialize;

use super::release_authorization_packet::Profile;
use super::{digest, parse_utc_seconds};

pub(crate) const POLICY_PATH: &str = "policy/release-authorizers.toml";
pub(crate) const POLICY_SCHEMA: &str = "ripr-release-authorizers/v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPolicy {
    schema: String,
    review_owner: String,
    reason: String,
    created: String,
    review_after: String,
    repository: RawRepository,
    authorizer: Vec<RawAuthorizer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRepository {
    id: u64,
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAuthorizer {
    user_id: u64,
    login: String,
    profiles: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Authorizer {
    pub(crate) user_id: u64,
    pub(crate) login: String,
    pub(crate) profiles: Vec<Profile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuthorizerPolicy {
    pub(crate) digest: String,
    pub(crate) repository_id: u64,
    pub(crate) repository_name: String,
    /// First second after the `review_after` day; later verification refuses.
    pub(crate) review_deadline: u64,
    pub(crate) authorizers: Vec<Authorizer>,
}

impl AuthorizerPolicy {
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let invalid = |message: String| format!("{POLICY_PATH}: {message}");
        let raw: RawPolicy = toml::from_str(text).map_err(|e| invalid(e.to_string()))?;
        if raw.schema != POLICY_SCHEMA {
            return Err(invalid(format!("schema must be {POLICY_SCHEMA}")));
        }
        if raw.review_owner.trim().is_empty() || raw.reason.trim().is_empty() {
            return Err(invalid("review_owner and reason are required".to_string()));
        }
        let day = |label: &str, value: &str| {
            parse_utc_seconds(&format!("{value}T00:00:00Z"))
                .ok_or_else(|| invalid(format!("{label} must be a YYYY-MM-DD date")))
        };
        let created = day("created", &raw.created)?;
        let review_after = day("review_after", &raw.review_after)?;
        if review_after <= created {
            return Err(invalid("review_after must follow created".to_string()));
        }
        if raw.repository.id == 0 || raw.repository.name.split('/').count() != 2 {
            return Err(invalid(
                "repository needs a numeric id and an owner/name".to_string(),
            ));
        }
        if raw.authorizer.is_empty() {
            return Err(invalid("at least one authorizer is required".to_string()));
        }
        let mut authorizers: Vec<Authorizer> = Vec::new();
        for entry in raw.authorizer {
            let mut profiles = Vec::new();
            for name in &entry.profiles {
                let profile = Profile::parse(name)
                    .ok_or_else(|| invalid(format!("unknown profile `{name}`")))?;
                if profiles.contains(&profile) {
                    return Err(invalid(format!("profile `{name}` is listed twice")));
                }
                profiles.push(profile);
            }
            if entry.user_id == 0 || entry.login.trim().is_empty() || profiles.is_empty() {
                return Err(invalid(
                    "an authorizer needs user_id, login and at least one profile".to_string(),
                ));
            }
            if authorizers.iter().any(|seen| {
                seen.user_id == entry.user_id || seen.login.eq_ignore_ascii_case(&entry.login)
            }) {
                return Err(invalid(format!(
                    "authorizer {} ({}) is listed twice",
                    entry.user_id, entry.login
                )));
            }
            authorizers.push(Authorizer {
                user_id: entry.user_id,
                login: entry.login,
                profiles,
            });
        }
        Ok(Self {
            digest: digest(text.as_bytes()),
            repository_id: raw.repository.id,
            repository_name: raw.repository.name,
            review_deadline: review_after + 86_400,
            authorizers,
        })
    }

    pub(crate) fn authorizer(&self, user_id: u64) -> Option<&Authorizer> {
        self.authorizers
            .iter()
            .find(|authorizer| authorizer.user_id == user_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED: &str = include_str!("../../../policy/release-authorizers.toml");

    #[test]
    fn release_authorization_record_committed_policy_names_stable_user_ids() -> Result<(), String> {
        let policy = AuthorizerPolicy::parse(COMMITTED)?;
        if policy.repository_name != "EffortlessMetrics/ripr" || policy.repository_id == 0 {
            return Err("the policy must govern the public source repository".to_string());
        }
        if policy.authorizers.iter().any(|entry| entry.user_id == 0) {
            return Err("an authorizer without a stable user id was accepted".to_string());
        }
        Ok(())
    }

    #[test]
    fn release_authorization_record_policy_rejects_open_or_duplicate_entries() -> Result<(), String>
    {
        let edits: [(&str, &str); 6] = [
            (
                "schema = \"ripr-release-authorizers/v1\"",
                "schema = \"v2\"",
            ),
            ("user_id = 15812269", "user_id = 0"),
            ("[[authorizer]]", "extra = true\n\n[[authorizer]]"),
            ("\"public_rc\", ", "\"public_rc\", \"public_rc\", "),
            ("\"public_rc\", ", "\"anything\", "),
            (
                "review_after = \"2027-02-21\"",
                "review_after = \"2026-01-01\"",
            ),
        ];
        for (from, to) in edits {
            if !COMMITTED.contains(from) {
                return Err(format!("control edit `{from}` no longer applies"));
            }
            if AuthorizerPolicy::parse(&COMMITTED.replacen(from, to, 1)).is_ok() {
                return Err(format!("policy accepted `{to}`"));
            }
        }
        let doubled = format!(
            "{COMMITTED}\n[[authorizer]]\nuser_id = 15812269\nlogin = \"Other\"\nprofiles = [\"public_rc\"]\n"
        );
        if AuthorizerPolicy::parse(&doubled).is_ok() {
            return Err("one user id was accepted under two entries".to_string());
        }
        Ok(())
    }
}
