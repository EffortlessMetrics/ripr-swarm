//! Versioned source-precondition contract for the guarded test-only assertion
//! replacement edit class (#1902 / #1571). This module types and validates the
//! boundary; it does not emit `CodeAction.edit`, apply edits, or prove runtime
//! correctness.

use super::{CanonicalItemId, InstructionInstanceId, InstructionSemanticId};
use sha2::{Digest, Sha256};
use std::fmt;

pub const EXACT_TEST_EDIT_PRECONDITION_SCHEMA_VERSION: &str = "exact_test_edit_precondition.v1";

pub const GUARDED_EDIT_CLAIM_BOUNDARY: &str = "Static precondition for one bounded test-only assertion replacement; it does not authorize application, establish runtime correctness, or replace verification or receipt commands.";

pub const GUARDED_EDIT_NON_CLAIMS: &[&str] = &[
    "does not prove the replacement is semantically correct",
    "does not execute verification or receipt commands",
    "does not establish mutation adequacy or merge readiness",
    "does not apply or advertise a workspace edit by itself",
];

/// Snapshot-bound edit instruction identity (#1571/#4806 lineage).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize)]
#[serde(transparent)]
pub struct EditInstructionId(pub String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditInstructionIdParseError {
    Empty,
    WrongPrefix { value: String },
}

impl fmt::Display for EditInstructionIdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("edit instruction id must not be empty"),
            Self::WrongPrefix { value } => {
                write!(f, "value `{value}` is not an EditInstructionId")
            }
        }
    }
}

impl EditInstructionId {
    pub fn parse(value: &str) -> Result<Self, EditInstructionIdParseError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(EditInstructionIdParseError::Empty);
        }
        if !trimmed.starts_with("edit-instruction:") {
            return Err(EditInstructionIdParseError::WrongPrefix {
                value: trimmed.to_string(),
            });
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> serde::Deserialize<'de> for EditInstructionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Fail-closed reasons when a guarded edit precondition cannot be minted or
/// honored.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum GuardedEditUnavailableReason {
    AmbiguousFixSite,
    SourcePreconditionMissing,
    SourceTextMismatch,
    DocumentVersionMismatch,
    ReplacementUnresolved,
    RequiredImport,
    ParseValidationFailed,
    FormatValidationFailed,
    OutsideAllowedEditSurface,
    ProductionFileTarget,
    PreviewOrStaticLimitation,
    StaleSnapshot,
    ClientCapabilityMissing,
}

impl GuardedEditUnavailableReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AmbiguousFixSite => "ambiguous_fix_site",
            Self::SourcePreconditionMissing => "source_precondition_missing",
            Self::SourceTextMismatch => "source_text_mismatch",
            Self::DocumentVersionMismatch => "document_version_mismatch",
            Self::ReplacementUnresolved => "replacement_unresolved",
            Self::RequiredImport => "required_import",
            Self::ParseValidationFailed => "parse_validation_failed",
            Self::FormatValidationFailed => "format_validation_failed",
            Self::OutsideAllowedEditSurface => "outside_allowed_edit_surface",
            Self::ProductionFileTarget => "production_file_target",
            Self::PreviewOrStaticLimitation => "preview_or_static_limitation",
            Self::StaleSnapshot => "stale_snapshot",
            Self::ClientCapabilityMissing => "client_capability_missing",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GuardedEditUnavailable {
    pub reason: GuardedEditUnavailableReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl GuardedEditUnavailable {
    pub fn new(reason: GuardedEditUnavailableReason) -> Self {
        Self {
            reason,
            detail: None,
        }
    }

    pub fn with_detail(reason: GuardedEditUnavailableReason, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: Some(detail.into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardedEditValidationStatus {
    Passed,
    Failed,
    NotRun,
}

/// Assertion/fix-site range in the negotiated UTF-16 line/character encoding.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NegotiatedSourceRange {
    pub start_line: u32,
    pub start_character: u32,
    pub end_line: u32,
    pub end_character: u32,
}

impl NegotiatedSourceRange {
    pub fn validate(&self) -> Result<(), GuardedEditUnavailable> {
        if self.end_line < self.start_line {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::SourcePreconditionMissing,
            ));
        }
        if self.end_line == self.start_line && self.end_character < self.start_character {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::SourcePreconditionMissing,
            ));
        }
        Ok(())
    }
}

