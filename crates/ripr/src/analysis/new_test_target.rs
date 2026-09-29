//! Producer-owned Integration `NewTestTargetProposal` admission (#4576).
//!
//! When no existing test can own a test-only repair, this module may earn
//! one exact new integration-test file from RustIndex facts. It does not
//! invent expected values or generate the test body. Inline unit insertion
//! stays out of scope.

use crate::analysis::facts::{FunctionSourceRole, RustIndex};
use crate::analysis::rust_index::{self, FunctionSummary};
use crate::analysis::seams::{RepoSeam, SeamKind};
use crate::domain::{
    CancellationPolicy, CommandAuthorityBoundary, CommandCostClass, CommandExecutionMode,
    CommandPlatform, CommandRole, CommandSpec, EnvironmentPolicy, ExpectedResultParser,
    NetworkPolicy, StdinPolicy,
};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[cfg(test)]
mod tests;

const SAFE_NEW_INTEGRATION_TEST_EVIDENCE: &str = "producer-owned new integration test proposal";

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct NewTestTargetAdmission {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) proposal: Option<NewTestTargetProposal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) blocker: Option<NewTestProposalBlocker>,
}

impl NewTestTargetAdmission {
    pub(crate) fn missing_reason(&self) -> Option<String> {
        self.blocker.as_ref().map(|blocker| {
            format!(
                "new integration test proposal blocked: {}",
                blocker.as_str()
            )
        })
    }

