//! Compile-time distinct analysis execution, completed snapshot, diagnostic
//! result, and continuation identities (#4807 / ID04).
//!
//! Wire spellings stay compatible with #4804; these wrappers reject cross-class
//! aliases at parse time.

use std::fmt;

/// One refresh/analysis execution echoed on agent surfaces as `snapshot_id`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AnalysisAttemptId(String);

/// Immutable completed analysis meaning for one snapshot handle.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompletedAnalysisSnapshotId(String);

/// One bounded standard-LSP diagnostic projection for a completed snapshot.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticResultId(String);

/// One deterministic pagination/continuation page over an immutable snapshot.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContinuationId(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnalysisExecutionIdentityParseError {
    Empty,
    WrongClass {
        value: String,
        expected: &'static str,
    },
}

impl fmt::Display for AnalysisExecutionIdentityParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("identity value must not be empty"),
            Self::WrongClass { value, expected } => {
                write!(f, "value `{value}` is not a {expected}")
            }
        }
    }
}

fn nonempty_trimmed(
    value: &str,
    _label: &'static str,
) -> Result<String, AnalysisExecutionIdentityParseError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AnalysisExecutionIdentityParseError::Empty);
    }
    Ok(trimmed.to_string())
}

fn is_refresh_generation_echo(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("snapshot:") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit())
}

fn is_completed_snapshot_handle(value: &str) -> bool {
    if is_refresh_generation_echo(value) {
        return false;
    }
    value.starts_with("snapshot:sha256:")
        || value.starts_with("snapshot:input:")
        || (value.starts_with("snapshot:") && value.matches(':').count() >= 2)
}

impl AnalysisAttemptId {
    pub fn from_refresh_generation(generation: u64) -> Self {
        Self(format!("snapshot:{generation}"))
    }

