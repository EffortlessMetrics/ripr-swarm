//! Crate-private seam model per `docs/specs/RIPR-SPEC-0005-repo-seam-inventory.md`.
//!
//! This module introduces the seam evidence data types — `RepoSeam`,
//! `SeamId`, `SeamKind`, `RequiredDiscriminator`, `ExpectedSink`,
//! `SeamGripClass` — but does not walk source, attach evidence,
//! classify, or render output. Those
//! responsibilities land in subsequent work items
//! (`analysis/repo-seam-inventory-v1`, `analysis/test-grip-evidence-v1`,
//! `analysis/repo-ripr-classification-v1`, `output/repo-exposure-report-v1`).
//!
//! All items are `pub(crate)`. `policy/public_api.txt` is intentionally
//! unchanged: the seam model is internal until a real consumer contract
//! exists.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Stable seam identifier.
///
/// Deterministic across runs and across input file walk reorderings — and
/// across line-ending spellings of the same logical file (#7203): the local
/// coordinate is the byte offset the seam has in the file's CRLF→LF
/// normalized text, so a `core.autocrlf=true` checkout and an LF checkout
/// of one commit produce one ID.
/// Format: 16 lowercase hex chars, the FNV-1a 64-bit hash of the canonical
/// fields (file, owner, kind, normalized byte offset).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct SeamId(String);

impl SeamId {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Behavior seam category. The initial set is syntax-backed; per
/// RIPR-SPEC-0005 § Non-Goals, MIR/trait-resolution kinds may be added
/// later. `ValidationBranch` from the spec is intentionally absent
/// until `analysis/test-grip-evidence-v1` adds detection — the model
/// admits new variants additively.
///
/// Rust-only boundary (#1937/#3039): [`SeamKind`] classifies behavior
/// boundaries found in current Rust source by the Rust seam inventory;
/// preview-language adapters emit domain [`crate::domain::ProbeFamily`] values
/// instead. The vocabularies are not semantically dual —
/// [`crate::domain::ProbeFamily::CallDeletion`]
/// detects a call site *removed by the diff*, while
/// [`SeamKind::CallPresence`] marks a *present* call-site boundary that
/// needs a call-expectation oracle — so there is deliberately no canonical
/// crosswalk between them. Preview-language limitations are expressed
/// through [`crate::domain::StaticLimitKind`], never through lossy
/// [`SeamKind`] conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum SeamKind {
    PredicateBoundary,
    ErrorVariant,
    ReturnValue,
    FieldConstruction,
    SideEffect,
    MatchArm,
    CallPresence,
}

impl SeamKind {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            SeamKind::PredicateBoundary => "predicate_boundary",
            SeamKind::ErrorVariant => "error_variant",
            SeamKind::ReturnValue => "return_value",
            SeamKind::FieldConstruction => "field_construction",
            SeamKind::SideEffect => "side_effect",
            SeamKind::MatchArm => "match_arm",
            SeamKind::CallPresence => "call_presence",
        }
    }

    /// Test-only round-trip helper. The walker constructs `SeamKind`
    /// values directly from probe shape strings via
    /// `seam_inventory::seam_kind_from_probe_shape`; nothing in
    /// production code parses kind discriminants back into the enum.
    #[cfg(test)]
    pub(crate) fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "predicate_boundary" => SeamKind::PredicateBoundary,
            "error_variant" => SeamKind::ErrorVariant,
            "return_value" => SeamKind::ReturnValue,
            "field_construction" => SeamKind::FieldConstruction,
            "side_effect" => SeamKind::SideEffect,
            "match_arm" => SeamKind::MatchArm,
            "call_presence" => SeamKind::CallPresence,
            _ => return None,
        })
    }
}

/// What a test would need to observe to grip this seam.
///
/// The variant set tracks what the inventory walker can currently
/// emit. Spec variants (e.g. `BranchTaken` for validation branches)
/// will be added when `analysis/test-grip-evidence-v1` introduces
/// the corresponding detection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RequiredDiscriminator {
    BoundaryValue { description: String },
    ErrorVariant { variant: String },
    ReturnValue { description: String },
    FieldValue { field: String },
    Effect { sink: String },
    MatchArmTaken { arm: String },
    CallSite { target: String },
}

impl RequiredDiscriminator {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            RequiredDiscriminator::BoundaryValue { .. } => "boundary_value",
            RequiredDiscriminator::ErrorVariant { .. } => "error_variant",
            RequiredDiscriminator::ReturnValue { .. } => "return_value",
            RequiredDiscriminator::FieldValue { .. } => "field_value",
            RequiredDiscriminator::Effect { .. } => "effect",
            RequiredDiscriminator::MatchArmTaken { .. } => "match_arm_taken",
            RequiredDiscriminator::CallSite { .. } => "call_site",
        }
    }
}

/// Where a seam's effect would manifest — the sink class a test must
/// observe to discriminate the changed behavior. Subsequent inventory
/// and classification PRs populate this from existing flow-sink facts.
/// `Unknown` will be added back when a kind without a determinable
/// sink is detected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum ExpectedSink {
    ReturnValue,
    OutputField,
    ErrorChannel,
    SideEffect,
}

impl ExpectedSink {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            ExpectedSink::ReturnValue => "return_value",
            ExpectedSink::OutputField => "output_field",
            ExpectedSink::ErrorChannel => "error_channel",
            ExpectedSink::SideEffect => "side_effect",
        }
    }
}