impl Default for GuardedEditBuildInput {
    fn default() -> Self {
        Self {
            identity: GuardedEditIdentity::default(),
            canonical_item: None,
            seam_id: None,
            instruction_semantic_id: None,
            instruction_instance_id: None,
            test_selector: String::new(),
            test_file: String::new(),
            assertion_range: NegotiatedSourceRange {
                start_line: 0,
                start_character: 0,
                end_line: 0,
                end_character: 0,
            },
            original_source_text: None,
            document_version: None,
            exact_replacement_text: String::new(),
            parse_validation_status: GuardedEditValidationStatus::NotRun,
            format_validation_status: GuardedEditValidationStatus::NotRun,
            allowed_edit_surface: Vec::new(),
            must_not_change: Vec::new(),
            verification_command_refs: Vec::new(),
            receipt_command_refs: Vec::new(),
            ambiguous_fix_site: false,
            required_import: false,
            preview_or_static_limitation: false,
            stale_snapshot: false,
            production_file_target: false,
            multi_file_edit: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct GuardedEditIdentity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RequiredClientCapability {
    pub workspace_edit: bool,
    pub ripr_editor_guarded_test_edit: bool,
}

impl RequiredClientCapability {
    pub fn v1_guarded_test_edit() -> Self {
        Self {
            workspace_edit: true,
            ripr_editor_guarded_test_edit: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExactTestEditPrecondition {
    pub schema_version: String,
    pub edit_instruction_id: EditInstructionId,
    pub identity: GuardedEditIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_gap_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seam_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction_semantic_id: Option<InstructionSemanticId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction_instance_id: Option<InstructionInstanceId>,
    pub test_selector: String,
    pub test_file: String,
    pub assertion_range: NegotiatedSourceRange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_source_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_source_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_version: Option<u32>,
    pub exact_replacement_text: String,
    pub parse_validation_status: GuardedEditValidationStatus,
    pub format_validation_status: GuardedEditValidationStatus,
    pub allowed_edit_surface: Vec<String>,
    pub must_not_change: Vec<String>,
    pub required_client_capability: RequiredClientCapability,
    pub verification_command_refs: Vec<String>,
    pub receipt_command_refs: Vec<String>,
    pub non_claims: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct GuardedEditBuildInput {
    pub identity: GuardedEditIdentity,
    pub canonical_item: Option<CanonicalItemId>,
    pub seam_id: Option<String>,
    pub instruction_semantic_id: Option<InstructionSemanticId>,
    pub instruction_instance_id: Option<InstructionInstanceId>,
    pub test_selector: String,
    pub test_file: String,
    pub assertion_range: NegotiatedSourceRange,
    pub original_source_text: Option<String>,
    pub document_version: Option<u32>,
    pub exact_replacement_text: String,
    pub parse_validation_status: GuardedEditValidationStatus,
    pub format_validation_status: GuardedEditValidationStatus,
    pub allowed_edit_surface: Vec<String>,
    pub must_not_change: Vec<String>,
    pub verification_command_refs: Vec<String>,
    pub receipt_command_refs: Vec<String>,
    pub ambiguous_fix_site: bool,
    pub required_import: bool,
    pub preview_or_static_limitation: bool,
    pub stale_snapshot: bool,
    pub production_file_target: bool,
    pub multi_file_edit: bool,
}

pub fn sha256_digest(text: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(text.as_bytes()))
}

pub fn is_recognized_rust_test_source_file(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.starts_with("tests/")
        || normalized.contains("/tests/")
        || normalized.ends_with("_test.rs")
        || normalized.ends_with("_tests.rs")
}

pub fn try_build_exact_test_edit_precondition(
    input: &GuardedEditBuildInput,
) -> Result<ExactTestEditPrecondition, GuardedEditUnavailable> {
    if input.stale_snapshot {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::StaleSnapshot,
        ));
    }
    if input.preview_or_static_limitation {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::PreviewOrStaticLimitation,
        ));
    }
    if input.production_file_target {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::ProductionFileTarget,
        ));
    }
    if input.ambiguous_fix_site {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::AmbiguousFixSite,
        ));
    }
    if input.required_import {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::RequiredImport,
        ));
    }
    if input.multi_file_edit {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::OutsideAllowedEditSurface,
        ));
    }
    input.assertion_range.validate()?;
    if input.test_selector.trim().is_empty() || input.test_file.trim().is_empty() {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::SourcePreconditionMissing,
        ));
    }
    if !is_recognized_rust_test_source_file(&input.test_file) {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::OutsideAllowedEditSurface,
        ));
    }
    if input.allowed_edit_surface.is_empty() {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::OutsideAllowedEditSurface,
        ));
    }
    if input.allowed_edit_surface.len() != 1 {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::OutsideAllowedEditSurface,
        ));
    }
    let allowed = normalize_path(&input.allowed_edit_surface[0]);
    let test_file = normalize_path(&input.test_file);
    if allowed != test_file {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::OutsideAllowedEditSurface,
        ));
    }
    if input.must_not_change.is_empty() {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::SourcePreconditionMissing,
        ));
    }
    if input.exact_replacement_text.trim().is_empty() {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::ReplacementUnresolved,
        ));
    }
    let original_text = match input.original_source_text.as_deref() {
        Some(text) if !text.is_empty() => text,
        _ => {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::SourcePreconditionMissing,
            ));
        }
    };
    if input.parse_validation_status == GuardedEditValidationStatus::Failed {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::ParseValidationFailed,
        ));
    }
    if input.format_validation_status == GuardedEditValidationStatus::Failed {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::FormatValidationFailed,
        ));
    }
    if input.verification_command_refs.is_empty() || input.receipt_command_refs.is_empty() {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::SourcePreconditionMissing,
        ));
    }

    let edit_instruction_id = mint_edit_instruction_id(input);
    let original_source_digest = sha256_digest(original_text);
    let canonical_gap_id = input
        .canonical_item
        .as_ref()
        .map(|item| item.as_str().to_string());

    let precondition = ExactTestEditPrecondition {
        schema_version: EXACT_TEST_EDIT_PRECONDITION_SCHEMA_VERSION.to_string(),
        edit_instruction_id,
        identity: input.identity.clone(),
        canonical_gap_id,
        seam_id: input.seam_id.clone(),
        instruction_semantic_id: input.instruction_semantic_id.clone(),
        instruction_instance_id: input.instruction_instance_id.clone(),
        test_selector: input.test_selector.clone(),
        test_file: test_file.clone(),
        assertion_range: input.assertion_range.clone(),
        original_source_text: Some(original_text.to_string()),
        original_source_digest: Some(original_source_digest),
        document_version: input.document_version,
        exact_replacement_text: input.exact_replacement_text.clone(),
        parse_validation_status: input.parse_validation_status,
        format_validation_status: input.format_validation_status,
        allowed_edit_surface: vec![allowed],
        must_not_change: input.must_not_change.clone(),
        required_client_capability: RequiredClientCapability::v1_guarded_test_edit(),
        verification_command_refs: input.verification_command_refs.clone(),
        receipt_command_refs: input.receipt_command_refs.clone(),
        non_claims: GUARDED_EDIT_NON_CLAIMS
            .iter()
            .map(|claim| (*claim).to_string())
            .collect(),
    };
    precondition.validate()?;
    Ok(precondition)
}

