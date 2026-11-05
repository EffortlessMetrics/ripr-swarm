//! Failure-only, bounded read-only evidence. Producer attribution stays unresolved.
//!
//! The capture machinery in the `linux` submodule is reachable only through the
//! Linux-only tail of [`command`]; the submodule carries the same
//! `cfg(target_os = "linux")` gate, so every other host compiles only the
//! explicit adapter refusal control instead of a dead copy of the adapter
//! (which broke `cargo clippy --workspace --all-targets` on Windows hosts,
//! #7624).

type Result<T> = std::result::Result<T, String>;

fn supported_profile_platform(platform: &str) -> bool {
    platform == "linux"
}

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
mod storage;

/// Run the fixed coverage command with bounded, metadata-only storage receipts.
pub fn run_coverage() -> Result<i32> {
    if std::env::args_os().len() != 2 {
        return Err("run-coverage accepts no arguments".into());
    }
    #[cfg(not(target_os = "linux"))]
    return Err("coverage storage observation is a Linux workflow adapter".into());
    #[cfg(target_os = "linux")]
    storage::run()
}

pub fn command() -> Result<()> {
    if !supported_profile_platform(std::env::consts::OS) {
        return Err("coverage diagnostic capture is a Linux workflow adapter".into());
    }
    #[cfg(not(target_os = "linux"))]
    return Err("coverage diagnostic capture is a Linux workflow adapter".into());
    #[cfg(target_os = "linux")]
    linux::capture_failure()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_capture_platform_policy_has_linux_positive_and_negative_controls() {
        assert!(supported_profile_platform("linux"));
        for platform in ["windows", "macos", "freebsd"] {
            assert!(!supported_profile_platform(platform));
        }
    }

    // Linux owns the actual capture proof inside the gated submodule;
    // unsupported OSes compile this explicit adapter refusal control.
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn unsupported_platform_is_refused() {
        assert!(matches!(
            command(),
            Err(message) if message == "coverage diagnostic capture is a Linux workflow adapter"
        ));
    }
}
