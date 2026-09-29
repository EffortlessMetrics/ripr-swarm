use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::super::hex_lower;

pub(super) const PAYLOAD_SCHEMA_VERSION: u32 = 1;
pub(super) const RUNTIME_EVIDENCE_STATE: &str = "unqualified";
pub(super) const RUNTIME_EVIDENCE_SOURCE: &str = "issue:#4489";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FinalNativePayloadIdentity {
    pub(super) schema_version: u32,
    pub(super) product: String,
    pub(super) native_version: String,
    pub(super) target: String,
    pub(super) executable: String,
    pub(super) candidate_sha: String,
    pub(super) candidate_tree: String,
    pub(super) cargo_lock_sha256: String,
    pub(super) cargo_default_features: bool,
    pub(super) cargo_features: Vec<String>,
    pub(super) toolchain: PayloadToolchainIdentity,
    pub(super) build_environment: PayloadBuildEnvironment,
    pub(super) payload_files: Vec<PayloadFileIdentity>,
    pub(super) payload_sha256: String,
    pub(super) native_runtime_evidence: NativeRuntimeEvidence,
}

impl FinalNativePayloadIdentity {
    pub(crate) fn payload_sha256(&self) -> &str {
        &self.payload_sha256
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PayloadToolchainIdentity {
    pub(super) rustc_verbose_version: String,
    pub(super) cargo_verbose_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PayloadBuildEnvironment {
    pub(super) provider: String,
    pub(super) runner_os: Option<String>,
    pub(super) runner_arch: Option<String>,
    pub(super) image_os: Option<String>,
    pub(super) image_version: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PayloadFileIdentity {
    pub(super) relative_path: String,
    pub(super) role: String,
    pub(super) size: u64,
    pub(super) sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NativeRuntimeEvidence {
    pub(super) state: String,
    pub(super) source: String,
}

pub(super) fn payload_aggregate_sha256(files: &[PayloadFileIdentity]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"ripr-final-native-payload-v1");
    hasher.update([0]);
    for file in files {
        hasher.update(file.relative_path.as_bytes());
        hasher.update([0]);
        hasher.update(file.role.as_bytes());
        hasher.update([0]);
        hasher.update(file.size.to_le_bytes());
        hasher.update([0]);
        hasher.update(file.sha256.as_bytes());
        hasher.update([0]);
    }
    hex_lower(&hasher.finalize())
}

pub(super) fn payload_identity_json(
    identity: &FinalNativePayloadIdentity,
) -> Result<String, String> {
    serde_json::to_string_pretty(identity)
        .map(|text| format!("{text}\n"))
        .map_err(|err| format!("failed to render final native payload identity: {err}"))
}

pub(super) fn payload_identity_markdown(identity: &FinalNativePayloadIdentity) -> String {
    let mut text = String::new();
    text.push_str("# Final native payload identity\n\n");
    text.push_str(&format!("- schema version: `{}`\n", identity.schema_version));
    text.push_str(&format!("- product: `{}`\n", identity.product));
    text.push_str(&format!(
        "- native version: `{}`\n",
        identity.native_version
    ));
    text.push_str(&format!("- target: `{}`\n", identity.target));
    text.push_str(&format!("- executable: `{}`\n", identity.executable));
    text.push_str(&format!(
        "- candidate commit: `{}`\n",
        identity.candidate_sha
    ));
    text.push_str(&format!(
        "- candidate tree: `{}`\n",
        identity.candidate_tree
    ));
    text.push_str(&format!(
        "- Cargo.lock SHA-256: `{}`\n",
        identity.cargo_lock_sha256
    ));
    text.push_str(&format!(
        "- final payload SHA-256: `{}`\n",
        identity.payload_sha256
    ));
    text.push_str(&format!(
        "- native runtime evidence: `{}` (`{}`)\n",
        identity.native_runtime_evidence.state, identity.native_runtime_evidence.source
    ));
    text.push_str("\n## Cargo feature identity\n\n");
    text.push_str(&format!(
        "- default features: `{}`\n",
        identity.cargo_default_features
    ));
    for feature in &identity.cargo_features {
        text.push_str(&format!("- `{feature}`\n"));
    }
    text.push_str("\n## Toolchain\n\n```text\n");
    text.push_str(&identity.toolchain.rustc_verbose_version);
    text.push_str("\n\n");
    text.push_str(&identity.toolchain.cargo_verbose_version);
    text.push_str("\n```\n\n## Build environment\n\n");
    text.push_str(&format!(
        "- provider: `{}`\n",
        identity.build_environment.provider
    ));
    append_optional_markdown(&mut text, "runner OS", &identity.build_environment.runner_os);
    append_optional_markdown(
        &mut text,
        "runner architecture",
        &identity.build_environment.runner_arch,
    );
    append_optional_markdown(&mut text, "image OS", &identity.build_environment.image_os);
    append_optional_markdown(
        &mut text,
        "image version",
        &identity.build_environment.image_version,
    );
    text.push_str("\n## Payload files\n\n");
    text.push_str("| Relative path | Role | Size | SHA-256 |\n");
    text.push_str("| --- | --- | ---: | --- |\n");
    for file in &identity.payload_files {
        text.push_str(&format!(
            "| `{}` | `{}` | {} | `{}` |\n",
            file.relative_path, file.role, file.size, file.sha256
        ));
    }
    text
}

fn append_optional_markdown(text: &mut String, label: &str, value: &Option<String>) {
    let rendered = value.as_deref().unwrap_or("not-recorded");
    text.push_str(&format!("- {label}: `{rendered}`\n"));
}
