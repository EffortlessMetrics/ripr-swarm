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

/// Normalizes a repository-relative path to forward slashes and returns it only
/// when it is contained: no absolute root, drive prefix, empty, `.` or `..`
/// segment. A path that escapes or aliases another path is never normalized into
/// an edit surface.
fn contained_relative_path(path: &str) -> Option<String> {
    let normalized = path.trim().replace('\\', "/");
    if normalized.is_empty() || normalized.starts_with('/') {
        return None;
    }
    let mut segments = normalized.split('/');
    let first = segments.clone().next().unwrap_or("");
    if first.contains(':') {
        return None;
    }
    if segments.any(|segment| segment.is_empty() || segment == "." || segment == "..") {
        return None;
    }
    Some(normalized)
}

/// A contained, normalized Rust test source path: a `.rs` file under a `tests`
/// directory segment (the integration-test layout). A naming convention such as
/// `*_test.rs` cannot establish a test role (RIPR-SPEC-0153), so it is not
/// recognized here; `tests/readme.md`, `tests/../src/lib.rs` and
/// `../other/tests/demo.rs` are not recognized either.
pub fn is_recognized_rust_test_source_file(path: &str) -> bool {
    let Some(normalized) = contained_relative_path(path) else {
        return false;
    };
    let Some(file_name) = normalized.rsplit('/').next() else {
        return false;
    };
    if !file_name.ends_with(".rs") || file_name == ".rs" {
        return false;
    }
    normalized
        .rsplit_once('/')
        .is_some_and(|(directory, _)| directory.split('/').any(|segment| segment == "tests"))
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
    check_validation_evidence(
        input.parse_validation_status,
        input.format_validation_status,
    )?;
    if input.verification_command_refs.is_empty() || input.receipt_command_refs.is_empty() {
        return Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::SourcePreconditionMissing,
        ));
    }

    let original_source_digest = sha256_digest(original_text);
    let canonical_gap_id = input
        .canonical_item
        .as_ref()
        .map(|item| item.as_str().to_string());

    let mut precondition = ExactTestEditPrecondition {
        schema_version: EXACT_TEST_EDIT_PRECONDITION_SCHEMA_VERSION.to_string(),
        // Replaced below from the precondition's own fields, so the builder and
        // `validate` derive the identity from the same governed inputs.
        edit_instruction_id: EditInstructionId(String::new()),
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
    precondition.edit_instruction_id = expected_edit_instruction_id(&precondition)?;
    precondition.validate()?;
    Ok(precondition)
}

/// Both validations must have run and passed for the exact edit. `NotRun` is
/// missing evidence, not a pass, so an omitted field cannot mint.
fn check_validation_evidence(
    parse: GuardedEditValidationStatus,
    format: GuardedEditValidationStatus,
) -> Result<(), GuardedEditUnavailable> {
    match parse {
        GuardedEditValidationStatus::Passed => {}
        GuardedEditValidationStatus::Failed => {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::ParseValidationFailed,
            ));
        }
        GuardedEditValidationStatus::NotRun => {
            return Err(GuardedEditUnavailable::with_detail(
                GuardedEditUnavailableReason::SourcePreconditionMissing,
                "parse validation did not run",
            ));
        }
    }
    match format {
        GuardedEditValidationStatus::Passed => Ok(()),
        GuardedEditValidationStatus::Failed => Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::FormatValidationFailed,
        )),
        GuardedEditValidationStatus::NotRun => Err(GuardedEditUnavailable::with_detail(
            GuardedEditUnavailableReason::SourcePreconditionMissing,
            "format validation did not run",
        )),
    }
}

/// `sha256:` followed by 64 lowercase hex digits, the shape `sha256_digest`
/// produces.
fn is_valid_sha256_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

