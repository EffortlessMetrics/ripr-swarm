//! Application-layer command-spec digest computation for
//! [`VerificationExecutionResultV1`] provenance binding.
//!
//! The domain result type stays JSON-free (policy/architecture.txt); hashing
//! the canonical serialized [`CommandSpec`] lives here, in the layer that may
//! know JSON rendering. Consumers compute the expected digest with
//! [`command_spec_sha256`] and pass it to
//! [`VerificationExecutionResultV1::validate_against`].

use crate::domain::{CommandSpec, VerificationExecutionResultValidationError};
use sha2::{Digest, Sha256};

/// Hash the canonical serialized command specification used by the result
/// binding. The human display string is included only as ordinary typed data;
/// consumers never reconstruct argv from it.
pub(crate) fn command_spec_sha256(
    command_spec: &CommandSpec,
) -> Result<String, VerificationExecutionResultValidationError> {
    let bytes = serde_json::to_vec(command_spec).map_err(|error| {
        VerificationExecutionResultValidationError::CommandSpecInvalid(error.to_string())
    })?;
    let digest = Sha256::digest(&bytes);
    Ok(format!("sha256:{digest:x}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        CancellationPolicy, CommandAuthorityBoundary, CommandCostClass, CommandExecutionMode,
        CommandPlatform, CommandRole, EnvironmentPolicy, ExpectedResultParser, NetworkPolicy,
        StdinPolicy, VerificationCurrentnessV1, VerificationExecutionResultV1,
        VerificationProcessDispositionV1,
    };

    const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const ROOT: &str = "root:fixture";
    const ZERO_DIGEST: &str =
        "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    const SPEC_DIGEST: &str =
        "sha256:7594ebd8d5ea61336f33c236c213c87467b752befec1b582d7cc99ce42f8a5ab";

    fn command_spec() -> CommandSpec {
        CommandSpec {
            schema_version: CommandSpec::SCHEMA_VERSION.to_string(),
            command_id: "ripr:test:verification".to_string(),
            role: CommandRole::Verify,
            execution_mode: CommandExecutionMode::Direct,
            program: "cargo".to_string(),
            args: vec!["test".to_string(), "boundary".to_string()],
            cwd: ".".to_string(),
            env_set: Vec::new(),
            env_passthrough: Vec::new(),
            environment_policy: EnvironmentPolicy::Clean,
            stdin: StdinPolicy::Null,
            timeout_ms: 60_000,
            cancellation: CancellationPolicy::Allowed,
            network_policy: NetworkPolicy::Forbidden,
            expected_result_parser: ExpectedResultParser::ExitCode,
            expected_exit_codes: vec![0],
            expected_writes: Vec::new(),
            cost_class: CommandCostClass::CompileOrTest,
            platforms: vec![CommandPlatform::Windows],
            display: "cargo test boundary".to_string(),
            authority_boundary: CommandAuthorityBoundary::VerificationRouteOnly,
        }
    }

    fn result(spec_digest: &str) -> VerificationExecutionResultV1 {
        VerificationExecutionResultV1 {
            schema_version: VerificationExecutionResultV1::SCHEMA_VERSION.to_string(),
            root_identity: ROOT.to_string(),
            head_before: HEAD.to_string(),
            head_after: HEAD.to_string(),
            command_spec_sha256: spec_digest.to_string(),
            process_disposition: VerificationProcessDispositionV1::Completed,
            exit_status: Some(0),
            stdout_sha256: ZERO_DIGEST.to_string(),
            stderr_sha256: ZERO_DIGEST.to_string(),
            currentness: VerificationCurrentnessV1::Current,
        }
    }

    #[test]
    fn command_spec_digest_is_pinned_to_serialized_field_order() -> Result<(), String> {
        let digest = command_spec_sha256(&command_spec()).map_err(|error| error.to_string())?;
        if digest != SPEC_DIGEST {
            return Err(format!("unexpected command-spec digest: {digest}"));
        }
        Ok(())
    }

    #[test]
    fn valid_result_round_trips_and_validates_against_exact_context() -> Result<(), String> {
        let spec = command_spec();
        let digest = command_spec_sha256(&spec).map_err(|error| error.to_string())?;
        let encoded = serde_json::to_string(&result(&digest)).map_err(|error| error.to_string())?;
        let decoded: VerificationExecutionResultV1 =
            serde_json::from_str(&encoded).map_err(|error| error.to_string())?;
        decoded
            .validate_against(&spec, &digest, ROOT, HEAD, HEAD)
            .map_err(|error| error.to_string())
    }

    #[test]
    fn digest_from_a_different_spec_fails_the_binding() -> Result<(), String> {
        let spec = command_spec();
        let mut other_spec = command_spec();
        other_spec.command_id = "ripr:test:other".to_string();
        let spec_digest = command_spec_sha256(&spec).map_err(|error| error.to_string())?;
        let other_digest = command_spec_sha256(&other_spec).map_err(|error| error.to_string())?;
        if spec_digest == other_digest {
            return Err("distinct command specs produced the same digest".to_string());
        }
        // A result bound to `other_spec` must fail when the expected digest is
        // computed from `spec`.
        let outcome = result(&other_digest).validate_against(&spec, &spec_digest, ROOT, HEAD, HEAD);
        if !matches!(
            outcome,
            Err(VerificationExecutionResultValidationError::CommandSpecDigestMismatch { .. })
        ) {
            return Err(
                "result bound to a different command spec was accepted: {outcome:?}".to_string(),
            );
        }
        Ok(())
    }
}
