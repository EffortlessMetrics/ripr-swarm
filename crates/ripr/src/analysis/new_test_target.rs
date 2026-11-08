//! Producer-owned `NewTestTargetProposal` admission.
//!
//! InlineUnit (#4784) may earn one exact insertion into an already-governed
//! inline cfg-test module. Integration (#4576) may earn one new `tests/` file
//! for a crate-root public library item with established autodiscovery.
//! Neither invents expected values or generates a test body.

use crate::analysis::facts::FunctionSourceRole;
use crate::analysis::language::is_generated_rust_file_with_patterns;
use crate::analysis::rust_index::{self, FunctionSummary, RustIndex};
use crate::analysis::seams::{RepoSeam, SeamKind};
use crate::analysis::syntax::{GovernedCfgTestModule, inline_unit_module_layout};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

mod integration;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod preflight_tests;
mod region;
#[cfg(test)]
mod tests;

pub(crate) use region::InlineTestRegionAuthority;
#[cfg(test)]
pub(crate) use region::validate_inline_region_edit;

const SAFE_NEW_INLINE_UNIT_EVIDENCE: &str = "producer-owned new inline unit test proposal";

#[cfg(test)]
pub(crate) use integration::admit_new_integration_test;
pub(crate) use integration::library_crate_name_for_root_file;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct NewTestTargetAdmission {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) proposal: Option<NewTestTargetProposal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) region: Option<InlineTestRegionAuthority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) blocker: Option<NewTestProposalBlocker>,
    /// The owner file's one governed inline cfg-test module region, admitted
    /// by the InlineUnit law whichever target the ranking selects (#5210). A
    /// repair whose recommended target is that same production file may be
    /// routed into this module; the edit cage re-captures and enforces it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) owner_inline_region: Option<InlineTestRegionAuthority>,
}

impl NewTestTargetAdmission {
    pub(crate) fn missing_reason(&self) -> Option<String> {
        self.blocker
            .as_ref()
            .map(|blocker| format!("new test proposal blocked: {}", blocker.as_str()))
    }

    pub(crate) fn present_reason(&self) -> &'static str {
        match self.proposal.as_ref().map(|proposal| proposal.kind) {
            Some(NewTestKind::Integration) => integration::integration_present_reason(),
            _ => SAFE_NEW_INLINE_UNIT_EVIDENCE,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NewTestProposalBlocker {
    NoTestModule,
    AmbiguousModule,
    OutOfLineModule,
    CustomHarness,
    GeneratedOrVendor,
    OwnerUnresolved,
    OwnerInaccessible,
    LexicalFallback,
    PathUnsafe,
    StaleSource,
    ProductionEdit,
    InlineUnitOutOfScope,
    PrivateOwner,
    AutotestsDisabled,
    MissingIntegrationLayout,
    LibraryTargetUnresolved,
    FileCollision,
}

impl NewTestProposalBlocker {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NoTestModule => "no governed inline cfg-test module exists in the owner file",
            Self::AmbiguousModule => {
                "more than one governed inline test module or insertion anchor is plausible"
            }
            Self::OutOfLineModule => {
                "the only test module is out-of-line; V1 does not follow mod tests;"
            }
            Self::CustomHarness => {
                "owner file is a custom test harness, not an ordinary inline module"
            }
            Self::GeneratedOrVendor => {
                "owner file is generated, vendored, or otherwise outside the governed source set"
            }
            Self::OwnerUnresolved => "production owner is unresolved",
            Self::OwnerInaccessible => {
                "the selected inline test module does not have ordinary Rust access to the owner"
            }
            Self::LexicalFallback => {
                "owner file used lexical fallback; module region identity is not established"
            }
            Self::PathUnsafe => "owner source path is not a root-contained production file",
            Self::StaleSource => {
                "region authority source digest does not match the supplied before text"
            }
            Self::ProductionEdit => {
                "edit changed production text outside the governed inline test-module region"
            }
            Self::InlineUnitOutOfScope => {
                "inline unit insertion is out of scope for this seam or file"
            }
            Self::PrivateOwner => {
                "owner is private and would require a production visibility change"
            }
            Self::AutotestsDisabled => "cargo autotests discovery is disabled",
            Self::MissingIntegrationLayout => "package has no established tests/ layout",
            Self::LibraryTargetUnresolved => "exact public library target is unresolved",
            Self::FileCollision => "proposed integration file collides with an existing path",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct NewTestTargetProposal {
    pub(crate) kind: NewTestKind,
    pub(crate) file: PathBuf,
    pub(crate) owner: String,
    pub(crate) provenance: NewTestProposalProvenance,
    /// Portable workspace identity the producer minted from the same
    /// workspace authority that names an existing test target's identity
    /// (#13203 A0). Empty when the authority could not name it, which the
    /// repair-start gate treats as unnameable: the card never fabricates one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) workspace_identity: String,
}

/// The one projection of the selected new-test proposal (#13204). The CLI
/// repair card, the agent packet and MCP read the kind, file and (for an
/// inline module) the module name from here, so no surface chooses its own
/// target or wording. No insertion anchor exists beyond the inline module
/// name; this type invents none, selects nothing and generates no test.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct SelectedProposal {
    pub(crate) kind: NewTestKind,
    pub(crate) file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) module: Option<String>,
}

impl SelectedProposal {
    /// Projects the producer-owned proposal. The file spelling is the stable
    /// forward-slash path text, which for a repository-relative path is what
    /// `display_path` rendered (the analysis layer may not call output).
    pub(crate) fn from_proposal(proposal: &NewTestTargetProposal) -> Self {
        Self {
            kind: proposal.kind,
            file: crate::analysis::stable_path_text(&proposal.file),
            module: None,
        }
    }

