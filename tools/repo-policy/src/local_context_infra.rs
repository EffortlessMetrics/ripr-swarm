//! One pinned infrastructure contract, not an extension of the generic path allowlist.
//!
//! The em-ci host fixes the owned JIT runner's disposable workspace root:
//! https://github.com/EffortlessMetrics/em-ci/blob/df90ed3a55c4ab3d7eccaa68200c8ffb514efa65/host/em-ci-slot#L1487-L1524
//! The deployed image and capacity are separate activation gates. Changes to the
//! owned JIT step require an intentional review and corresponding digest update.

use sha2::{Digest, Sha256};

pub const CONTRACT_PATH: &str = ".github/workflows/rust-gates.yml";
pub const CONTRACT_TOKEN: &str = concat!("/home", "/runner", "/_work/*");
const STEP_START: &str = "      - name: Configure owned JIT environment\n";
const STEP_END: &str = "\n      # #3841:";
// Digest of the entire step body, including its execution condition and shell guard.
const STEP_SHA256: &str = "538326c00a408d5ccb514ace9062ef1a0f9adc4f54bc0ba8aa2ebe9d422a051a";

/// Accept only the unique raw token in the exact workflow and unchanged step.
/// Raw counting matters because the general scanner deduplicates same-line findings.
pub fn matches_contract(path: &str, text: &str) -> bool {
    if path != CONTRACT_PATH
        || text.matches(CONTRACT_TOKEN).count() != 1
        || text.matches(STEP_START).count() != 1
        || text.matches(STEP_END).count() != 1
    {
        return false;
    }
    let Some((_, rest)) = text.split_once(STEP_START) else {
        return false;
    };
    let Some((step, _)) = rest.split_once(STEP_END) else {
        return false;
    };
    if step.matches(CONTRACT_TOKEN).count() != 1 {
        return false;
    }
    format!("{:x}", Sha256::digest(step.as_bytes())) == STEP_SHA256
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = include_str!("../../../.github/workflows/rust-gates.yml");

    #[test]
    fn exact_contract_only() {
        assert!(matches_contract(CONTRACT_PATH, SOURCE));
        assert!(!matches_contract("other.yml", SOURCE));
        for replacement in [
            concat!("/home", "/someone", "/_work/*"),
            concat!("/home", "/runner", "/_work-other/*"),
            concat!("/home", "/runner", "/*"),
            concat!("/home", "/runner", "/_work/child/*"),
            "removed",
        ] {
            assert!(!matches_contract(
                CONTRACT_PATH,
                &SOURCE.replace(CONTRACT_TOKEN, replacement)
            ));
        }
    }

    #[test]
    fn raw_duplicates_moved_token_and_step_changes_fail_closed() {
        for changed in [
            SOURCE.replacen(
                CONTRACT_TOKEN,
                &format!("{CONTRACT_TOKEN} {CONTRACT_TOKEN}"),
                1,
            ),
            format!("{SOURCE}\n# {CONTRACT_TOKEN}\n"),
            SOURCE.replace(CONTRACT_TOKEN, "removed") + &format!("\n# {CONTRACT_TOKEN}\n"),
            SOURCE.replace("if: inputs.execution-profile == 'owned-jit'", "if: false"),
            SOURCE.replace("$(realpath \"$GITHUB_WORKSPACE\")", "$GITHUB_WORKSPACE"),
            SOURCE.replace(STEP_START, "      - name: another step\n"),
            format!("{SOURCE}\n{STEP_START}"),
        ] {
            assert!(!matches_contract(CONTRACT_PATH, &changed));
        }
    }
}