    pub(crate) fn present_reason() -> &'static str {
        SAFE_NEW_INTEGRATION_TEST_EVIDENCE
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NewTestProposalBlocker {
    PrivateOwner,
    AutotestsDisabled,
    MissingIntegrationLayout,
    LibraryTargetUnresolved,
    OwnerUnresolved,
    FileCollision,
    PathUnsafe,
    InlineUnitOutOfScope,
}

impl NewTestProposalBlocker {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PrivateOwner => {
                "owner is private and would require a production visibility change"
            }
            Self::AutotestsDisabled => "cargo autotests discovery is disabled",
            Self::MissingIntegrationLayout => "package has no established tests/ layout",
            Self::LibraryTargetUnresolved => "exact public library target is unresolved",
            Self::OwnerUnresolved => "production owner is unresolved",
            Self::FileCollision => "proposed integration file collides with an existing path",
            Self::PathUnsafe => "proposed integration path is not a root-contained new tests/ file",
            Self::InlineUnitOutOfScope => {
                "inline unit insertion is out of scope for this integration leaf"
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct NewTestTargetProposal {
    pub(crate) kind: NewTestKind,
    pub(crate) file: PathBuf,
    pub(crate) owner: String,
    pub(crate) provenance: NewTestProposalProvenance,
    /// After-edit verification and cage surface. Not part of the reserved
    /// 4-field public proposal identity.
    #[serde(skip)]
    pub(crate) details: Option<NewTestTargetDetails>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct NewTestTargetDetails {
    pub(crate) package_name: String,
    pub(crate) package_identity: String,
    pub(crate) library_crate_name: String,
    pub(crate) import_path: String,
    pub(crate) visibility_basis: NewTestVisibilityBasis,
    pub(crate) discovery_basis: NewTestDiscoveryBasis,
    pub(crate) planned_verification: PlannedNewTestVerification,
    pub(crate) allowed_edit_surface: Vec<PathBuf>,
    pub(crate) must_not_change: Vec<PathBuf>,
    pub(crate) workspace_identity: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NewTestVisibilityBasis {
    PublicLibraryItem,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NewTestDiscoveryBasis {
    EstablishedAutotestsLayout,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct PlannedNewTestVerification {
    pub(crate) status: PlannedVerificationStatus,
    pub(crate) package_name: String,
    pub(crate) cargo_test_target: String,
    pub(crate) command: CommandSpec,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PlannedVerificationStatus {
    AfterEditUnexecuted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NewTestKind {
    InlineUnit,
    Integration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NewTestProposalProvenance {
    ProducerOwned,
}

/// Admit one Integration proposal from indexed package, visibility, and
/// Cargo discovery facts. Callers that already have a safe Existing target
/// must not invoke this.
pub(crate) fn admit_new_integration_test(
    seam: &RepoSeam,
    index: &RustIndex,
) -> NewTestTargetAdmission {
    match try_admit_new_integration_test(seam, index) {
        Ok(proposal) => NewTestTargetAdmission {
            proposal: Some(proposal),
            blocker: None,
        },
        Err(blocker) => NewTestTargetAdmission {
            proposal: None,
            blocker: Some(blocker),
        },
    }
}

fn try_admit_new_integration_test(
    seam: &RepoSeam,
    index: &RustIndex,
) -> Result<NewTestTargetProposal, NewTestProposalBlocker> {
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

    let owner_fn = rust_index::find_owner_function(index, seam.file(), seam.display_line())
        .ok_or(NewTestProposalBlocker::OwnerUnresolved)?;
    if owner_fn.source_role != FunctionSourceRole::Production {
        return Err(NewTestProposalBlocker::OwnerUnresolved);
    }

    let authority = index
        .workspace_authority
        .as_ref()
        .ok_or(NewTestProposalBlocker::PathUnsafe)?;
    let package = owning_package(&authority.root, seam.file())?;
    if !owner_is_public_library_item(index, owner_fn, &package) {
        return Err(NewTestProposalBlocker::PrivateOwner);
    }
    if !package.has_library_target {
        return Err(NewTestProposalBlocker::LibraryTargetUnresolved);
    }
    if !package.autotests {
        return Err(NewTestProposalBlocker::AutotestsDisabled);
    }
    if !package.has_established_tests_layout {
        return Err(NewTestProposalBlocker::MissingIntegrationLayout);
    }

    let proposed = proposed_integration_file(&authority.root, &package, owner_fn)?;
    if !is_root_contained_new_test_file(&authority.root, &proposed) {
        return Err(NewTestProposalBlocker::PathUnsafe);
    }

    let import_path = format!("{}::{}", package.library_crate_name, owner_fn.name);
    let command = after_edit_verify_command(&package.package_name, &proposed)?;
    let cargo_test_target = proposed
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .ok_or(NewTestProposalBlocker::PathUnsafe)?
        .to_string();

    Ok(NewTestTargetProposal {
        kind: NewTestKind::Integration,
        file: proposed.clone(),
        owner: import_path.clone(),
        provenance: NewTestProposalProvenance::ProducerOwned,
        details: Some(NewTestTargetDetails {
            package_name: package.package_name.clone(),
            package_identity: package.identity,
            library_crate_name: package.library_crate_name,
            import_path,
            visibility_basis: NewTestVisibilityBasis::PublicLibraryItem,
            discovery_basis: NewTestDiscoveryBasis::EstablishedAutotestsLayout,
            planned_verification: PlannedNewTestVerification {
                status: PlannedVerificationStatus::AfterEditUnexecuted,
                package_name: package.package_name.clone(),
                cargo_test_target,
                command,
            },
            allowed_edit_surface: vec![proposed],
            must_not_change: vec![
                seam.file().to_path_buf(),
                package.manifest_relative,
                PathBuf::from("Cargo.toml"),
            ],
            workspace_identity: authority.workspace_identity.clone(),
        }),
    })
}

struct PackageFacts {
    package_dir: PathBuf,
    package_name: String,
    library_crate_name: String,
    identity: String,
    manifest_relative: PathBuf,
    autotests: bool,
    has_library_target: bool,
    has_established_tests_layout: bool,
}

fn owning_package(
    root: &Path,
    relative_file: &Path,
) -> Result<PackageFacts, NewTestProposalBlocker> {
    let mut cursor = relative_file.parent().map(Path::to_path_buf);
    while let Some(directory) = cursor {
        let relative_manifest = directory.join("Cargo.toml");
        let manifest = root.join(&relative_manifest);
        if manifest.is_file() {
            let text = std::fs::read_to_string(&manifest)
                .map_err(|_| NewTestProposalBlocker::LibraryTargetUnresolved)?;
            let value = text
                .parse::<toml::Table>()
                .map_err(|_| NewTestProposalBlocker::LibraryTargetUnresolved)?;
            let Some(package) = value.get("package").and_then(toml::Value::as_table) else {
                cursor = directory.parent().map(Path::to_path_buf);
                continue;
            };
            let package_name = package
                .get("name")
                .and_then(toml::Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .ok_or(NewTestProposalBlocker::LibraryTargetUnresolved)?
                .to_string();
            let autotests = package
                .get("autotests")
                .and_then(toml::Value::as_bool)
                .unwrap_or(true);
            let lib_name = value
                .get("lib")
                .and_then(toml::Value::as_table)
                .and_then(|lib| lib.get("name"))
                .and_then(toml::Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| package_name.replace('-', "_"));
            let lib_path = value
                .get("lib")
                .and_then(toml::Value::as_table)
                .and_then(|lib| lib.get("path"))
                .and_then(toml::Value::as_str)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("src/lib.rs"));
            let has_library_target = root.join(&directory).join(&lib_path).is_file();
            let tests_dir = root.join(&directory).join("tests");
            let has_established_tests_layout = tests_dir.is_dir()
                && std::fs::read_dir(&tests_dir)
                    .map(|entries| {
                        entries.filter_map(Result::ok).any(|entry| {
                            entry.path().extension().and_then(|ext| ext.to_str()) == Some("rs")
                        })
                    })
                    .unwrap_or(false);
            let identity = format!(
                "{}:{}",
                relative_manifest.to_string_lossy().replace('\\', "/"),
                crate::analysis::facts::source_digest(text.as_bytes())
            );
            return Ok(PackageFacts {
                package_dir: directory,
                package_name,
                library_crate_name: lib_name.replace('-', "_"),
                identity,
                manifest_relative: relative_manifest,
                autotests,
                has_library_target,
                has_established_tests_layout,
            });
        }
        if directory.as_os_str().is_empty() {
            break;
        }
        cursor = directory.parent().map(Path::to_path_buf);
    }
    Err(NewTestProposalBlocker::LibraryTargetUnresolved)
}

fn owner_is_public_library_item(
    index: &RustIndex,
    owner_fn: &FunctionSummary,
    package: &PackageFacts,
) -> bool {
    let package_relative = owner_fn
        .file
        .strip_prefix(&package.package_dir)
        .unwrap_or(&owner_fn.file);
    if package_relative != Path::new("src/lib.rs") {
        return false;
    }
    let Some(facts) = index.files.get(&owner_fn.file) else {
        return false;
    };
    function_item_is_crate_public(&facts.source, owner_fn.start_line)
}

fn function_item_is_crate_public(source: &str, start_line: usize) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let mut index = start_line.saturating_sub(1);
    while index > 0 {
        let previous = lines
            .get(index.saturating_sub(1))
            .copied()
            .unwrap_or("")
            .trim();
        if previous.is_empty()
            || previous.starts_with("///")
            || previous.starts_with("//!")
            || previous.starts_with("//")
            || previous.starts_with("#[")
            || previous.starts_with("#!")
        {
            index -= 1;
            continue;
        }
        break;
    }
    let declaration = lines.get(index).copied().unwrap_or("").trim();
    crate_public_fn_declaration(declaration)
}

fn crate_public_fn_declaration(declaration: &str) -> bool {
    let Some(after_pub) = declaration.strip_prefix("pub") else {
        return false;
    };
    if after_pub.starts_with('(') {
        return false;
    }
    let rest = after_pub.trim_start();
    let rest = rest
        .strip_prefix("async")
        .map(str::trim_start)
        .unwrap_or(rest);
    let rest = rest
        .strip_prefix("const")
        .map(str::trim_start)
        .unwrap_or(rest);
    let rest = rest
        .strip_prefix("unsafe")
        .map(str::trim_start)
        .unwrap_or(rest);
    rest.starts_with("fn")
        && rest
            .as_bytes()
            .get(2)
            .is_none_or(|byte| byte.is_ascii_whitespace())
}

fn proposed_integration_file(
    root: &Path,
    package: &PackageFacts,
    owner_fn: &FunctionSummary,
) -> Result<PathBuf, NewTestProposalBlocker> {
    let stem = sanitize_test_file_stem(&owner_fn.name)?;
    let candidates = [
        format!("tests/{stem}.rs"),
        format!("tests/{stem}_boundary.rs"),
    ];
    for relative in candidates {
        let workspace_relative = if package.package_dir.as_os_str().is_empty() {
            PathBuf::from(&relative)
        } else {
            package.package_dir.join(&relative)
        };
        if !root.join(&workspace_relative).exists() {
            return Ok(normalize_relative(&workspace_relative));
        }
    }
    Err(NewTestProposalBlocker::FileCollision)
}

fn sanitize_test_file_stem(name: &str) -> Result<String, NewTestProposalBlocker> {
    let stem: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if stem.is_empty() {
        return Err(NewTestProposalBlocker::PathUnsafe);
    }
    Ok(stem)
}

fn is_root_contained_new_test_file(root: &Path, relative: &Path) -> bool {
    if !is_relative_without_parent(relative) {
        return false;
    }
    let normalized = relative.to_string_lossy().replace('\\', "/");
    if !rust_index::is_test_file(relative) {
        return false;
    }
    if !normalized.ends_with(".rs") {
        return false;
    }
    let forbidden = [
        "target/",
        ".git/",
        "vendor/",
        "node_modules/",
        "fixtures/",
        "generated/",
    ];
    if forbidden
        .iter()
        .any(|prefix| normalized.starts_with(prefix) || normalized.contains(&format!("/{prefix}")))
    {
        return false;
    }
    let full = root.join(relative);
    if full.exists() {
        return false;
    }
    let Some(parent) = full.parent() else {
        return false;
    };
    let Ok(canonical_root) = root.canonicalize() else {
        return false;
    };
    parent
        .canonicalize()
        .is_ok_and(|canonical| canonical.starts_with(&canonical_root))
}

fn is_relative_without_parent(path: &Path) -> bool {
    !path.is_absolute()
        && path.components().all(|component| {
            !matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

fn normalize_relative(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().replace('\\', "/"))
}

fn after_edit_verify_command(
    package_name: &str,
    proposed: &Path,
) -> Result<CommandSpec, NewTestProposalBlocker> {
    let target = proposed
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .ok_or(NewTestProposalBlocker::PathUnsafe)?;
    let args = vec![
        "test".to_string(),
        "-p".to_string(),
        package_name.to_string(),
        "--test".to_string(),
        target.to_string(),
    ];
    let display = format!("after edit (unexecuted): cargo test -p {package_name} --test {target}");
    let spec = CommandSpec {
        schema_version: CommandSpec::SCHEMA_VERSION.to_string(),
        command_id: format!("ripr:proposed-integration-test:after-edit:{package_name}:{target}"),
        role: CommandRole::Verify,
        execution_mode: CommandExecutionMode::Direct,
        program: "cargo".to_string(),
        args,
        cwd: ".".to_string(),
        env_set: Vec::new(),
        env_passthrough: Vec::new(),
        environment_policy: EnvironmentPolicy::Clean,
        stdin: StdinPolicy::Null,
        timeout_ms: 120_000,
        cancellation: CancellationPolicy::Allowed,
        network_policy: NetworkPolicy::Forbidden,
        expected_result_parser: ExpectedResultParser::ExitCode,
        expected_exit_codes: vec![0],
        expected_writes: Vec::new(),
        cost_class: CommandCostClass::CompileOrTest,
        platforms: vec![
            CommandPlatform::Linux,
            CommandPlatform::Macos,
            CommandPlatform::Windows,
        ],
        display,
        authority_boundary: CommandAuthorityBoundary::VerificationRouteOnly,
    };
    spec.validate()
        .map_err(|_| NewTestProposalBlocker::PathUnsafe)?;
    Ok(spec)
}
