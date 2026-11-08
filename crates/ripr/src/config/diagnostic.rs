//! Source coordinates for semantic `ripr.toml` validation failures.
//!
//! Coordinates are one-based lines and Unicode scalar columns. End positions
//! are exclusive. The span covers the TOML value, including its quotes, in the
//! original source. These are presentation coordinates, not LSP UTF-16 offsets.

use serde::Serialize;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigPosition {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigLocation {
    pub start: ConfigPosition,
    pub end: ConfigPosition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigLocationStatus {
    Exact,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigDiagnostic {
    pub kind: &'static str,
    pub config_path: Option<String>,
    pub file: &'static str,
    pub location_status: ConfigLocationStatus,
    pub location: Option<ConfigLocation>,
    pub invalid_value: Option<String>,
    pub expected_values: Vec<&'static str>,
    pub message: String,
}

/// Machine-readable semantic validation fields for doctor/JSON consumers (#2624).
///
/// Carries coordinates and expected values without embedding `ripr.toml` source
/// text (RIPR-SPEC-0007). Structural TOML parse failures do not populate this
/// shape; they keep the parser's native location in the human summary only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConfigValidationMachine {
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    pub file: &'static str,
    pub location_status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid_value: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_values: Vec<String>,
    pub message: String,
}

impl ConfigDiagnostic {
    /// First human line for a load error: `path:line:column: message` when the
    /// span is exact, otherwise `path: message`.
    pub(crate) fn human_first_line(&self, config_path: &str) -> String {
        let message = self.message.trim();
        if let Some(location) = &self.location {
            format!(
                "{}:{}:{}: {}",
                config_path, location.start.line, location.start.column, message
            )
        } else {
            format!("{config_path}: {message}")
        }
    }

    pub(crate) fn machine_validation(&self) -> Option<ConfigValidationMachine> {
        if self.kind != "config_validation" {
            return None;
        }
        let (start_line, start_column, end_line, end_column) = match &self.location {
            Some(location) => (
                Some(location.start.line),
                Some(location.start.column),
                Some(location.end.line),
                Some(location.end.column),
            ),
            None => (None, None, None, None),
        };
        Some(ConfigValidationMachine {
            kind: self.kind,
            config_path: self.config_path.clone(),
            file: self.file,
            location_status: match self.location_status {
                ConfigLocationStatus::Exact => "exact",
                ConfigLocationStatus::Unavailable => "unavailable",
            },
            start_line,
            start_column,
            end_line,
            end_column,
            invalid_value: self.invalid_value.clone(),
            expected_values: self
                .expected_values
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            message: self.message.clone(),
        })
    }

    pub(super) fn structural(message: String) -> Self {
        let mut diagnostic = Self::unavailable(message);
        diagnostic.kind = "config_syntax";
        diagnostic
    }

    pub(super) fn unavailable(message: String) -> Self {
        Self {
            kind: "config_validation",
            config_path: None,
            file: "ripr.toml",
            location_status: ConfigLocationStatus::Unavailable,
            location: None,
            invalid_value: None,
            expected_values: Vec::new(),
            message,
        }
    }

    pub(super) fn at_value(message: String, path: &str, span: Range<usize>, text: &str) -> Self {
        let location =
            (span.start <= span.end && text.get(span.clone()).is_some()).then(|| ConfigLocation {
                start: position(text, span.start),
                end: position(text, span.end),
            });
        let invalid_value = text.get(span).and_then(|value| {
            (value.len() <= 100 && !value.chars().any(char::is_control)).then(|| value.to_owned())
        });
        let expected_values = match path {
            "analysis.mode" => vec!["instant", "draft", "fast", "deep", "ready"],
            "oracles.snapshot_strength"
            | "oracles.mock_expectation_strength"
            | "oracles.broad_error_strength" => {
                vec!["strong", "medium", "weak", "smoke", "none", "unknown"]
            }
            "severity.findings.exposed"
            | "severity.findings.weakly_exposed"
            | "severity.findings.reachable_unrevealed"
            | "severity.findings.no_static_path"
            | "severity.findings.infection_unknown"
            | "severity.findings.propagation_unknown"
            | "severity.findings.static_unknown" => vec!["info", "warning", "note"],
            path if path.starts_with("severity.seams.") => vec!["off", "info", "warning", "note"],
            "lsp.diagnostic_profile" => vec!["actionable", "full"],
            "perl.producer" => vec!["perl-ripr-facts", "perllsp", "perl-lsp"],
            _ => Vec::new(),
        };
        Self {
            kind: "config_validation",
            config_path: Some(path.to_owned()),
            file: "ripr.toml",
            location_status: if location.is_some() {
                ConfigLocationStatus::Exact
            } else {
                ConfigLocationStatus::Unavailable
            },
            location,
            invalid_value,
            expected_values,
            message,
        }
    }
}

impl From<String> for Box<ConfigDiagnostic> {
    fn from(message: String) -> Self {
        Box::new(ConfigDiagnostic::unavailable(message))
    }
}

fn position(text: &str, offset: usize) -> ConfigPosition {
    let prefix = &text[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .chars()
        .count()
        + 1;
    ConfigPosition { line, column }
}