    /// Projects the admitted proposal together with the governed inline
    /// module the same admission captured for it, for the surfaces that hold
    /// the whole admission (an InlineUnit proposal's module is its `region`).
    pub(crate) fn from_admission(admission: &NewTestTargetAdmission) -> Option<Self> {
        let proposal = admission.proposal.as_ref()?;
        let selected = Self::from_proposal(proposal);
        let module = match (proposal.kind, admission.region.as_ref()) {
            (NewTestKind::InlineUnit, Some(region)) if region.file == proposal.file => {
                Some(region.module_name.as_str())
            }
            _ => None,
        };
        Some(selected.with_module(module))
    }

    /// Adds the governed inline module the producer admitted for the same
    /// file, for the surfaces that hold the region.
    pub(crate) fn with_module(mut self, module: Option<&str>) -> Self {
        self.module = module.map(str::to_string);
        self
    }

    /// The plain-language statement of the selected proposal.
    pub(crate) fn statement(&self) -> String {
        match (&self.kind, &self.module) {
            (NewTestKind::Integration, _) => {
                format!("add a new integration test file `{}`", self.file)
            }
            (NewTestKind::InlineUnit, Some(module)) => {
                format!("add a test inside the `{module}` module of `{}`", self.file)
            }
            (NewTestKind::InlineUnit, None) => {
                format!("add a test in the inline test module of `{}`", self.file)
            }
        }
    }
}

/// The premise a new-test proposal rests on, shared by the bind-time check and
/// the read-only preflight so one rule decides staleness (#13205): an
/// integration proposal names a path that must hold nothing yet, an inline
/// proposal an existing production file. `exists` is the caller's observation
/// of that path.
pub(crate) fn proposal_premise_error(
    kind: NewTestKind,
    file: &str,
    exists: bool,
) -> Option<String> {
    match (kind, exists) {
        (NewTestKind::Integration, true) => Some(format!(
            "the new-test proposal is stale: it names `{file}` as a new integration test file, but that path already exists; re-run the before phase to get a current proposal"
        )),
        (NewTestKind::InlineUnit, false) => Some(format!(
            "the new-test proposal is stale: it names the existing file `{file}` for an inline test module, but that file does not exist; re-run the before phase to get a current proposal"
        )),
        _ => None,
    }
}

/// What a read-only look at a proposed target finds. There is no test yet, so
/// the only non-refusal state is `NoTestYet`: never an empty or invented test
/// result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProposedTargetPreflight {
    NoTestYet { package: String },
    Refused(String),
}