impl ExactTestEditPrecondition {
    pub fn validate(&self) -> Result<(), GuardedEditUnavailable> {
        if self.schema_version != EXACT_TEST_EDIT_PRECONDITION_SCHEMA_VERSION {
            return Err(GuardedEditUnavailable::with_detail(
                GuardedEditUnavailableReason::SourcePreconditionMissing,
                "schema_version mismatch",
            ));
        }
        self.assertion_range.validate()?;
        if self.original_source_text.is_none() && self.original_source_digest.is_none() {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::SourcePreconditionMissing,
            ));
        }
        if let (Some(text), Some(digest)) = (
            self.original_source_text.as_deref(),
            self.original_source_digest.as_deref(),
        ) {
            if sha256_digest(text) != digest {
                return Err(GuardedEditUnavailable::new(
                    GuardedEditUnavailableReason::SourceTextMismatch,
                ));
            }
        }
        if self.allowed_edit_surface.len() != 1 {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::OutsideAllowedEditSurface,
            ));
        }
        if normalize_path(&self.allowed_edit_surface[0]) != normalize_path(&self.test_file) {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::OutsideAllowedEditSurface,
            ));
        }
        if self.must_not_change.is_empty()
            || self.exact_replacement_text.trim().is_empty()
            || self.verification_command_refs.is_empty()
            || self.receipt_command_refs.is_empty()
        {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::SourcePreconditionMissing,
            ));
        }
        Ok(())
    }
}