/// Classification of how strongly current tests grip a seam.
///
/// The full set is locked in RIPR-SPEC-0005. The headline-eligibility
/// table on `is_headline_eligible` mirrors the spec's
/// "Headline Count vs Visible-Only Mapping" section and is consumed
/// by the upcoming `output/repo-exposure-report-v1` work item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum SeamGripClass {
    StronglyGripped,
    WeaklyGripped,
    Ungripped,
    ReachableUnrevealed,
    ActivationUnknown,
    PropagationUnknown,
    ObservationUnknown,
    DiscriminationUnknown,
    Opaque,
    Intentional,
    Suppressed,
}

impl SeamGripClass {
    /// Every grip class, in spec declaration order. Used by future
    /// renderers to enumerate per-class metric buckets.
    pub(crate) const ALL: [SeamGripClass; 11] = [
        SeamGripClass::StronglyGripped,
        SeamGripClass::WeaklyGripped,
        SeamGripClass::Ungripped,
        SeamGripClass::ReachableUnrevealed,
        SeamGripClass::ActivationUnknown,
        SeamGripClass::PropagationUnknown,
        SeamGripClass::ObservationUnknown,
        SeamGripClass::DiscriminationUnknown,
        SeamGripClass::Opaque,
        SeamGripClass::Intentional,
        SeamGripClass::Suppressed,
    ];

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            SeamGripClass::StronglyGripped => "strongly_gripped",
            SeamGripClass::WeaklyGripped => "weakly_gripped",
            SeamGripClass::Ungripped => "ungripped",
            SeamGripClass::ReachableUnrevealed => "reachable_unrevealed",
            SeamGripClass::ActivationUnknown => "activation_unknown",
            SeamGripClass::PropagationUnknown => "propagation_unknown",
            SeamGripClass::ObservationUnknown => "observation_unknown",
            SeamGripClass::DiscriminationUnknown => "discrimination_unknown",
            SeamGripClass::Opaque => "opaque",
            SeamGripClass::Intentional => "intentional",
            SeamGripClass::Suppressed => "suppressed",
        }
    }

    /// The plain word human output leads with, shared with
    /// `ExposureClass::plain_label`: a seam whose tests reach it but miss the
    /// discriminator reads `weak` in `pilot` exactly as the changed line reads
    /// `weak` in `ripr check`. The schema value (`as_str`) is unchanged.
    pub(crate) fn plain_label(&self) -> &'static str {
        match self {
            SeamGripClass::StronglyGripped => "exposed",
            SeamGripClass::WeaklyGripped => "weak",
            SeamGripClass::Ungripped => "no path",
            SeamGripClass::ReachableUnrevealed => "unrevealed",
            SeamGripClass::ActivationUnknown
            | SeamGripClass::PropagationUnknown
            | SeamGripClass::ObservationUnknown
            | SeamGripClass::DiscriminationUnknown
            | SeamGripClass::Opaque => "unknown",
            SeamGripClass::Intentional => "intentional",
            SeamGripClass::Suppressed => "suppressed",
        }
    }

    /// `plain word, schema value` for human lines, or the value alone when
    /// the two are the same (`intentional`, `suppressed`).
    pub(crate) fn human_label(&self) -> String {
        if self.plain_label() == self.as_str() {
            self.as_str().to_string()
        } else {
            format!("{}, {}", self.plain_label(), self.as_str())
        }
    }

    /// Parse a schema value back to its class, for renderers that read the
    /// value out of a JSON artifact.
    pub(crate) fn from_schema_value(value: &str) -> Option<SeamGripClass> {
        SeamGripClass::ALL
            .into_iter()
            .find(|class| class.as_str() == value)
    }

    /// Whether this class counts toward the headline badge per
    /// RIPR-SPEC-0005 § "Headline Count vs Visible-Only Mapping".
    /// `Opaque`'s headline treatment is decided by badge policy at
    /// render time; this method returns `false` so the model itself
    /// stays policy-free.
    pub(crate) fn is_headline_eligible(&self) -> bool {
        matches!(
            self,
            SeamGripClass::Ungripped
                | SeamGripClass::WeaklyGripped
                | SeamGripClass::ReachableUnrevealed
                | SeamGripClass::ActivationUnknown
                | SeamGripClass::PropagationUnknown
                | SeamGripClass::ObservationUnknown
                | SeamGripClass::DiscriminationUnknown
        )
    }

    /// Whether the classifier stopped on a stage it could not establish
    /// (`opaque`, or an `*_unknown` class), so the class names a static
    /// limitation rather than a gap. `classify_seam` owns this meaning: a
    /// gap class (`weakly_gripped`, `ungripped`, `reachable_unrevealed`) is
    /// one it reached a verdict for, though later stages may still be
    /// unknown (a `weakly_gripped` seam can carry unknown propagation or
    /// observation); this predicate filters on the class, not the stages
    /// (#5497).
    pub(crate) fn is_static_limitation(&self) -> bool {
        matches!(
            self,
            SeamGripClass::ActivationUnknown
                | SeamGripClass::PropagationUnknown
                | SeamGripClass::ObservationUnknown
                | SeamGripClass::DiscriminationUnknown
                | SeamGripClass::Opaque
        )
    }
}

/// A first-class behavior seam discovered in a production file.
///
/// The `id` is computed from the canonical fields by `RepoSeam::new`; do
/// not assemble seams via field literals at call sites, because that would
/// allow constructing a seam whose `id` does not match its fields.
/// Parser-owned source span of a seam's expression, 1-based line and column
/// with an end-exclusive end. Columns count Unicode scalar values from the
/// line start plus one, matching cargo-mutants span columns for calibration
/// joins (#5336). `None` on a seam means span geometry was unavailable
/// (legacy cache, a test fixture, or a shape kind whose parser range does
/// not cover its expression); consumers must fall back to line-only
/// behavior, never to zero coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SeamSpan {
    pub(crate) start_line: usize,
    pub(crate) start_column: usize,
    pub(crate) end_line: usize,
    pub(crate) end_column: usize,
}

