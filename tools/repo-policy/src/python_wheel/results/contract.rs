//! Required semantic steps from python-wheel-qualification.yml, not a runner.
use super::ArtifactKind;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Cell {
    Applicability,
    SourceContract,
    Build,
    Pip,
    Uv,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    DownloadDeadlineVerifier,
    EnterDeadline,
    VerifyCallableSubject,
    VerifyNativeSubject,
    WorkflowPolicy,
    BindSource,
    SourceContract,
    StageOrdinaryRunner,
    UploadOrdinaryRunner,
    BindNativeVersion,
    InstallTooling,
    ResolveDistributionVersion,
    BuildWheel,
    ValidateWheelAndNegativeControls,
    PackagePythonFixture,
    UploadWheel,
    Install,
    PythonJourney,
    DownloadOrdinaryRunner,
    ProvisionOrdinaryRust,
    OrdinaryJourney,
    UploadOrdinaryEvidence,
    ReinstallAndUninstall,
    CompleteDeadline,
}

impl Step {
    /// Exact YAML step name provides a drift check, not execution evidence.
    pub const fn name(self) -> &'static str {
        match self {
            Self::DownloadDeadlineVerifier => "Download native-run deadline verifier",
            Self::EnterDeadline => "Enter within native qualification deadline",
            Self::VerifyCallableSubject => {
                "Verify callable exact subject before policy or product execution"
            }
            Self::VerifyNativeSubject => "Verify actual native merge-group subject",
            Self::WorkflowPolicy => "Validate workflow policy independently",
            Self::BindSource => "Bind exact package source candidate",
            Self::SourceContract => "Validate package source contract",
            Self::StageOrdinaryRunner => "Stage the exact-source ordinary integration runner",
            Self::UploadOrdinaryRunner => "Retain ordinary runner source and build evidence",
            Self::BindNativeVersion => "Bind exact source candidate and native version",
            Self::InstallTooling => "Install pinned package tooling",
            Self::ResolveDistributionVersion => {
                "Resolve and cross-check Python distribution version"
            }
            Self::BuildWheel => "Build wheel without a compatibility-floor claim",
            Self::ValidateWheelAndNegativeControls => {
                "Validate metadata, RECORD, payload, and negative controls"
            }
            Self::PackagePythonFixture => "Package independent Python consumer fixture",
            Self::UploadWheel => "Upload no-publish wheel evidence",
            Self::Install => "Install from the local wheelhouse without Rust or source checkout",
            Self::PythonJourney => {
                "Run no-config Python preview, fresh-shell follow-up, and disable control"
            }
            Self::DownloadOrdinaryRunner => {
                "Download the ordinary test runner for the installed pip consumer"
            }
            Self::ProvisionOrdinaryRust => {
                "Provision Rust only for the ordinary fixture native test"
            }
            Self::OrdinaryJourney => "Run the strict installed ordinary journey",
            Self::UploadOrdinaryEvidence => {
                "Retain installed ordinary identities, logs, and native outcomes"
            }
            Self::ReinstallAndUninstall => "Reinstall and uninstall cleanly",
            Self::CompleteDeadline => "Complete within native qualification deadline",
        }
    }
}

/// Wheel semantic obligations, including both producers and each client cell.
/// Setup actions and artifact bytes still need authenticated observation; this
/// source contract is not a complete provider job/step or archive manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Definition {
    pub cell: Cell,
    pub id: &'static str,
    pub job_key: &'static str,
    pub matrix_key: &'static str,
    pub steps: &'static [Step],
    pub consumes: &'static [ArtifactKind],
    pub produces: &'static [ArtifactKind],
}

impl Cell {
    pub const fn definition(self) -> Definition {
        use ArtifactKind::{InstalledOrdinaryEvidence, OrdinaryRunner, Wheelhouse};
        use Step::*;
        let (id, job_key, matrix_key, steps, consumes, produces): (
            _,
            _,
            _,
            &[Step],
            &[ArtifactKind],
            &[ArtifactKind],
        ) = match self {
            Self::Applicability => (
                "wheel.applicability",
                "applicability",
                "",
                &[
                    DownloadDeadlineVerifier,
                    EnterDeadline,
                    VerifyCallableSubject,
                    VerifyNativeSubject,
                    WorkflowPolicy,
                    CompleteDeadline,
                ],
                &[],
                &[],
            ),
            Self::SourceContract => (
                "wheel.source_contract",
                "source-contract",
                "",
                &[
                    DownloadDeadlineVerifier,
                    EnterDeadline,
                    BindSource,
                    SourceContract,
                    StageOrdinaryRunner,
                    UploadOrdinaryRunner,
                    CompleteDeadline,
                ],
                &[],
                &[OrdinaryRunner],
            ),
            Self::Build => (
                "wheel.build",
                "build-wheel",
                "",
                &[
                    DownloadDeadlineVerifier,
                    EnterDeadline,
                    BindNativeVersion,
                    InstallTooling,
                    ResolveDistributionVersion,
                    BuildWheel,
                    ValidateWheelAndNegativeControls,
                    PackagePythonFixture,
                    UploadWheel,
                    CompleteDeadline,
                ],
                &[],
                &[Wheelhouse],
            ),
            Self::Pip => (
                "wheel.consume.pip",
                "consume",
                "client=pip",
                &[
                    DownloadDeadlineVerifier,
                    EnterDeadline,
                    Install,
                    PythonJourney,
                    DownloadOrdinaryRunner,
                    ProvisionOrdinaryRust,
                    OrdinaryJourney,
                    UploadOrdinaryEvidence,
                    ReinstallAndUninstall,
                    CompleteDeadline,
                ],
                &[Wheelhouse, OrdinaryRunner],
                &[InstalledOrdinaryEvidence],
            ),
            Self::Uv => (
                "wheel.consume.uv",
                "consume",
                "client=uv",
                &[
                    DownloadDeadlineVerifier,
                    EnterDeadline,
                    Install,
                    PythonJourney,
                    ReinstallAndUninstall,
                    CompleteDeadline,
                ],
                &[Wheelhouse],
                &[],
            ),
        };
        Definition {
            cell: self,
            id,
            job_key,
            matrix_key,
            steps,
            consumes,
            produces,
        }
    }
}

pub fn definitions() -> [Definition; 5] {
    [
        Cell::Applicability,
        Cell::SourceContract,
        Cell::Build,
        Cell::Pip,
        Cell::Uv,
    ]
    .map(Cell::definition)
}