pub fn verify_document_version(
    precondition: &ExactTestEditPrecondition,
    current_document_version: Option<u32>,
) -> Result<(), GuardedEditUnavailable> {
    match (precondition.document_version, current_document_version) {
        (Some(expected), Some(actual)) if expected == actual => Ok(()),
        (None, _) => Ok(()),
        _ => Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::DocumentVersionMismatch,
        )),
    }
}

pub fn verify_source_text(
    precondition: &ExactTestEditPrecondition,
    current_text: &str,
) -> Result<(), GuardedEditUnavailable> {
    let expected = precondition
        .original_source_text
        .as_deref()
        .ok_or_else(|| {
            GuardedEditUnavailable::new(GuardedEditUnavailableReason::SourcePreconditionMissing)
        })?;
    if expected != current_text {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::SourceTextMismatch,
        ));
    }
    Ok(())
}

pub fn client_supports_guarded_edit(
    workspace_edit: bool,
    ripr_editor_guarded_test_edit: bool,
) -> Result<(), GuardedEditUnavailable> {
    if workspace_edit && ripr_editor_guarded_test_edit {
        Ok(())
    } else {
        Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::ClientCapabilityMissing,
        ))
    }
}

fn mint_edit_instruction_id(input: &GuardedEditBuildInput) -> EditInstructionId {
    let canonical = input
        .canonical_item
        .as_ref()
        .map(|item| item.as_str())
        .unwrap_or("");
    let digest = identity_digest(&[
        ("canonical_item", canonical),
        ("test_file", &normalize_path(&input.test_file)),
        ("test_selector", &input.test_selector),
        (
            "original",
            input.original_source_text.as_deref().unwrap_or(""),
        ),
        ("replacement", &input.exact_replacement_text),
        (
            "range",
            &format!(
                "{}:{}-{}:{}",
                input.assertion_range.start_line,
                input.assertion_range.start_character,
                input.assertion_range.end_line,
                input.assertion_range.end_character
            ),
        ),
    ]);
    EditInstructionId(format!("edit-instruction:{digest:016x}"))
}