/// Line-start byte offsets for `source`, so callers deriving many spans
/// from one file pay the scan once instead of once per shape.
pub(crate) fn build_line_starts(source: &str) -> Vec<usize> {
    let bytes = source.as_bytes();
    let mut line_starts = vec![0usize];
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            line_starts.push(index + 1);
        }
    }
    line_starts
}

/// Derive 1-based line/column geometry for a parser byte range. Returns
/// `None` (fail closed: the seam keeps line-only behavior) when the range
/// is inverted, empty, outside `source`, or not on character boundaries.
/// Test-only single-shot form; production reuses one per-file index via
/// `byte_span_to_lines_with_starts`.
#[cfg(test)]
pub(crate) fn byte_span_to_lines(
    source: &str,
    start_line: usize,
    start_byte: usize,
    end_byte: usize,
) -> Option<SeamSpan> {
    byte_span_to_lines_with_starts(
        source,
        &build_line_starts(source),
        start_line,
        start_byte,
        end_byte,
    )
}

/// `byte_span_to_lines` against a caller-owned per-file line index.
pub(crate) fn byte_span_to_lines_with_starts(
    source: &str,
    line_starts: &[usize],
    start_line: usize,
    start_byte: usize,
    end_byte: usize,
) -> Option<SeamSpan> {
    if start_byte >= end_byte || end_byte > source.len() {
        return None;
    }
    let line_col = |offset: usize| -> Option<(usize, usize)> {
        let line_idx = line_starts.partition_point(|start| *start <= offset);
        if line_idx == 0 {
            return None;
        }
        let line_start = line_starts[line_idx - 1];
        let column = source.get(line_start..offset)?.chars().count() + 1;
        Some((line_idx, column))
    };
    let (actual_start_line, start_column) = line_col(start_byte)?;
    if actual_start_line != start_line {
        return None;
    }
    let (end_line, end_column) = line_col(end_byte)?;
    Some(SeamSpan {
        start_line,
        start_column,
        end_line,
        end_column,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RepoSeam {
    id: SeamId,
    kind: SeamKind,
    file: PathBuf,
    owner: String,
    byte_offset: usize,
    display_line: usize,
    expression: String,
    required_discriminator: RequiredDiscriminator,
    expected_sink: ExpectedSink,
    span: Option<SeamSpan>,
    /// How a test calls the owner (#5357). Presentation only: not part of
    /// the seam ID. A seam deserialized from a cache entry written before
    /// the field existed reads `Unknown`, which renders no call.
    #[serde(default)]
    owner_call: OwnerCallShape,
}

/// How a test can call a seam's owner function, read from the parser's item
/// facts (#5357). Suggested assertions name the owner through this, so a
/// method is never presented as a free-function call.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum OwnerCallShape {
    /// Not established: lexical fallback, a cache entry older than the fact,
    /// a trait default method, a function-local `fn`, or an impl whose self
    /// type is not a plain named path. Renders no call syntax.
    #[default]
    Unknown,
    /// A module-level `fn`: `name(args)`.
    Free,
    /// An impl method with a `self` receiver: `<receiver>.name(args)`.
    Method { self_type: String },
    /// An impl associated function without `self`: `Type::name(args)`.
    Associated { self_type: String },
}

impl OwnerCallShape {
    /// Derive the call shape from the owner's parser facts. Every path that
    /// is not positively established stays `Unknown`.
    pub(crate) fn from_function(function: &crate::analysis::facts::FunctionFact) -> Self {
        use crate::analysis::facts::{FunctionContainer, FunctionImplContext};
        match (&function.item.container, &function.impl_context) {
            (FunctionContainer::Free, FunctionImplContext::Free) => Self::Free,
            (
                FunctionContainer::Inherent { .. } | FunctionContainer::TraitImpl { .. },
                FunctionImplContext::Impl { self_type },
            ) if !self_type.trim().is_empty() => {
                let self_type = self_type.clone();
                if function.item.has_self_param {
                    Self::Method { self_type }
                } else {
                    Self::Associated { self_type }
                }
            }
            _ => Self::Unknown,
        }
    }

    /// Render a call of `name` with `arguments` placed between the
    /// parentheses. `Unknown` renders a placeholder comment naming the owner
    /// instead of a call that may not compile; an `arguments` placeholder
    /// comment is folded into it.
    pub(crate) fn call(&self, name: &str, arguments: &str) -> String {
        match self {
            Self::Free => format!("{name}({arguments})"),
            Self::Method { self_type } => {
                format!("/* {self_type} value */.{name}({arguments})")
            }
            Self::Associated { self_type } => format!("{self_type}::{name}({arguments})"),
            Self::Unknown => {
                let inner = arguments
                    .trim()
                    .strip_prefix("/*")
                    .and_then(|rest| rest.strip_suffix("*/"))
                    .unwrap_or(arguments)
                    .trim();
                format!("/* call {name} (receiver or path not established) with {inner} */")
            }
        }
    }
}

impl RepoSeam {
    /// Construct a synthetic or already-normalized seam: the ID hashes the
    /// byte offset exactly as given.
    ///
    /// A producer that measured `byte_offset` against a real file's raw text
    /// must use [`RepoSeam::new_in_source`] instead, so line-ending spelling
    /// cannot move the ID (#7203). This constructor stays for test fixtures
    /// and synthetic profiles whose offsets carry no file spelling.
    ///
    /// `expression` is the source-code text at the seam origin and is
    /// surfaced verbatim in human/JSON output. It is *not* part of the
    /// canonical ID hash, so reformatting whitespace within an expression
    /// does not change `SeamId`.
    // Eight fields are intrinsic to a seam's identity and presentation;
    // grouping them into nested structs would force every call site
    // through extra constructors without making the data simpler.
    #[expect(
        clippy::too_many_arguments,
        reason = "Eight fields are intrinsic to a seam's identity and presentation; grouping them into nested structs forces every call site through extra constructors without simplifying the data."
    )]
    pub(crate) fn new(
        file: impl AsRef<Path>,
        owner: impl Into<String>,
        kind: SeamKind,
        byte_offset: usize,
        display_line: usize,
        expression: impl Into<String>,
        required_discriminator: RequiredDiscriminator,
        expected_sink: ExpectedSink,
    ) -> Self {
        RepoSeam::from_canonical_parts(
            file,
            owner,
            kind,
            byte_offset,
            byte_offset,
            display_line,
            expression,
            required_discriminator,
            expected_sink,
        )
    }

    /// Construct a seam whose `byte_offset` was measured against `source`,
    /// hashing the ID's local coordinate over the file's logical content
    /// with CRLF pairs normalized to LF (#7203).
    ///
    /// The stored [`RepoSeam::byte_offset`] stays the raw offset into
    /// `source`: rendering, span geometry and stub placement index the
    /// actual file bytes. Only the ID reads the normalized coordinate — the
    /// offset the seam would have in the same text with CRLF pairs spelled
    /// LF — so a checkout's line-ending spelling (`core.autocrlf=true`)
    /// cannot split one logical seam into `new`+`removed` across snapshots
    /// (RIPR-SPEC-0005 § "Stable Seam ID Rules"). This is the
    /// local-coordinate counterpart of [`normalize_path`], and mirrors the
    /// input-identity normalization `normalize_workspace_file_bytes`
    /// applies since #3118: CRLF and LF spell the same line break, while a
    /// standalone CR is preserved so invalid text cannot collide with a
    /// valid LF input.
    #[expect(
        clippy::too_many_arguments,
        reason = "The source-backed twin of `new`; it carries the same eight intrinsic fields plus the source the offset was measured against."
    )]
    pub(crate) fn new_in_source(
        file: impl AsRef<Path>,
        owner: impl Into<String>,
        kind: SeamKind,
        byte_offset: usize,
        display_line: usize,
        expression: impl Into<String>,
        required_discriminator: RequiredDiscriminator,
        expected_sink: ExpectedSink,
        source: &str,
    ) -> Self {
        let identity_offset = normalized_identity_byte_offset(source, byte_offset);
        RepoSeam::from_canonical_parts(
            file,
            owner,
            kind,
            byte_offset,
            identity_offset,
            display_line,
            expression,
            required_discriminator,
            expected_sink,
        )
    }

    /// Shared constructor: `identity_offset` is the local coordinate the ID
    /// hashes; `byte_offset` is the raw coordinate stored for rendering.
    #[expect(
        clippy::too_many_arguments,
        reason = "Internal funnel for `new`/`new_in_source`; the fields are the seam's intrinsic identity and presentation set."
    )]
    fn from_canonical_parts(
        file: impl AsRef<Path>,
        owner: impl Into<String>,
        kind: SeamKind,
        byte_offset: usize,
        identity_offset: usize,
        display_line: usize,
        expression: impl Into<String>,
        required_discriminator: RequiredDiscriminator,
        expected_sink: ExpectedSink,
    ) -> Self {
        let file_normalized = normalize_path(file.as_ref());
        let owner = owner.into();
        let id = compute_seam_id(&file_normalized, &owner, kind, identity_offset);
        RepoSeam {
            id,
            kind,
            file: PathBuf::from(file_normalized),
            owner,
            byte_offset,
            display_line,
            expression: expression.into(),
            required_discriminator,
            expected_sink,
            span: None,
            owner_call: OwnerCallShape::Unknown,
        }
    }

    /// Attach parser-owned span geometry. The seam ID is computed from
    /// file/owner/kind/byte offset only, so spans never change identity.
    pub(crate) fn with_span(mut self, span: SeamSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Attach the owner's call shape read from the parser (#5357).
    pub(crate) fn with_owner_call(mut self, owner_call: OwnerCallShape) -> Self {
        self.owner_call = owner_call;
        self
    }

    pub(crate) fn id(&self) -> &SeamId {
        &self.id
    }
    pub(crate) fn kind(&self) -> SeamKind {
        self.kind
    }
    pub(crate) fn file(&self) -> &Path {
        &self.file
    }
    pub(crate) fn owner(&self) -> &str {
        &self.owner
    }
    pub(crate) fn byte_offset(&self) -> usize {
        self.byte_offset
    }
    pub(crate) fn display_line(&self) -> usize {
        self.display_line
    }
    pub(crate) fn span(&self) -> Option<SeamSpan> {
        self.span
    }
    pub(crate) fn expression(&self) -> &str {
        &self.expression
    }
    pub(crate) fn required_discriminator(&self) -> &RequiredDiscriminator {
        &self.required_discriminator
    }
    pub(crate) fn expected_sink(&self) -> ExpectedSink {
        self.expected_sink
    }
    pub(crate) fn owner_call(&self) -> &OwnerCallShape {
        &self.owner_call
    }
}