impl ExactTestEditPrecondition {
    /// The single invariant check for a precondition, whether built here or
    /// decoded from a document. The builder calls it on its own output, so a
    /// decoded or mutated value is held to exactly the builder's rules.
    pub fn validate(&self) -> Result<(), GuardedEditUnavailable> {
        if self.schema_version != EXACT_TEST_EDIT_PRECONDITION_SCHEMA_VERSION {
            return Err(GuardedEditUnavailable::with_detail(
                GuardedEditUnavailableReason::SourcePreconditionMissing,
                "schema_version mismatch",
            ));
        }
        self.assertion_range.validate()?;
        if self.test_selector.trim().is_empty() || self.test_file.trim().is_empty() {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::SourcePreconditionMissing,
            ));
        }
        if !is_recognized_rust_test_source_file(&self.test_file) {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::OutsideAllowedEditSurface,
            ));
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
        if self.original_source_text.is_none() && self.original_source_digest.is_none() {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::SourcePreconditionMissing,
            ));
        }
        if let Some(digest) = self.original_source_digest.as_deref()
            && !is_valid_sha256_digest(digest)
        {
            return Err(GuardedEditUnavailable::with_detail(
                GuardedEditUnavailableReason::SourcePreconditionMissing,
                "original_source_digest is not a sha256 digest",
            ));
        }
        if let (Some(text), Some(digest)) = (
            self.original_source_text.as_deref(),
            self.original_source_digest.as_deref(),
        ) && sha256_digest(text) != digest
        {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::SourceTextMismatch,
            ));
        }
        if self.original_source_text.as_deref() == Some("") {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::SourcePreconditionMissing,
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
        check_validation_evidence(self.parse_validation_status, self.format_validation_status)?;
        if !self.required_client_capability.workspace_edit
            || !self
                .required_client_capability
                .ripr_editor_guarded_test_edit
        {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::ClientCapabilityMissing,
            ));
        }
        if expected_edit_instruction_id(self)? != self.edit_instruction_id {
            return Err(GuardedEditUnavailable::with_detail(
                GuardedEditUnavailableReason::SourcePreconditionMissing,
                "edit_instruction_id does not match the governed inputs",
            ));
        }
        Ok(())
    }
}

/// Fails closed: an absent version on either side never verifies.
pub fn verify_document_version(
    precondition: &ExactTestEditPrecondition,
    current_document_version: Option<u32>,
) -> Result<(), GuardedEditUnavailable> {
    match (precondition.document_version, current_document_version) {
        (Some(expected), Some(actual)) if expected == actual => Ok(()),
        _ => Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::DocumentVersionMismatch,
        )),
    }
}

