//! Projection of an observed offline-provider failure onto the existing limit.
//! No renderer parses stderr or invents a missing-package diagnosis.

use crate::analysis::workspace::MetadataProbeFailure;
use crate::domain::Finding;

const MANUAL_METADATA: &str =
    "cargo metadata --offline --locked --format-version 1 --manifest-path Cargo.toml";
const GENERIC_EDGE: &str = "limitation_first_unresolved_edge: property macro expansion, executable-test collection and owner reach";

pub(super) fn apply(finding: &mut Finding, failure: MetadataProbeFailure) {
    // Keep the existing source-invocation pointer and all non-claims. The
    // supported spelling is not proof of provider identity or target admission.
    finding.evidence.retain(|line| line != GENERIC_EDGE);
    finding.evidence.push(format!(
        "limitation_first_unresolved_edge: offline provider verification; {failure}"
    ));
    finding.evidence.push(
        "Provider identity and target eligibility remain unverified; this failure supplies no generated test, observed value or execution evidence."
            .to_string(),
    );
    finding.recommended_next_step = Some(recovery_step(failure));
}

fn recovery_step(failure: MetadataProbeFailure) -> String {
    match failure {
        MetadataProbeFailure::ToolchainPathPin => {
            // Never recommend executing a toolchain path the process owner
            // refused. A manually trusted, installed toolchain is prerequisite.
            "Inspect the repository's toolchain path and select a trusted installed toolchain before rerunning RIPR. No repository-local toolchain was executed or installed."
                .to_string()
        }
        MetadataProbeFailure::CaptureCreate(_)
        | MetadataProbeFailure::CaptureInspect(_)
        | MetadataProbeFailure::CaptureRead(_) => {
            "Check access and free space in the local temporary directory used for Cargo metadata capture, then rerun RIPR. A capture failure does not identify a missing dependency."
                .to_string()
        }
        _ => format!(
            "From the same analysis root and environment, run `{MANUAL_METADATA}` to inspect Cargo's own diagnostic. Check the installed Cargo/toolchain and locked local inputs, then rerun RIPR after addressing the reported problem. RIPR did not fetch dependencies, install a toolchain or retry online."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_command_keeps_offline_locked_scope_without_guessing_a_package() {
        for failure in [
            MetadataProbeFailure::Launch(std::io::ErrorKind::NotFound),
            MetadataProbeFailure::UnsuccessfulExit(Some(101)),
            MetadataProbeFailure::UnsuccessfulExit(None),
            MetadataProbeFailure::TimedOut,
            MetadataProbeFailure::OutputLimit,
            MetadataProbeFailure::InvalidJson,
            MetadataProbeFailure::Wait(std::io::ErrorKind::Other),
        ] {
            let action = recovery_step(failure);
            assert!(action.contains(MANUAL_METADATA), "{failure:?}: {action}");
            assert!(action.contains("same analysis root and environment"));
            assert!(!action.contains("zerocopy"));
            assert!(!action.contains("cargo fetch"));
            assert!(!action.contains("rustup install"));
        }
    }

    #[test]
    fn capture_failure_and_refused_toolchain_do_not_borrow_dependency_advice() {
        for failure in [
            MetadataProbeFailure::CaptureCreate(std::io::ErrorKind::PermissionDenied),
            MetadataProbeFailure::CaptureInspect(std::io::ErrorKind::NotFound),
            MetadataProbeFailure::CaptureRead(std::io::ErrorKind::NotFound),
        ] {
            let action = recovery_step(failure);
            assert!(action.contains("temporary directory"));
            assert!(!action.contains(MANUAL_METADATA));
        }
        let action = recovery_step(MetadataProbeFailure::ToolchainPathPin);
        assert!(action.contains("trusted installed toolchain"));
        assert!(!action.contains(MANUAL_METADATA));
    }
}