/// Repo-root-relative path normalization: Unix separators, no leading `./`.
/// Used inside the ID hash so `src/x.rs`, `./src/x.rs`, and `src\x.rs`
/// produce the same seam ID across platforms.
fn normalize_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    s.strip_prefix("./").unwrap_or(&s).to_string()
}

/// The seam ID's local coordinate for a byte offset measured against raw
/// `source`: the offset the same byte has after CRLF→LF normalization.
///
/// Each `\r\n` pair before the offset shifts it by one byte; an offset that
/// points at the LF of a pair lands where that pair collapses. A standalone
/// CR is preserved, mirroring `normalize_workspace_file_bytes` (#3118):
/// only a CRLF pair spells the same line break as LF, so text holding a
/// lone CR cannot be read as the LF spelling of different content (#7203).
/// An offset beyond `source`'s length clamps to the normalized length
/// instead of underflowing.
fn normalized_identity_byte_offset(source: &str, byte_offset: usize) -> usize {
    let bytes = source.as_bytes();
    let bound = byte_offset.min(bytes.len());
    let mut pairs = 0usize;
    for index in 0..bound {
        if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
            pairs += 1;
        }
    }
    bound - pairs
}

/// FNV-1a 64-bit hash of the canonical seam fields, encoded as a 16-char
/// lowercase hex string.
///
/// FNV-1a is chosen because it is simple, has no third-party dependency,
/// and is stable across Rust versions — unlike
/// `std::collections::hash_map::DefaultHasher`, which is intentionally not
/// stable across releases. The hash never reads time, walk order, process
/// ID, or any other ambient state.
///
/// This uses the **same FNV-1a constants** as the Perl gap ID
/// (`crates/ripr/src/analysis/language/perl/mod.rs:3249`), the canonical
/// gap ID (`crates/ripr/src/analysis/canonical_gap.rs:128`), and the seam
/// cache (`crates/ripr/src/analysis/seam_cache.rs:1411`). Deliberate
/// parity: all gap/seam IDs across languages use one scheme. See #1722.
fn compute_seam_id(file: &str, owner: &str, kind: SeamKind, identity_offset: usize) -> SeamId {
    // FNV-1a constants — deliberate parity with Perl adapter
    // (crates/ripr/src/analysis/language/perl/mod.rs). Both sides must use
    // identical constants so Rust and Perl gap IDs are comparable (#1722).
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    // Null byte separator avoids collisions: `\0` cannot appear in POSIX
    // or Windows file paths, in Rust module/identifier names that make up
    // owner symbols, in our static kind discriminants, or in the decimal
    // stringification of `identity_offset`.
    let offset_str = identity_offset.to_string();
    let parts: [&[u8]; 7] = [
        file.as_bytes(),
        b"\0",
        owner.as_bytes(),
        b"\0",
        kind.as_str().as_bytes(),
        b"\0",
        offset_str.as_bytes(),
    ];
    let mut hash: u64 = FNV_OFFSET;
    for part in parts {
        for byte in part {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    SeamId(format!("{hash:016x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grip_and_exposure_counterparts_share_one_plain_word() {
        use crate::domain::ExposureClass;
        // The same judgment at seam scope (`pilot`) and changed-line scope
        // (`check`) must read the same to a person; the values stay distinct.
        let pairs = [
            (SeamGripClass::StronglyGripped, ExposureClass::Exposed),
            (SeamGripClass::WeaklyGripped, ExposureClass::WeaklyExposed),
            (SeamGripClass::Ungripped, ExposureClass::NoStaticPath),
            (
                SeamGripClass::ReachableUnrevealed,
                ExposureClass::ReachableUnrevealed,
            ),
            (
                SeamGripClass::PropagationUnknown,
                ExposureClass::PropagationUnknown,
            ),
        ];
        for (grip, exposure) in pairs {
            assert_eq!(grip.plain_label(), exposure.plain_label(), "{grip:?}");
        }
        assert_eq!(SeamGripClass::WeaklyGripped.plain_label(), "weak");
        for class in SeamGripClass::ALL {
            assert_eq!(
                SeamGripClass::from_schema_value(class.as_str()),
                Some(class)
            );
        }
        assert_eq!(SeamGripClass::from_schema_value("weakly_exposed"), None);
    }

    fn make_seam(file: &str, owner: &str, kind: SeamKind, off: usize) -> RepoSeam {
        RepoSeam::new(
            file,
            owner,
            kind,
            off,
            1,
            "amount >= threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        )
    }

    #[test]
    fn byte_span_derives_one_based_line_and_columns() -> Result<(), String> {
        let source = "pub fn f() { a + b }\nlet z = 1;\n";
        // "a + b" occupies bytes 13..18 on line 1.
        let span =
            byte_span_to_lines(source, 1, 13, 18).ok_or_else(|| "span must derive".to_string())?;
        assert_eq!(span.start_line, 1);
        assert_eq!(span.start_column, 14);
        assert_eq!(span.end_line, 1);
        assert_eq!(span.end_column, 19);
        Ok(())
    }

    #[test]
    fn byte_span_tracks_multiline_ranges() -> Result<(), String> {
        let source = "fn f() {\n    a + b\n}\n";
        // bytes 13..20 run from "a" on line 2 to the newline ending line 3.
        let span =
            byte_span_to_lines(source, 2, 13, 20).ok_or_else(|| "span must derive".to_string())?;
        assert_eq!((span.start_line, span.start_column), (2, 5));
        assert_eq!((span.end_line, span.end_column), (3, 2));
        Ok(())
    }

    #[test]
    fn byte_span_columns_count_chars_not_bytes() -> Result<(), String> {
        // Greek alpha is two bytes in UTF-8; cargo-mutants columns are
        // 1-based character columns (measured against cargo-mutants 26.1.2
        // list output), so the seam projection matches that unit. The plus
        // sits after the multibyte char, where byte and char columns differ.
        let source = "fn f() { α + β }\n";
        let plus_at = source
            .find('+')
            .ok_or_else(|| "plus must exist".to_string())?;
        let span = byte_span_to_lines(source, 1, plus_at, plus_at + 1)
            .ok_or_else(|| "span must derive".to_string())?;
        // Byte column would be plus_at + 1 = 13; the character column is 12.
        assert_eq!(span.start_column, 12);
        assert_eq!(span.end_column, 13);
        Ok(())
    }

    #[test]
    fn byte_span_with_shared_starts_matches_single_shot() -> Result<(), String> {
        let source = "fn f() {\n    a + b\n}\n";
        let starts = build_line_starts(source);
        let (shared, single) = (
            byte_span_to_lines_with_starts(source, &starts, 2, 13, 18),
            byte_span_to_lines(source, 2, 13, 18),
        );
        assert_eq!(shared, single);
        let span = shared.ok_or_else(|| "span must derive".to_string())?;
        assert_eq!((span.start_line, span.start_column), (2, 5));
        assert_eq!((span.end_line, span.end_column), (2, 10));
        Ok(())
    }

    #[test]
    fn byte_span_refuses_mid_character_offsets() -> Result<(), String> {
        // Splitting the two-byte alpha is not a character boundary, so both
        // endpoints fail closed instead of emitting shifted columns.
        let source = "fn f() { α }\n";
        let alpha_at = source
            .find('α')
            .ok_or_else(|| "alpha must exist".to_string())?;
        assert_eq!(
            byte_span_to_lines(source, 1, alpha_at + 1, alpha_at + 3),
            None
        );
        assert_eq!(byte_span_to_lines(source, 1, alpha_at, alpha_at + 1), None);
        Ok(())
    }

    #[test]
    fn byte_span_fails_closed_on_bad_geometry() {
        let source = "fn f() { a }\n";
        // Inverted, empty, out-of-range, and line-mismatched ranges all
        // refuse rather than emit wrong coordinates.
        assert_eq!(byte_span_to_lines(source, 1, 8, 8), None);
        assert_eq!(byte_span_to_lines(source, 1, 9, 8), None);
        assert_eq!(byte_span_to_lines(source, 1, 8, source.len() + 1), None);
        assert_eq!(byte_span_to_lines(source, 2, 8, 9), None);
    }

    #[test]
    fn seam_id_is_stable_when_span_attaches() {
        let span = SeamSpan {
            start_line: 1,
            start_column: 14,
            end_line: 1,
            end_column: 19,
        };
        let base = make_seam(
            "src/pricing.rs",
            "pricing::quote",
            SeamKind::PredicateBoundary,
            88,
        );
        let with_span = make_seam(
            "src/pricing.rs",
            "pricing::quote",
            SeamKind::PredicateBoundary,
            88,
        )
        .with_span(span);
        assert_eq!(base.id(), with_span.id());
        assert_eq!(with_span.span(), Some(span));
        assert_eq!(base.span(), None);
    }

    #[test]
    fn seam_id_is_deterministic_for_identical_inputs() {
        let a = make_seam(
            "src/pricing.rs",
            "pricing::quote",
            SeamKind::PredicateBoundary,
            88,
        );
        let b = make_seam(
            "src/pricing.rs",
            "pricing::quote",
            SeamKind::PredicateBoundary,
            88,
        );
        assert_eq!(a.id(), b.id());
    }

    #[test]
    fn seam_id_differs_when_any_canonical_field_differs() {
        let base = make_seam(
            "src/pricing.rs",
            "pricing::quote",
            SeamKind::PredicateBoundary,
            88,
        );
        let other_file = make_seam(
            "src/checkout.rs",
            "pricing::quote",
            SeamKind::PredicateBoundary,
            88,
        );
        let other_owner = make_seam(
            "src/pricing.rs",
            "pricing::compute",
            SeamKind::PredicateBoundary,
            88,
        );
        let other_kind = make_seam(
            "src/pricing.rs",
            "pricing::quote",
            SeamKind::ReturnValue,
            88,
        );
        let other_offset = make_seam(
            "src/pricing.rs",
            "pricing::quote",
            SeamKind::PredicateBoundary,
            89,
        );

        assert_ne!(base.id(), other_file.id());
        assert_ne!(base.id(), other_owner.id());
        assert_ne!(base.id(), other_kind.id());
        assert_ne!(base.id(), other_offset.id());
    }

    #[test]
    fn seam_ids_do_not_depend_on_construction_order() -> Result<(), String> {
        let inputs = [
            ("src/a.rs", "a::f", SeamKind::PredicateBoundary, 10),
            ("src/b.rs", "b::g", SeamKind::ErrorVariant, 20),
            ("src/c.rs", "c::h", SeamKind::ReturnValue, 30),
        ];

        let forward: Vec<String> = inputs
            .iter()
            .map(|(f, o, k, off)| make_seam(f, o, *k, *off).id().as_str().to_string())
            .collect();

        let mut reversed: Vec<String> = inputs
            .iter()
            .rev()
            .map(|(f, o, k, off)| make_seam(f, o, *k, *off).id().as_str().to_string())
            .collect();
        reversed.reverse();

        if forward != reversed {
            return Err("seam IDs depend on construction order".to_string());
        }
        Ok(())
    }

    #[test]
    fn seam_id_normalizes_windows_path_separators() {
        let unix = make_seam(
            "src/pricing.rs",
            "pricing::quote",
            SeamKind::PredicateBoundary,
            88,
        );
        let windows = make_seam(
            "src\\pricing.rs",
            "pricing::quote",
            SeamKind::PredicateBoundary,
            88,
        );
        assert_eq!(unix.id(), windows.id());
    }

    #[test]
    fn seam_id_normalizes_leading_dot_slash() {
        let plain = make_seam(
            "src/pricing.rs",
            "pricing::quote",
            SeamKind::PredicateBoundary,
            88,
        );
        let dotted = make_seam(
            "./src/pricing.rs",
            "pricing::quote",
            SeamKind::PredicateBoundary,
            88,
        );
        assert_eq!(plain.id(), dotted.id());
    }

    /// #7203: the ID's local coordinate is the offset in the file's logical
    /// CRLF→LF-normalized content, so the same source spelled with CRLF
    /// terminators (`core.autocrlf=true` checkout) hashes to the same seam
    /// ID as the LF spelling, while the stored raw offset — what rendering
    /// and span geometry index — keeps its position in the actual bytes.
    #[test]
    fn seam_in_source_hashes_the_lf_normalized_offset_and_keeps_the_raw_one() -> Result<(), String>
    {
        let lf = "fn f(x: i32) -> bool {\n    x >= 0\n}\n";
        let crlf = lf.replace('\n', "\r\n");
        let lf_offset = lf
            .find("x >= 0")
            .ok_or_else(|| "fixture predicate must exist".to_string())?;
        let crlf_offset = crlf
            .find("x >= 0")
            .ok_or_else(|| "fixture predicate must exist".to_string())?;
        let lf_seam = RepoSeam::new_in_source(
            "src/pricing.rs",
            "pricing::f",
            SeamKind::PredicateBoundary,
            lf_offset,
            2,
            "x >= 0",
            RequiredDiscriminator::BoundaryValue {
                description: "x >= 0".to_string(),
            },
            ExpectedSink::ReturnValue,
            lf,
        );
        let crlf_seam = RepoSeam::new_in_source(
            "src/pricing.rs",
            "pricing::f",
            SeamKind::PredicateBoundary,
            crlf_offset,
            2,
            "x >= 0",
            RequiredDiscriminator::BoundaryValue {
                description: "x >= 0".to_string(),
            },
            ExpectedSink::ReturnValue,
            &crlf,
        );
        // The fixture must exercise the shift: the CRLF spelling moves the
        // raw byte behind line 1's two-byte terminator.
        assert_ne!(
            lf_seam.byte_offset(),
            crlf_seam.byte_offset(),
            "fixture must place the predicate behind a CRLF pair"
        );
        assert_eq!(
            lf_seam.id(),
            crlf_seam.id(),
            "line-ending spelling must not move the seam ID"
        );
        assert_eq!(
            crlf_seam.byte_offset(),
            crlf_offset,
            "the stored offset stays raw so rendering indexes the real bytes"
        );
        Ok(())
    }

    /// Existing LF-only inventories keep their IDs byte-for-byte: for an LF
    /// source the normalized coordinate equals the raw offset, so
    /// `new_in_source` agrees with the offset-as-given constructor.
    #[test]
    fn seam_in_source_matches_the_legacy_id_for_lf_sources() -> Result<(), String> {
        let source = "fn f(x: i32) -> bool {\n    x >= 0\n}\n";
        let offset = source
            .find("x >= 0")
            .ok_or_else(|| "fixture predicate must exist".to_string())?;
        let from_source = RepoSeam::new_in_source(
            "src/pricing.rs",
            "pricing::f",
            SeamKind::PredicateBoundary,
            offset,
            2,
            "x >= 0",
            RequiredDiscriminator::BoundaryValue {
                description: "x >= 0".to_string(),
            },
            ExpectedSink::ReturnValue,
            source,
        );
        let legacy = RepoSeam::new(
            "src/pricing.rs",
            "pricing::f",
            SeamKind::PredicateBoundary,
            offset,
            2,
            "x >= 0",
            RequiredDiscriminator::BoundaryValue {
                description: "x >= 0".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        assert_eq!(from_source.id(), legacy.id());
        Ok(())
    }

    /// Only a CRLF pair spells a line break for identity purposes. A
    /// standalone CR shifts nothing, mirroring
    /// `normalize_workspace_file_bytes` (#3118): treating lone CR as a
    /// removed terminator would collide text that never held a line break
    /// with the LF spelling of different content.
    #[test]
    fn identity_offset_normalizes_crlf_pairs_and_preserves_standalone_cr() {
        // "ab\r\ncd": the 'd' sits at raw 5, normalized 4 — behind one pair.
        assert_eq!(normalized_identity_byte_offset("ab\r\ncd", 5), 4);
        // LF-only coordinates pass through unchanged.
        assert_eq!(normalized_identity_byte_offset("ab\ncd", 4), 4);
        // A standalone CR is not a line break: nothing shifts.
        assert_eq!(normalized_identity_byte_offset("ab\rcd", 4), 4);
        // An offset pointing at the CR of a pair is still at the pair.
        assert_eq!(normalized_identity_byte_offset("ab\r\ncd", 2), 2);
        // An offset pointing at the LF of a pair lands where the pair
        // collapses to.
        assert_eq!(normalized_identity_byte_offset("ab\r\ncd", 3), 2);
        // An offset past the source end saturates to the present pairs.
        assert_eq!(normalized_identity_byte_offset("ab\r\n", 100), 3);
        assert_eq!(normalized_identity_byte_offset("", 0), 0);
    }

    #[test]
    fn seam_id_is_16_lowercase_hex_chars() -> Result<(), String> {
        let seam = make_seam("src/x.rs", "x::y", SeamKind::PredicateBoundary, 0);
        let id = seam.id().as_str();
        if id.len() != 16 {
            return Err(format!(
                "seam id should be 16 chars, got {}: {id}",
                id.len()
            ));
        }
        for c in id.chars() {
            if !c.is_ascii_hexdigit() {
                return Err(format!("seam id should be hex, got: {id}"));
            }
            if c.is_ascii_alphabetic() && !c.is_ascii_lowercase() {
                return Err(format!("seam id hex should be lowercase, got: {id}"));
            }
        }
        Ok(())
    }

    #[test]
    fn seam_kind_round_trips_through_str() -> Result<(), String> {
        let all = [
            SeamKind::PredicateBoundary,
            SeamKind::ErrorVariant,
            SeamKind::ReturnValue,
            SeamKind::FieldConstruction,
            SeamKind::SideEffect,
            SeamKind::MatchArm,
            SeamKind::CallPresence,
        ];
        for kind in all {
            let s = kind.as_str();
            let parsed = SeamKind::from_str(s)
                .ok_or_else(|| format!("SeamKind::from_str rejected its own as_str: {s}"))?;
            if parsed != kind {
                return Err(format!("round-trip failed for {s}"));
            }
        }
        if SeamKind::from_str("nonsense").is_some() {
            return Err("SeamKind::from_str should reject unknown strings".to_string());
        }
        Ok(())
    }

    #[test]
    fn required_discriminator_carries_kind_via_as_str() {
        let cases: &[(RequiredDiscriminator, &str)] = &[
            (
                RequiredDiscriminator::BoundaryValue {
                    description: "amount >= threshold".to_string(),
                },
                "boundary_value",
            ),
            (
                RequiredDiscriminator::ErrorVariant {
                    variant: "QuoteError::Insolvent".to_string(),
                },
                "error_variant",
            ),
            (
                RequiredDiscriminator::ReturnValue {
                    description: "non-zero discount".to_string(),
                },
                "return_value",
            ),
            (
                RequiredDiscriminator::FieldValue {
                    field: "Discount.amount".to_string(),
                },
                "field_value",
            ),
            (
                RequiredDiscriminator::Effect {
                    sink: "log::error".to_string(),
                },
                "effect",
            ),
            (
                RequiredDiscriminator::MatchArmTaken {
                    arm: "Pricing::Premium".to_string(),
                },
                "match_arm_taken",
            ),
            (
                RequiredDiscriminator::CallSite {
                    target: "metrics::record".to_string(),
                },
                "call_site",
            ),
        ];
        for (case, expected) in cases {
            assert_eq!(case.as_str(), *expected);
        }
    }

    #[test]
    fn expected_sink_str_covers_all_variants() {
        let all = [
            (ExpectedSink::ReturnValue, "return_value"),
            (ExpectedSink::OutputField, "output_field"),
            (ExpectedSink::ErrorChannel, "error_channel"),
            (ExpectedSink::SideEffect, "side_effect"),
        ];
        for (sink, expected) in all {
            assert_eq!(sink.as_str(), expected);
        }
    }

    #[test]
    fn repo_seam_accessors_round_trip_construction_inputs() -> Result<(), String> {
        let seam = RepoSeam::new(
            "src/pricing.rs",
            "pricing::check_discount",
            SeamKind::PredicateBoundary,
            1234,
            88,
            "amount >= discount_threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= discount_threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        assert_eq!(seam.kind(), SeamKind::PredicateBoundary);
        assert_eq!(seam.owner(), "pricing::check_discount");
        assert_eq!(seam.byte_offset(), 1234);
        assert_eq!(seam.display_line(), 88);
        assert_eq!(seam.expression(), "amount >= discount_threshold");
        assert_eq!(seam.expected_sink(), ExpectedSink::ReturnValue);
        assert_eq!(seam.file().to_string_lossy(), "src/pricing.rs");
        match seam.required_discriminator() {
            RequiredDiscriminator::BoundaryValue { description } => {
                assert_eq!(description, "amount >= discount_threshold");
                Ok(())
            }
            other => Err(format!("expected BoundaryValue, got {}", other.as_str())),
        }
    }

    /// #5357: the call shape decides the call syntax. Before the fix every
    /// shape rendered as the `Free` form, so the method and associated cases
    /// below failed and the `Unknown` case presented a free call.
    #[test]
    fn owner_call_shape_renders_receiver_path_or_honest_placeholder() {
        let hint = "/* boundary input where unit == 0 */";
        assert_eq!(
            OwnerCallShape::Free.call("clamp_units", hint),
            "clamp_units(/* boundary input where unit == 0 */)"
        );
        assert_eq!(
            OwnerCallShape::Method {
                self_type: "ByteSize".to_string()
            }
            .call("as_whole_units", hint),
            "/* ByteSize value */.as_whole_units(/* boundary input where unit == 0 */)"
        );
        assert_eq!(
            OwnerCallShape::Associated {
                self_type: "ByteSize".to_string()
            }
            .call("from_kib", "/* input */"),
            "ByteSize::from_kib(/* input */)"
        );
        let unknown = OwnerCallShape::Unknown.call("as_whole_units", hint);
        assert_eq!(
            unknown,
            "/* call as_whole_units (receiver or path not established) with boundary input where unit == 0 */"
        );
        assert!(
            !unknown.contains("as_whole_units("),
            "an unestablished shape must not present a call: {unknown}"
        );
        assert_eq!(
            OwnerCallShape::Unknown.call("emit", "..."),
            "/* call emit (receiver or path not established) with ... */"
        );
    }

    /// A seam built without parser facts, or read from a cache entry older
    /// than the field, has no established call shape.
    #[test]
    fn owner_call_defaults_to_unknown_for_new_and_legacy_seams() -> Result<(), String> {
        let seam = make_seam("src/lib.rs", "src/lib.rs::f", SeamKind::ReturnValue, 0);
        assert_eq!(seam.owner_call(), &OwnerCallShape::Unknown);
        let mut legacy = serde_json::to_value(&seam).map_err(|err| err.to_string())?;
        legacy
            .as_object_mut()
            .ok_or("seam serializes as an object")?
            .remove("owner_call");
        let restored: RepoSeam = serde_json::from_value(legacy).map_err(|err| err.to_string())?;
        assert_eq!(restored.owner_call(), &OwnerCallShape::Unknown);
        let shaped = seam.with_owner_call(OwnerCallShape::Free);
        assert_eq!(shaped.owner_call(), &OwnerCallShape::Free);
        Ok(())
    }
}