/// Whether a manifest declares a `[package]` (a virtual workspace root does
/// not own tests). A manifest that exists but cannot be read or parsed is an
/// error, never "not a package", so the walk cannot slip past it to an
/// ancestor.
fn declares_package(manifest: &Path) -> Result<bool, String> {
    if !manifest.is_file() {
        return Ok(false);
    }
    let text = std::fs::read_to_string(manifest).map_err(|error| error.to_string())?;
    let table = text
        .parse::<toml::Table>()
        .map_err(|error| error.to_string())?;
    Ok(table.contains_key("package"))
}

/// Filesystem-free part of the preflight: the path is non-empty, relative, and
/// every component is a plain name (no `..`, root, prefix or `.`).
pub(crate) fn is_plain_relative_path(file: &str) -> bool {
    !file.is_empty()
        && Path::new(file)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

/// Filesystem-free part of the preflight: an integration proposal is exactly
/// `<dir>/tests/<name>.rs` (the `tests` directory is the file's parent).
pub(crate) fn is_integration_test_file_shape(file: &str) -> bool {
    let relative = Path::new(file);
    let parts: Vec<_> = relative.components().collect();
    parts.len() >= 2
        && parts[parts.len() - 2].as_os_str() == "tests"
        && file.ends_with(".rs")
        && relative.file_name().is_some_and(|name| name != ".rs")
}

/// Read-only preflight of a proposed new-test target (#13205): the path stays
/// inside the repository, no parent is a symlink, the stale premise holds, and
/// an owning Cargo package exists. Nothing is executed or written.
pub(crate) fn preflight_new_test_target(
    root: &Path,
    kind: NewTestKind,
    file: &str,
) -> ProposedTargetPreflight {
    let refuse = |reason: String| ProposedTargetPreflight::Refused(reason);
    let relative = Path::new(file);
    if !is_plain_relative_path(file) {
        return refuse(format!(
            "the proposed test path `{file}` is not a plain path inside the repository; re-run the before phase to get a current proposal"
        ));
    }
    // No directory on the way may be a symlink: it could point outside. A
    // directory that does not exist yet is fine; any other error refuses.
    let mut walked = root.to_path_buf();
    let parts: Vec<_> = relative.components().collect();
    for part in &parts[..parts.len().saturating_sub(1)] {
        walked.push(part);
        match std::fs::symlink_metadata(&walked) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return refuse(format!(
                    "the proposed test path `{file}` passes through a symlinked directory; re-run the before phase after replacing the link with a real directory"
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return refuse(format!(
                    "the proposed test path `{file}` could not be inspected ({error}); check the directory permissions, then re-run the before phase"
                ));
            }
        }
    }
    let target = root.join(relative);
    let exists = match std::fs::symlink_metadata(&target) {
        Ok(meta) => {
            if kind == NewTestKind::InlineUnit && !meta.file_type().is_file() {
                return refuse(format!(
                    "the production file `{file}` for the proposed inline test module is not a regular file; re-run the before phase to get a current proposal"
                ));
            }
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return refuse(format!(
                "the proposed test path `{file}` could not be inspected ({error}); check the directory permissions, then re-run the before phase"
            ));
        }
    };
    if let Some(reason) = proposal_premise_error(kind, file, exists) {
        return refuse(reason);
    }
    if kind == NewTestKind::Integration && !is_integration_test_file_shape(file) {
        return refuse(format!(
            "the proposed integration test `{file}` is not a `tests/<name>.rs` file; re-run the before phase to get a current proposal"
        ));
    }
    // The owning package: the nearest Cargo.toml above the file (above its
    // `tests` directory for an integration test).
    let mut dir = relative.parent().map(Path::to_path_buf).unwrap_or_default();
    if kind == NewTestKind::Integration {
        dir = dir.parent().map(Path::to_path_buf).unwrap_or_default();
    }
    loop {
        let manifest = root.join(&dir).join("Cargo.toml");
        let owned = match declares_package(&manifest) {
            Ok(owned) => owned,
            Err(error) => {
                let shown = if dir.as_os_str().is_empty() {
                    "Cargo.toml".to_string()
                } else {
                    format!("{}/Cargo.toml", dir.to_string_lossy().replace('\\', "/"))
                };
                return refuse(format!(
                    "the manifest `{shown}` that would own the proposed test `{file}` cannot be read or parsed ({error}); fix {shown}, then re-run the before phase"
                ));
            }
        };
        if owned {
            let package = if dir.as_os_str().is_empty() {
                ".".to_string()
            } else {
                dir.to_string_lossy().replace('\\', "/")
            };
            return ProposedTargetPreflight::NoTestYet { package };
        }
        if !dir.pop() {
            return refuse(format!(
                "no Cargo package owns the proposed test path `{file}`; run the repair from a Cargo workspace root or re-run the before phase"
            ));
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NewTestKind {
    InlineUnit,
    Integration,
}

impl NewTestKind {
    /// The packet's `recommended_test.target_kind` spelling for a proposal.
    /// One owner for the emitter and the attempt's reader, so the attempt can
    /// only follow a proposal the packet actually named.
    pub(crate) fn packet_target_kind(self) -> &'static str {
        match self {
            Self::InlineUnit => "new_inline_test_module",
            Self::Integration => "new_integration_test",
        }
    }

    pub(crate) fn from_packet_target_kind(label: &str) -> Option<Self> {
        [Self::InlineUnit, Self::Integration]
            .into_iter()
            .find(|kind| kind.packet_target_kind() == label)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NewTestProposalProvenance {
    ProducerOwned,
}

/// Prefer a producer-owned Integration file when the owner is a crate-root
/// public library item; otherwise keep the landed InlineUnit producer.
/// When both stay Missing, keep Integration's Cargo/layout blockers. A
/// PrivateOwner Integration refusal does not replace an InlineUnit module
/// reason: private items can still earn a same-file unit proposal.
///
/// `owner_fn` is the seam's owner as [`rust_index::find_owner_function`]
/// resolves it; the evidence pass passes its memoized answer.
pub(crate) fn admit_new_test_target(
    seam: &RepoSeam,
    index: &RustIndex,
    owner_fn: Option<&FunctionSummary>,
    layouts: &InlineUnitLayoutMemo,
) -> NewTestTargetAdmission {
    let integration = integration::admit_new_integration_test_for_owner(seam, index, owner_fn);
    let inline = admit_new_inline_unit_test_with(seam, index, owner_fn, layouts);
    let owner_inline_region = inline.region.clone();
    let mut selected = if integration.proposal.is_some() {
        integration
    } else if inline.proposal.is_some() {
        inline
    } else {
        match integration.blocker {
            Some(
                NewTestProposalBlocker::AutotestsDisabled
                | NewTestProposalBlocker::MissingIntegrationLayout
                | NewTestProposalBlocker::LibraryTargetUnresolved
                | NewTestProposalBlocker::FileCollision,
            ) => integration,
            _ => inline,
        }
    };
    selected.owner_inline_region = owner_inline_region;
    selected
}

/// Admit one InlineUnit proposal from indexed source-role and parser-backed
/// module facts. Callers that already have a related observer still invoke
/// this so Missing reasons stay typed; ranking prefers an admitted Existing
/// target, a refused DirectOwnerCall stays Missing rather than Proposed, and
/// an advisory related observer does not block an independently admitted
/// proposal.
#[cfg(test)]
pub(crate) fn admit_new_inline_unit_test(
    seam: &RepoSeam,
    index: &RustIndex,
) -> NewTestTargetAdmission {
    let owner_fn = rust_index::find_owner_function(index, seam.file(), seam.display_line());
    admit_new_inline_unit_test_with(seam, index, owner_fn, &InlineUnitLayoutMemo::default())
}

fn admit_new_inline_unit_test_with(
    seam: &RepoSeam,
    index: &RustIndex,
    owner_fn: Option<&FunctionSummary>,
    layouts: &InlineUnitLayoutMemo,
) -> NewTestTargetAdmission {
    match try_admit_new_inline_unit_test(seam, index, owner_fn, layouts) {
        Ok((proposal, region)) => NewTestTargetAdmission {
            proposal: Some(proposal),
            region: Some(region),
            blocker: None,
            owner_inline_region: None,
        },
        Err(blocker) => NewTestTargetAdmission {
            proposal: None,
            region: None,
            blocker: Some(blocker),
            owner_inline_region: None,
        },
    }
}

fn try_admit_new_inline_unit_test(
    seam: &RepoSeam,
    index: &RustIndex,
    owner_fn: Option<&FunctionSummary>,
    layouts: &InlineUnitLayoutMemo,
) -> Result<(NewTestTargetProposal, InlineTestRegionAuthority), NewTestProposalBlocker> {
    if !matches!(
        seam.kind(),
        SeamKind::PredicateBoundary
            | SeamKind::ErrorVariant
            | SeamKind::ReturnValue
            | SeamKind::FieldConstruction
            | SeamKind::MatchArm
    ) {
        return Err(NewTestProposalBlocker::InlineUnitOutOfScope);
    }
    if rust_index::is_test_file(seam.file()) {
        return Err(NewTestProposalBlocker::InlineUnitOutOfScope);
    }
    if path_is_generated_or_vendor(seam.file()) {
        return Err(NewTestProposalBlocker::GeneratedOrVendor);
    }
    if !is_relative_without_parent(seam.file()) {
        return Err(NewTestProposalBlocker::PathUnsafe);
    }

    let owner_fn = owner_fn.ok_or(NewTestProposalBlocker::OwnerUnresolved)?;
    if owner_fn.source_role != FunctionSourceRole::Production {
        return Err(NewTestProposalBlocker::OwnerUnresolved);
    }

    let facts = rust_index::find_file_facts(index, seam.file())
        .ok_or(NewTestProposalBlocker::OwnerUnresolved)?;
    if facts.used_lexical_fallback {
        return Err(NewTestProposalBlocker::LexicalFallback);
    }
    if facts
        .functions
        .iter()
        .any(|function| function.source_role == FunctionSourceRole::HarnessHelper)
    {
        return Err(NewTestProposalBlocker::CustomHarness);
    }

    let layout = layouts.layout(seam.file(), &facts.source);
    let modules = layout
        .modules
        .as_deref()
        .ok_or(NewTestProposalBlocker::LexicalFallback)?;
    let region = unique_inline_region(seam.file(), &facts.source, owner_fn, &layout, modules)?;
    // #13203 A0: the proposal's identity comes from the workspace authority;
    // without it nothing can name the workspace, so admit no proposal.
    let workspace_identity = index
        .workspace_authority
        .as_ref()
        .map(|authority| authority.workspace_identity.clone())
        .ok_or(NewTestProposalBlocker::PathUnsafe)?;

    Ok((
        NewTestTargetProposal {
            kind: NewTestKind::InlineUnit,
            file: normalize_relative(seam.file()),
            owner: seam.owner().to_string(),
            provenance: NewTestProposalProvenance::ProducerOwned,
            workspace_identity,
        },
        region,
    ))
}

fn unique_inline_region(
    file: &Path,
    source: &str,
    owner_fn: &FunctionSummary,
    layout: &InlineUnitFileLayout,
    modules: &[GovernedCfgTestModule],
) -> Result<InlineTestRegionAuthority, NewTestProposalBlocker> {
    let owner_modules = layout
        .owner_module_paths
        .get(&owner_fn.start_line)
        .ok_or(NewTestProposalBlocker::OwnerUnresolved)?;
    let inline = modules
        .iter()
        .filter(|module| module.is_inline)
        .collect::<Vec<_>>();
    let out_of_line = modules
        .iter()
        .filter(|module| !module.is_inline)
        .collect::<Vec<_>>();

    if inline.is_empty() {
        if !out_of_line.is_empty() {
            return Err(NewTestProposalBlocker::OutOfLineModule);
        }
        return Err(NewTestProposalBlocker::NoTestModule);
    }
    if inline.len() != 1 || !out_of_line.is_empty() {
        return Err(NewTestProposalBlocker::AmbiguousModule);
    }
    let module = inline[0];
    if &module.parent_modules != owner_modules {
        return Err(NewTestProposalBlocker::OwnerInaccessible);
    }
    let body_start = module
        .body_start
        .ok_or(NewTestProposalBlocker::AmbiguousModule)?;
    let close_brace_start = module
        .close_brace_start
        .ok_or(NewTestProposalBlocker::AmbiguousModule)?;
    if body_start > close_brace_start || close_brace_start > source.len() {
        return Err(NewTestProposalBlocker::AmbiguousModule);
    }
    Ok(InlineTestRegionAuthority {
        file: normalize_relative(file),
        module_name: module.name.clone(),
        parent_modules: module.parent_modules.clone(),
        body_start,
        close_brace_start,
        source_digest: layout.source_digest.clone(),
    })
}

/// Parser-backed facts an InlineUnit admission reads from one file. Each
/// depends only on the file's source, so one parse and one digest per file
/// serve every seam in it; per seam, admission used to parse the whole file
/// twice and hash it once, which dominated cold `ripr pilot` time.
#[derive(Debug)]
struct InlineUnitFileLayout {
    /// `None` when the file is not parser-valid.
    modules: Option<Vec<GovernedCfgTestModule>>,
    /// Enclosing non-cfg-test modules of each function, by start line.
    owner_module_paths: BTreeMap<usize, Vec<String>>,
    source_digest: String,
}

/// Per-file [`InlineUnitFileLayout`] memo for one evidence pass. The pass
/// borrows the index immutably, so a file's source cannot change while the
/// memo lives.
#[derive(Debug, Default)]
pub(crate) struct InlineUnitLayoutMemo {
    layouts: Mutex<BTreeMap<PathBuf, Arc<InlineUnitFileLayout>>>,
}

impl InlineUnitLayoutMemo {
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<PathBuf, Arc<InlineUnitFileLayout>>> {
        self.layouts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn layout(&self, file: &Path, source: &str) -> Arc<InlineUnitFileLayout> {
        if let Some(layout) = self.lock().get(file) {
            return Arc::clone(layout);
        }
        let (modules, owner_module_paths) = match inline_unit_module_layout(source) {
            Some(layout) => (Some(layout.modules), layout.owner_module_paths),
            None => (None, BTreeMap::new()),
        };
        let layout = Arc::new(InlineUnitFileLayout {
            modules,
            owner_module_paths,
            source_digest: region::source_digest(source),
        });
        self.lock().insert(file.to_path_buf(), Arc::clone(&layout));
        layout
    }
}

fn path_is_generated_or_vendor(path: &Path) -> bool {
    if is_generated_rust_file_with_patterns(path, &[]) {
        return true;
    }
    path.components().any(|component| {
        let Component::Normal(value) = component else {
            return false;
        };
        matches!(
            value.to_string_lossy().as_ref(),
            "vendor" | "vendored" | "node_modules" | "target" | ".git" | "fixtures"
        )
    })
}

pub(super) fn is_relative_without_parent(path: &Path) -> bool {
    !path.is_absolute()
        && path.components().all(|component| {
            !matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

pub(super) fn normalize_relative(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().replace('\\', "/"))
}