/// Compares the current document text with the retained original. When the
/// original text was dropped and only its digest is retained, the current text's
/// SHA-256 is compared instead; with neither, nothing can be verified.
pub fn verify_source_text(
    precondition: &ExactTestEditPrecondition,
    current_text: &str,
) -> Result<(), GuardedEditUnavailable> {
    let mismatch = || GuardedEditUnavailable::new(GuardedEditUnavailableReason::SourceTextMismatch);
    match (
        precondition.original_source_text.as_deref(),
        precondition.original_source_digest.as_deref(),
    ) {
        (Some(expected), digest) => {
            if expected != current_text {
                return Err(mismatch());
            }
            match digest {
                Some(digest) if sha256_digest(current_text) != digest => Err(mismatch()),
                _ => Ok(()),
            }
        }
        (None, Some(digest)) => {
            if !is_valid_sha256_digest(digest) || sha256_digest(current_text) != digest {
                return Err(mismatch());
            }
            Ok(())
        }
        (None, None) => Err(GuardedEditUnavailable::new(
            GuardedEditUnavailableReason::SourcePreconditionMissing,
        )),
    }
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

/// Derives the edit instruction identity from the precondition's own governed
/// inputs: the snapshot identity, instruction identities, document version, the
/// original source (by digest), the target, the range and the replacement.
/// Changing any of them changes the identity, as `identity/catalog.rs` records
/// for a snapshot-bound edit instruction. Absent optional inputs hash differently
/// from empty ones.
fn expected_edit_instruction_id(
    precondition: &ExactTestEditPrecondition,
) -> Result<EditInstructionId, GuardedEditUnavailable> {
    let original = match (
        precondition.original_source_digest.as_deref(),
        precondition.original_source_text.as_deref(),
    ) {
        (Some(digest), _) => digest.to_string(),
        (None, Some(text)) => sha256_digest(text),
        (None, None) => {
            return Err(GuardedEditUnavailable::new(
                GuardedEditUnavailableReason::SourcePreconditionMissing,
            ));
        }
    };
    let optional = |value: Option<&str>| match value {
        Some(value) => format!("some:{value}"),
        None => "none".to_string(),
    };
    let identity = &precondition.identity;
    let range = &precondition.assertion_range;
    let digest = identity_digest(&[
        (
            "canonical_gap",
            &optional(precondition.canonical_gap_id.as_deref()),
        ),
        ("seam", &optional(precondition.seam_id.as_deref())),
        (
            "instruction_semantic",
            &optional(
                precondition
                    .instruction_semantic_id
                    .as_ref()
                    .map(|id| id.as_str()),
            ),
        ),
        (
            "instruction_instance",
            &optional(
                precondition
                    .instruction_instance_id
                    .as_ref()
                    .map(|id| id.as_str()),
            ),
        ),
        (
            "workspace",
            &optional(identity.workspace_identity.as_deref()),
        ),
        (
            "repository_head",
            &optional(identity.repository_head.as_deref()),
        ),
        ("input", &optional(identity.input_identity.as_deref())),
        ("snapshot", &optional(identity.snapshot_identity.as_deref())),
        ("config", &optional(identity.config_identity.as_deref())),
        (
            "base_revision",
            &optional(identity.base_revision.as_deref()),
        ),
        (
            "document_version",
            &precondition
                .document_version
                .map_or_else(|| "none".to_string(), |version| format!("some:{version}")),
        ),
        ("test_file", &normalize_path(&precondition.test_file)),
        ("test_selector", &precondition.test_selector),
        ("original_digest", &original),
        ("replacement", &precondition.exact_replacement_text),
        (
            "range",
            &format!(
                "{}:{}-{}:{}",
                range.start_line, range.start_character, range.end_line, range.end_character
            ),
        ),
    ]);
    Ok(EditInstructionId(format!("edit-instruction:{digest:016x}")))
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
    contained_relative_path(path).unwrap_or_else(|| path.replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    type Mutation = (&'static str, Box<dyn Fn(&mut serde_json::Value)>);

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
                    let built_value = serde_json::to_value(&built)
                        .map_err(|error| format!("{id}: encode built: {error}"))?;
                    // The expected identity and digest are literals derived by an
                    // independent reference (RIPR-SPEC-0320), never copied from
                    // this build's output, so a hash that stops binding an input
                    // shows up as drift.
                    for key in [
                        "schema_version",
                        "edit_instruction_id",
                        "original_source_digest",
                        "test_file",
                        "exact_replacement_text",
                        "test_selector",
                    ] {
                        if expected.get(key).is_none() {
                            return Err(format!("{id}: corpus missing `{key}`"));
                        }
                        if built_value.get(key) != expected.get(key) {
                            return Err(format!("{id}: field `{key}` drift"));
                        }
                    }
                    built.validate().map_err(|unavailable| {
                        format!("{id}: built precondition invalid: {unavailable:?}")
                    })?;
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
    fn fix_instruction_guarded_edit_absent_document_version_never_verifies() -> Result<(), String> {
        let precondition = try_build_exact_test_edit_precondition(&sample_input())
            .map_err(|unavailable| format!("{unavailable:?}"))?;
        // Current document version unknown: refuse.
        match verify_document_version(&precondition, None) {
            Err(unavailable)
                if unavailable.reason == GuardedEditUnavailableReason::DocumentVersionMismatch => {}
            other => return Err(format!("expected mismatch for unknown current: {other:?}")),
        }
        // Precondition minted without a version: refuse any current version.
        let mut unversioned = precondition.clone();
        unversioned.document_version = None;
        for current in [None, Some(3)] {
            match verify_document_version(&unversioned, current) {
                Err(unavailable)
                    if unavailable.reason
                        == GuardedEditUnavailableReason::DocumentVersionMismatch => {}
                other => return Err(format!("expected mismatch for {current:?}: {other:?}")),
            }
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

    fn built() -> Result<ExactTestEditPrecondition, String> {
        try_build_exact_test_edit_precondition(&sample_input())
            .map_err(|unavailable| format!("{unavailable:?}"))
    }

    fn id_of(input: &GuardedEditBuildInput) -> Result<String, String> {
        try_build_exact_test_edit_precondition(input)
            .map(|p| p.edit_instruction_id.as_str().to_string())
            .map_err(|unavailable| format!("{unavailable:?}"))
    }

    fn reason_of(input: &GuardedEditBuildInput) -> Result<GuardedEditUnavailableReason, String> {
        match try_build_exact_test_edit_precondition(input) {
            Err(unavailable) => Ok(unavailable.reason),
            Ok(_) => Err("expected unavailable".to_string()),
        }
    }

    /// The identity is snapshot-bound (identity/catalog.rs): every governed
    /// input changes it, each perturbation on its own.
    #[test]
    fn fix_instruction_guarded_edit_id_binds_every_governed_input() -> Result<(), String> {
        let baseline = id_of(&sample_input())?;
        assert_eq!(baseline, id_of(&sample_input())?, "deterministic");
        type Perturb = (&'static str, fn(&mut GuardedEditBuildInput));
        let perturbations: Vec<Perturb> = vec![
            ("document_version", |i| i.document_version = Some(4)),
            ("document_version removed", |i| i.document_version = None),
            ("instruction_instance_id", |i| {
                i.instruction_instance_id =
                    InstructionInstanceId::parse("instruction-instance:abc").ok();
            }),
            ("instruction_semantic_id", |i| {
                i.instruction_semantic_id =
                    InstructionSemanticId::parse("instruction-semantic:abc").ok();
            }),
            ("workspace_identity", |i| {
                i.identity.workspace_identity = Some("ws:other".into())
            }),
            ("repository_head", |i| {
                i.identity.repository_head = Some("def456".into())
            }),
            ("input_identity", |i| {
                i.identity.input_identity = Some("input:2".into())
            }),
            ("snapshot_identity", |i| {
                i.identity.snapshot_identity = Some("snap:2".into())
            }),
            ("config_identity", |i| {
                i.identity.config_identity = Some("cfg:2".into())
            }),
            ("base_revision", |i| {
                i.identity.base_revision = Some("base:2".into())
            }),
            ("snapshot_identity removed", |i| {
                i.identity.snapshot_identity = None
            }),
            ("seam_id", |i| i.seam_id = Some("seam:other".into())),
            ("range", |i| i.assertion_range.end_character = 29),
            ("replacement", |i| {
                i.exact_replacement_text = "assert_eq!(a, b);".into()
            }),
            ("original text", |i| {
                i.original_source_text = Some("assert!(other.is_err());".into())
            }),
            ("test_selector", |i| {
                i.test_selector = "tests/demo.rs::other".into()
            }),
        ];
        for (name, perturb) in perturbations {
            let mut input = sample_input();
            perturb(&mut input);
            let changed = id_of(&input)?;
            assert_ne!(changed, baseline, "id must change when {name} changes");
        }
        // An absent optional input is not the same as an empty one.
        let mut empty = sample_input();
        empty.identity.config_identity = Some(String::new());
        let mut absent = sample_input();
        absent.identity.config_identity = None;
        assert_ne!(id_of(&empty)?, id_of(&absent)?, "empty vs absent");
        Ok(())
    }

    #[test]
    fn fix_instruction_guarded_edit_requires_passed_validation_evidence() -> Result<(), String> {
        for (name, parse, format, expected) in [
            (
                "parse not run",
                GuardedEditValidationStatus::NotRun,
                GuardedEditValidationStatus::Passed,
                GuardedEditUnavailableReason::SourcePreconditionMissing,
            ),
            (
                "format not run",
                GuardedEditValidationStatus::Passed,
                GuardedEditValidationStatus::NotRun,
                GuardedEditUnavailableReason::SourcePreconditionMissing,
            ),
            (
                "parse failed",
                GuardedEditValidationStatus::Failed,
                GuardedEditValidationStatus::Passed,
                GuardedEditUnavailableReason::ParseValidationFailed,
            ),
            (
                "format failed",
                GuardedEditValidationStatus::Passed,
                GuardedEditValidationStatus::Failed,
                GuardedEditUnavailableReason::FormatValidationFailed,
            ),
        ] {
            let mut input = sample_input();
            input.parse_validation_status = parse;
            input.format_validation_status = format;
            assert_eq!(reason_of(&input)?, expected, "{name}");
        }
        // Omitted fields default to NotRun and must not mint (serde default).
        let omitted: GuardedEditBuildInput = serde_json::from_value(serde_json::json!({
            "test_selector": "tests/demo.rs::t",
            "test_file": "tests/demo.rs",
            "original_source_text": "assert!(x);",
            "exact_replacement_text": "assert_eq!(x, 1);",
            "allowed_edit_surface": ["tests/demo.rs"],
            "must_not_change": ["src/lib.rs"],
            "verification_command_refs": ["v"],
            "receipt_command_refs": ["r"],
        }))
        .map_err(|error| error.to_string())?;
        assert_eq!(
            reason_of(&omitted)?,
            GuardedEditUnavailableReason::SourcePreconditionMissing,
            "omitted validation fields"
        );
        Ok(())
    }

    /// A decoded or mutated precondition is held to the builder's rules.
    #[test]
    fn fix_instruction_guarded_edit_decoded_dto_is_validated_like_the_builder() -> Result<(), String>
    {
        let valid = serde_json::to_value(built()?).map_err(|e| e.to_string())?;
        let decode =
            |mutate: &dyn Fn(&mut serde_json::Value)| -> Result<ExactTestEditPrecondition, String> {
                let mut value = valid.clone();
                mutate(&mut value);
                serde_json::from_value(value).map_err(|e| e.to_string())
            };
        decode(&|_| {})?
            .validate()
            .map_err(|unavailable| format!("unmutated decode must validate: {unavailable:?}"))?;
        let mutations: Vec<Mutation> = vec![
            (
                "production test_file and surface",
                Box::new(|v| {
                    v["test_file"] = "src/lib.rs".into();
                    v["allowed_edit_surface"] = serde_json::json!(["src/lib.rs"]);
                }),
            ),
            (
                "traversal test_file",
                Box::new(|v| {
                    v["test_file"] = "tests/../src/lib.rs".into();
                    v["allowed_edit_surface"] = serde_json::json!(["tests/../src/lib.rs"]);
                }),
            ),
            (
                "parse failed",
                Box::new(|v| v["parse_validation_status"] = "failed".into()),
            ),
            (
                "format failed",
                Box::new(|v| v["format_validation_status"] = "failed".into()),
            ),
            (
                "parse not run",
                Box::new(|v| v["parse_validation_status"] = "not_run".into()),
            ),
            (
                "format not run",
                Box::new(|v| v["format_validation_status"] = "not_run".into()),
            ),
            (
                "empty selector",
                Box::new(|v| v["test_selector"] = "  ".into()),
            ),
            (
                "workspace_edit disabled",
                Box::new(|v| {
                    v["required_client_capability"]["workspace_edit"] = false.into();
                }),
            ),
            (
                "guarded edit capability disabled",
                Box::new(|v| {
                    v["required_client_capability"]["ripr_editor_guarded_test_edit"] = false.into();
                }),
            ),
            (
                "replacement changed under a minted id",
                Box::new(|v| {
                    v["exact_replacement_text"] = "assert!(true);".into();
                }),
            ),
            (
                "document_version changed under a minted id",
                Box::new(|v| v["document_version"] = 9.into()),
            ),
            (
                "snapshot identity changed under a minted id",
                Box::new(|v| {
                    v["identity"]["snapshot_identity"] = "snap:other".into();
                }),
            ),
            (
                "digest malformed",
                Box::new(|v| v["original_source_digest"] = "sha256:xyz".into()),
            ),
            (
                "digest mismatches retained text",
                Box::new(|v| {
                    v["original_source_digest"] = sha256_digest("something else").into();
                }),
            ),
            (
                "two edit surfaces",
                Box::new(|v| {
                    v["allowed_edit_surface"] =
                        serde_json::json!(["tests/demo.rs", "tests/other.rs"]);
                }),
            ),
        ];
        for (name, mutate) in mutations {
            let decoded = decode(&*mutate)?;
            assert!(
                decoded.validate().is_err(),
                "mutation must be rejected: {name}"
            );
        }
        Ok(())
    }

    #[test]
    fn fix_instruction_guarded_edit_test_path_must_be_a_contained_rust_test_source() {
        for accepted in [
            "tests/demo.rs",
            "crates/ripr/tests/demo.rs",
            "tests\\demo.rs",
        ] {
            assert!(is_recognized_rust_test_source_file(accepted), "{accepted}");
        }
        // A drive-prefixed path, built so the literal is not a local path.
        let drive_prefixed = format!("{}:/tests/demo.rs", 'C');
        assert!(!is_recognized_rust_test_source_file(&drive_prefixed));
        for rejected in [
            "tests/../src/lib.rs",
            "tests/readme.md",
            "../other/tests/demo.rs",
            "/tests/demo.rs",
            "tests//demo.rs",
            "tests/./demo.rs",
            "tests/",
            "tests",
            "",
            "src/lib.rs",
            "mytests/demo.rs",
            "src/widget_test.rs",
            "src/widget_tests.rs",
            "tests/.rs",
        ] {
            assert!(!is_recognized_rust_test_source_file(rejected), "{rejected}");
        }
    }

    #[test]
    fn fix_instruction_guarded_edit_digest_only_representation_verifies_against_the_digest()
    -> Result<(), String> {
        let full = built()?;
        // Text and digest retained.
        verify_source_text(&full, "assert!(result.is_err());")
            .map_err(|unavailable| format!("text present: {unavailable:?}"))?;
        assert!(verify_source_text(&full, "assert!(result.is_ok());").is_err());
        // Only the digest retained (the passive representation).
        let mut digest_only = full.clone();
        digest_only.original_source_text = None;
        digest_only
            .validate()
            .map_err(|unavailable| format!("digest-only must validate: {unavailable:?}"))?;
        verify_source_text(&digest_only, "assert!(result.is_err());")
            .map_err(|unavailable| format!("digest-only match: {unavailable:?}"))?;
        assert_eq!(
            verify_source_text(&digest_only, "assert!(result.is_ok());")
                .err()
                .map(|u| u.reason),
            Some(GuardedEditUnavailableReason::SourceTextMismatch)
        );
        // A malformed retained digest never verifies.
        let mut malformed = digest_only.clone();
        malformed.original_source_digest = Some("sha256:00".to_string());
        assert!(malformed.validate().is_err());
        assert!(verify_source_text(&malformed, "assert!(result.is_err());").is_err());
        // Neither retained: nothing to verify against.
        let mut neither = digest_only;
        neither.original_source_digest = None;
        assert_eq!(
            verify_source_text(&neither, "x").err().map(|u| u.reason),
            Some(GuardedEditUnavailableReason::SourcePreconditionMissing)
        );
        Ok(())
    }
}