    pub fn parse_agent_snapshot_id(
        value: &str,
    ) -> Result<Self, AnalysisExecutionIdentityParseError> {
        let trimmed = nonempty_trimmed(value, "AnalysisAttemptId")?;
        let generation = trimmed.strip_prefix("snapshot:").ok_or_else(|| {
            AnalysisExecutionIdentityParseError::WrongClass {
                value: trimmed.clone(),
                expected: "refresh-generation AnalysisAttemptId (`snapshot_id` echo)",
            }
        })?;
        if generation.is_empty() {
            return Err(AnalysisExecutionIdentityParseError::Empty);
        }
        if generation.parse::<u64>().is_err() {
            return Err(AnalysisExecutionIdentityParseError::WrongClass {
                value: trimmed,
                expected: "refresh-generation AnalysisAttemptId (`snapshot:{generation}`)",
            });
        }
        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl CompletedAnalysisSnapshotId {
    pub fn parse(value: &str) -> Result<Self, AnalysisExecutionIdentityParseError> {
        let trimmed = nonempty_trimmed(value, "CompletedAnalysisSnapshotId")?;
        if !is_completed_snapshot_handle(&trimmed) {
            return Err(AnalysisExecutionIdentityParseError::WrongClass {
                value: trimmed,
                expected: "CompletedAnalysisSnapshotId (completed snapshot handle)",
            });
        }
        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl DiagnosticResultId {
    pub fn from_producer_id(value: &str) -> Result<Self, AnalysisExecutionIdentityParseError> {
        let trimmed = nonempty_trimmed(value, "DiagnosticResultId")?;
        let Some((prefix, digest)) = trimmed.split_once(':') else {
            return Err(AnalysisExecutionIdentityParseError::WrongClass {
                value: trimmed,
                expected: "DiagnosticResultId (`<prefix>:<digest>`)",
            });
        };
        let valid_prefix = matches!(
            prefix,
            "ripr-document-diagnostics-v2" | "ripr-workspace-diagnostics-v2"
        );
        let valid_digest = digest.len() == 16 && digest.chars().all(|ch| ch.is_ascii_hexdigit());
        if !(valid_prefix && valid_digest) {
            return Err(AnalysisExecutionIdentityParseError::WrongClass {
                value: trimmed,
                expected: "DiagnosticResultId",
            });
        }
        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl ContinuationId {
    pub fn parse_request(value: &str) -> Result<Self, AnalysisExecutionIdentityParseError> {
        Self::parse_shared(value, "continuation_id")
    }

    pub fn parse_success_alias(value: &str) -> Result<Self, AnalysisExecutionIdentityParseError> {
        Self::parse_shared(value, "continuation_identity")
    }

    fn parse_shared(
        value: &str,
        label: &'static str,
    ) -> Result<Self, AnalysisExecutionIdentityParseError> {
        let trimmed = nonempty_trimmed(value, "ContinuationId")?;
        if trimmed.starts_with("snapshot:")
            || trimmed.starts_with("gap:")
            || trimmed.starts_with("action:")
            || trimmed.starts_with("instruction-")
            || trimmed.starts_with("ripr-document-diagnostics-v2:")
            || trimmed.starts_with("ripr-workspace-diagnostics-v2:")
        {
            return Err(AnalysisExecutionIdentityParseError::WrongClass {
                value: trimmed,
                expected: label,
            });
        }
        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_attempts_same_completed_semantics_have_distinct_attempt_ids() -> Result<(), String> {
        let first = AnalysisAttemptId::from_refresh_generation(3);
        let second = AnalysisAttemptId::from_refresh_generation(4);
        assert_ne!(first, second);
        let completed = "snapshot:sha256:deadbeef";
        let a = CompletedAnalysisSnapshotId::parse(completed).map_err(|e| e.to_string())?;
        let b = CompletedAnalysisSnapshotId::parse(completed).map_err(|e| e.to_string())?;
        assert_eq!(a, b);
        Ok(())
    }

    #[test]
    fn failed_attempt_echo_does_not_parse_as_completed_snapshot() {
        assert_eq!(CompletedAnalysisSnapshotId::parse("snapshot:9").ok(), None);
        assert!(
            AnalysisAttemptId::parse_agent_snapshot_id("snapshot:9")
                .ok()
                .is_some()
        );
    }

    #[test]
    fn completed_snapshot_rejects_attempt_echo() {
        assert_eq!(CompletedAnalysisSnapshotId::parse("snapshot:42").ok(), None);
    }

    #[test]
    fn diagnostic_result_rejects_snapshot_and_attempt_handles() {
        assert_eq!(
            DiagnosticResultId::from_producer_id("snapshot:1").ok(),
            None
        );
        assert_eq!(
            DiagnosticResultId::from_producer_id("snapshot:sha256:abc").ok(),
            None
        );
    }

    #[test]
    fn attempt_id_requires_numeric_refresh_generation() {
        assert_eq!(
            AnalysisAttemptId::parse_agent_snapshot_id("snapshot:abc").ok(),
            None
        );
        assert_eq!(
            AnalysisAttemptId::parse_agent_snapshot_id("snapshot:").ok(),
            None
        );
        assert!(
            AnalysisAttemptId::parse_agent_snapshot_id("snapshot:9")
                .ok()
                .is_some()
        );
    }

    #[test]
    fn continuation_rejects_diagnostic_result_prefix() {
        assert_eq!(
            ContinuationId::parse_request("ripr-document-diagnostics-v2:0123456789abcdef").ok(),
            None
        );
    }

    #[test]
    fn continuation_rejects_snapshot_rebind() {
        assert_eq!(
            ContinuationId::parse_request("snapshot:sha256:abc").ok(),
            None
        );
    }

    #[test]
    fn attempt_and_completed_types_reject_each_other_wire_values() {
        assert_eq!(
            AnalysisAttemptId::parse_agent_snapshot_id("snapshot:sha256:abc").ok(),
            None
        );
        assert_eq!(CompletedAnalysisSnapshotId::parse("snapshot:7").ok(), None);
    }

    #[test]
    fn diagnostic_result_accepts_producer_prefix() -> Result<(), String> {
        DiagnosticResultId::from_producer_id("ripr-document-diagnostics-v2:0123456789abcdef")
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn continuation_alias_and_request_share_parse() -> Result<(), String> {
        let token = "page:2:filter:exposed";
        let from_request = ContinuationId::parse_request(token).map_err(|e| e.to_string())?;
        let from_success = ContinuationId::parse_success_alias(token).map_err(|e| e.to_string())?;
        assert_eq!(from_request, from_success);
        Ok(())
    }
}