fn identity_digest(parts: &[(&str, &str)]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for (key, value) in parts {
        for byte in key.as_bytes().iter().chain([0].iter()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        for byte in value.as_bytes().iter().chain([0].iter()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::CanonicalItemId;

    fn sample_input() -> GuardedEditBuildInput {
        GuardedEditBuildInput {
            identity: GuardedEditIdentity {
                workspace_identity: Some("ws:demo".to_string()),
                repository_head: Some("abc123".to_string()),
                input_identity: Some("input:1".to_string()),
                snapshot_identity: Some("snap:1".to_string()),
                config_identity: Some("cfg:1".to_string()),
                base_revision: Some("base:1".to_string()),
            },
            canonical_item: CanonicalItemId::from_canonical_gap_id("gap:67f9714ba37b3967").ok(),
            seam_id: Some("seam:demo".to_string()),
            instruction_semantic_id: None,
            instruction_instance_id: None,
            test_selector: "tests/demo.rs::rejects_self_game".to_string(),
            test_file: "tests/demo.rs".to_string(),
            assertion_range: NegotiatedSourceRange {
                start_line: 10,
                start_character: 4,
                end_line: 10,
                end_character: 28,
            },
            original_source_text: Some("assert!(result.is_err());".to_string()),
            document_version: Some(3),
            exact_replacement_text: "assert_eq!(result, Err(StandingsError::SelfGame));"
                .to_string(),
            parse_validation_status: GuardedEditValidationStatus::Passed,
            format_validation_status: GuardedEditValidationStatus::Passed,
            allowed_edit_surface: vec!["tests/demo.rs".to_string()],
            must_not_change: vec!["src/standings.rs".to_string()],
            verification_command_refs: vec!["ripr agent verify --phase after".to_string()],
            receipt_command_refs: vec!["ripr agent receipt --phase after".to_string()],
            ambiguous_fix_site: false,
            required_import: false,
            preview_or_static_limitation: false,
            stale_snapshot: false,
            production_file_target: false,
            multi_file_edit: false,
        }
    }

    #[test]
    fn fix_instruction_guarded_edit_builds_broad_error_precondition() -> Result<(), String> {
        let precondition = try_build_exact_test_edit_precondition(&sample_input())
            .map_err(|unavailable| format!("{unavailable:?}"))?;
        assert_eq!(
            precondition.schema_version,
            EXACT_TEST_EDIT_PRECONDITION_SCHEMA_VERSION
        );
        assert!(
            precondition
                .edit_instruction_id
                .as_str()
                .starts_with("edit-instruction:")
        );
        assert_eq!(precondition.test_file, "tests/demo.rs");
        assert!(precondition.required_client_capability.workspace_edit);
        assert!(
            precondition
                .required_client_capability
                .ripr_editor_guarded_test_edit
        );
        Ok(())
    }

    #[test]
    fn fix_instruction_guarded_edit_boundary_value_fixture() -> Result<(), String> {
        let mut input = sample_input();
        input.test_selector = "tests/pricing.rs::boundary_equality".to_string();
        input.test_file = "tests/pricing.rs".to_string();
        input.original_source_text = Some("assert_eq!(total, 50);".to_string());
        input.exact_replacement_text = "assert_eq!(total, discount_threshold);".to_string();
        input.allowed_edit_surface = vec!["tests/pricing.rs".to_string()];
        let precondition = try_build_exact_test_edit_precondition(&input)
            .map_err(|unavailable| format!("{unavailable:?}"))?;
        assert_eq!(
            precondition.exact_replacement_text,
            input.exact_replacement_text
        );
        Ok(())
    }

    #[test]
    fn fix_instruction_guarded_edit_fail_closed_reason_fixture() -> Result<(), String> {
        let mut input = sample_input();
        input.ambiguous_fix_site = true;
        match try_build_exact_test_edit_precondition(&input) {
            Err(unavailable) => {
                assert_eq!(
                    unavailable.reason,
                    GuardedEditUnavailableReason::AmbiguousFixSite
                );
            }
            Ok(_) => return Err("expected unavailable".to_string()),
        }
        Ok(())
    }

    #[test]
    fn fix_instruction_guarded_edit_corpus_cases() -> Result<(), String> {
        let corpus_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/boundary_gap/expected/guarded-edit-precondition/corpus.json");
        let raw = std::fs::read_to_string(&corpus_path)
            .map_err(|error| format!("read corpus: {error}"))?;
        let corpus: serde_json::Value =
            serde_json::from_str(&raw).map_err(|error| format!("parse corpus: {error}"))?;
        let cases = corpus
            .get("cases")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "corpus missing cases".to_string())?;
        for case in cases {
            let id = case
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing-id>");
            let expect = case
                .get("expect")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("{id}: missing expect"))?;
            let input_value = case
                .get("input")
                .ok_or_else(|| format!("{id}: missing input"))?;
            let input: GuardedEditBuildInput = serde_json::from_value(input_value.clone())
                .map_err(|error| format!("{id}: input decode: {error}"))?;
            match expect {
                "precondition" => {
                    let built = try_build_exact_test_edit_precondition(&input)
                        .map_err(|unavailable| format!("{id}: {unavailable:?}"))?;
                    let expected = case
                        .get("precondition")
                        .ok_or_else(|| format!("{id}: missing precondition"))?;
                    let mut expected_value = expected.clone();
                    if let Some(obj) = expected_value.as_object_mut() {
                        obj.insert(
                            "edit_instruction_id".to_string(),
                            serde_json::Value::String(
                                built.edit_instruction_id.as_str().to_string(),
                            ),
                        );
                        if let Some(digest) = &built.original_source_digest {
                            obj.insert(
                                "original_source_digest".to_string(),
                                serde_json::Value::String(digest.clone()),
                            );
                        }
                    }
                    let built_value = serde_json::to_value(&built)
                        .map_err(|error| format!("{id}: encode built: {error}"))?;
                    if built_value.get("edit_instruction_id")
                        != expected_value.get("edit_instruction_id")
                    {
                        return Err(format!("{id}: edit_instruction_id drift"));
                    }
                    for key in [
                        "schema_version",
                        "test_file",
                        "exact_replacement_text",
                        "test_selector",
                    ] {
                        if built_value.get(key) != expected_value.get(key) {
                            return Err(format!("{id}: field `{key}` mismatch"));
                        }
                    }
                }
                "unavailable" => {
                    let unavailable = match try_build_exact_test_edit_precondition(&input) {
                        Err(unavailable) => unavailable,
                        Ok(_) => return Err(format!("{id}: expected unavailable")),
                    };
                    let expected_reason = case
                        .get("reason")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| format!("{id}: missing reason"))?;
                    assert_eq!(unavailable.reason.as_str(), expected_reason, "{id}");
                }
                other => return Err(format!("{id}: unknown expect `{other}`")),
            }
        }
        Ok(())
    }

    #[test]
    fn fix_instruction_guarded_edit_rejects_stale_and_production_targets() -> Result<(), String> {
        let mut stale = sample_input();
        stale.stale_snapshot = true;
        match try_build_exact_test_edit_precondition(&stale) {
            Err(unavailable) => assert_eq!(
                unavailable.reason,
                GuardedEditUnavailableReason::StaleSnapshot
            ),
            Ok(_) => return Err("expected stale unavailable".to_string()),
        }
        let mut production = sample_input();
        production.production_file_target = true;
        match try_build_exact_test_edit_precondition(&production) {
            Err(unavailable) => assert_eq!(
                unavailable.reason,
                GuardedEditUnavailableReason::ProductionFileTarget
            ),
            Ok(_) => return Err("expected production unavailable".to_string()),
        }
        Ok(())
    }

    #[test]
    fn fix_instruction_guarded_edit_document_and_source_checks() -> Result<(), String> {
        let precondition = try_build_exact_test_edit_precondition(&sample_input())
            .map_err(|unavailable| format!("{unavailable:?}"))?;
        verify_document_version(&precondition, Some(3))
            .map_err(|unavailable| format!("doc ok: {unavailable:?}"))?;
        if verify_document_version(&precondition, Some(4)).is_ok() {
            return Err("expected doc mismatch".to_string());
        }
        verify_source_text(&precondition, "assert!(result.is_err());")
            .map_err(|unavailable| format!("text ok: {unavailable:?}"))?;
        if verify_source_text(&precondition, "assert!(result.is_ok());").is_ok() {
            return Err("expected text mismatch".to_string());
        }
        client_supports_guarded_edit(true, true).map_err(|e| format!("{e:?}"))?;
        if client_supports_guarded_edit(true, false).is_ok() {
            return Err("expected cap miss".to_string());
        }
        Ok(())
    }
}
