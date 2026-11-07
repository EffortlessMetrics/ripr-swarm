//! One invocation of ci-policy plus check-fast, with no persisted reuse authority.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

// These exact leaves are unconditional in successful precommit_policy(true),
// including the nine-scan collect-all owner. Fast-only gates are never listed.
const SHARED: &[&str] = &[
    "fmt --check",
    "check-static-language",
    "check-command-catalog",
    "check-no-panic-family",
    "check-allow-attributes",
    "check-file-policy",
    "check-workflows",
    "check-covered-by",
    "check-process-policy",
    "check-network-policy",
    "check-fixture-contracts",
    "check-generated",
    "check-generated-clean",
    "check-lint-policy",
];

/// Constructed only after the native policy phase succeeds and its context is
/// rechecked. It cannot be deserialized, supplied by a caller, or retained for
/// another invocation. The transaction checks the context again after fast.
pub(crate) struct SharedPolicy {
    _private: (),
}

pub(crate) fn leaf(
    proof: Option<&SharedPolicy>,
    name: &str,
    execute: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if proof.is_some() && SHARED.contains(&name) {
        Ok(())
    } else {
        execute()
    }
}

pub(crate) fn reuse_disclosure(proof: Option<&SharedPolicy>, ran: &[&str]) -> String {
    if proof.is_none() {
        return String::new();
    }
    let reused = ran
        .iter()
        .filter(|name| SHARED.contains(name))
        .map(|name| format!("- {name}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "\nPolicy work reused within this session-check invocation:\n{reused}\n\nThe Ran section includes these leaves executed by the successful ci-policy phase. Native file-policy listings ran there; Clippy and other fast-only gates execute in the fast phase. No saved report supplied reuse authority.\n"
    )
}

pub(crate) fn run(
    policy: impl FnOnce() -> Result<(), String>,
    fast: impl FnOnce(&SharedPolicy) -> Result<(), String>,
) -> Result<(), String> {
    crate::ensure_reports_dir()?;
    invalidate_fast("pending session-check")?;
    crate::write_report("session-check.md", "# session-check\n\nStatus: pending\n")?;
    let invocation = format!(
        "{}:{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    );
    let mut identity = None;
    let result = transaction(
        || {
            let context = Context::capture()?;
            if identity.is_none() {
                identity = Some(context.receipt_identity());
            }
            Ok(context)
        },
        policy,
        fast,
    );
    let (status, detail) = match &result {
        Ok(()) => (
            "pass",
            "Both native compositions completed in one unchanged context.",
        ),
        Err(error) => ("failed", error.as_str()),
    };
    let invalidation = if result.is_err() {
        invalidate_fast(detail)
    } else {
        Ok(())
    };
    let publication = crate::write_report(
        "session-check.md",
        &format!(
            "# session-check\n\nStatus: {status}\nInvocation: {invocation}\n{}\n\n{detail}\n\nSee ci-policy.json, ci-policy.md, check-fast.md and check-fast-selector.md for the original phase receipts. This command does not establish hosted or merge-queue qualification.\n",
            identity.as_deref().unwrap_or("Context: unavailable")
        ),
    );
    match (result, invalidation.and(publication)) {
        (Err(primary), Err(report)) => Err(format!(
            "{primary}; additionally failed to publish session refusal: {report}"
        )),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(()), report) => report,
    }
}

fn invalidate_fast(reason: &str) -> Result<(), String> {
    for name in ["check-fast.md", "check-fast-selector.md"] {
        crate::write_report(
            name,
            &format!("# session-check fast phase\n\nStatus: failed\n\n{reason}\n"),
        )?;
    }
    Ok(())
}

fn transaction<C: Eq>(
    mut capture: impl FnMut() -> Result<C, String>,
    policy: impl FnOnce() -> Result<(), String>,
    fast: impl FnOnce(&SharedPolicy) -> Result<(), String>,
) -> Result<(), String> {
    let before = capture()?;
    policy()?;
    if capture()? != before {
        return Err("session-check context changed during ci-policy; shared proof refused".into());
    }
    let proof = SharedPolicy { _private: () };
    fast(&proof)?;
    if capture()? != before {
        return Err(
            "session-check context changed during check-fast; combined proof refused".into(),
        );
    }
    Ok(())
}

#[derive(Clone, Eq, PartialEq)]
struct Context {
    cwd: PathBuf,
    selector: crate::check_fast_selector::Selection,
    tree: Vec<u8>,
    environment: BTreeMap<OsString, OsString>,
    source: Vec<u8>,
    executable: Vec<u8>,
    index: Vec<u8>,
    git_config: Vec<u8>,
    tools: Vec<Vec<u8>>,
    cargo_config: Vec<Vec<u8>>,
}

impl Context {
    fn receipt_identity(&self) -> String {
        let source = self
            .source
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        format!(
            "{}\nCandidate tree: {}\nLive policy and tracked source SHA-256: {source}",
            self.selector.identity_line(),
            String::from_utf8_lossy(&self.tree).trim()
        )
    }

    fn capture() -> Result<Self, String> {
        Self::capture_in(std::env::current_dir().map_err(|e| e.to_string())?)
    }

    fn capture_in(cwd: PathBuf) -> Result<Self, String> {
        let output = |program: &str, args: &[&str]| output_at(program, args, &cwd);
        // A verified preflight receipt is valid for ci-policy's own route, but
        // this composition promises to execute the shared workflow leaf here.
        if std::env::var_os("RIPR_POLICY_PREFLIGHT_RECEIPT").is_some() {
            return Err("session-check requires direct policy execution, without RIPR_POLICY_PREFLIGHT_RECEIPT".into());
        }
        let top = output("git", &["rev-parse", "--show-toplevel"])?;
        let root = PathBuf::from(String::from_utf8(top).map_err(|e| e.to_string())?.trim());
        if fs::canonicalize(&cwd).map_err(|e| e.to_string())?
            != fs::canonicalize(root).map_err(|e| e.to_string())?
        {
            return Err("session-check must run at the repository root".into());
        }
        if !output(
            "git",
            &[
                "--no-optional-locks",
                "status",
                "--porcelain",
                "--untracked-files=all",
            ],
        )?
        .is_empty()
        {
            return Err(
                "session-check requires a clean tracked/index/untracked source state".into(),
            );
        }
        let files = output("git", &["ls-files", "--cached", "-z"])?;
        let source = source_digest(&cwd, &source_files(&cwd, &files)?)?;
        let mut cargo_config = Vec::new();
        // Packaged-crate provenance can be ignored by Git but still changes
        // the build script's identity. Include it explicitly.
        cargo_config.push(optional_file(
            &cwd.join("crates/ripr/.cargo_vcs_info.json"),
        )?);
        // Cargo configuration can also live above the checkout. Include every
        // ancestor plus Cargo/Rustup homes; env, including wrapper selections,
        // is compared separately. Missing files are explicit inputs too.
        for dir in cwd.ancestors() {
            for name in [
                ".cargo/config",
                ".cargo/config.toml",
                "rust-toolchain",
                "rust-toolchain.toml",
            ] {
                cargo_config.push(optional_file(&dir.join(name))?);
            }
        }
        // Use Cargo/Rustup's native resolver, including relative and empty
        // environment values and OS-home fallback.
        let cargo_home = home::cargo_home_with_cwd(&cwd).map_err(|e| e.to_string())?;
        for name in ["config", "config.toml"] {
            cargo_config.push(optional_file(&cargo_home.join(name))?);
        }
        let rustup_home = home::rustup_home_with_cwd(&cwd).map_err(|e| e.to_string())?;
        cargo_config.push(optional_file(&rustup_home.join("settings.toml"))?);
        let sysroot = output("rustc", &["--print", "sysroot"])?;
        let sysroot_path = PathBuf::from(
            String::from_utf8(sysroot.clone())
                .map_err(|e| e.to_string())?
                .trim(),
        );
        let mut tools = vec![
            output("cargo", &["-vV"])?,
            output("rustc", &["-vV"])?,
            sysroot,
        ];
        for tool in ["cargo", "rustc", "git"] {
            tools.push(program_digest(OsString::from(tool))?);
        }
        for tool in ["cargo", "rustc"] {
            let suffix = if cfg!(windows) { ".exe" } else { "" };
            tools.push(optional_file(
                &sysroot_path.join("bin").join(format!("{tool}{suffix}")),
            )?);
        }
        for key in [
            "RUSTC",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "CARGO_BUILD_RUSTC",
            "CARGO_BUILD_RUSTC_WRAPPER",
            "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
        ] {
            if let Some(program) = std::env::var_os(key).filter(|v| !v.is_empty()) {
                tools.push(program_digest(program)?);
            }
        }
        Ok(Self {
            selector: crate::check_fast_selector::discover(&cwd)?,
            tree: output("git", &["rev-parse", "HEAD^{tree}"])?,
            environment: std::env::vars_os().collect(),
            source,
            executable: optional_file(&std::env::current_exe().map_err(|e| e.to_string())?)?,
            index: output("git", &["ls-files", "--stage", "-z"])?,
            git_config: output("git", &["config", "--null", "--show-origin", "--list"])?,
            tools,
            cargo_config,
            cwd,
        })
    }
}

fn output_at(program: &str, args: &[&str], cwd: &Path) -> Result<Vec<u8>, String> {
    let args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let result = match crate::run::capture_bytes_in_dir_with_timeout(
        Path::new(program),
        &args,
        cwd,
        &[],
        &[],
        std::time::Duration::from_secs(30),
        "session-check context query",
    ) {
        Ok(result) => result,
        // Do not expose captured config/environment contents in an error.
        Err(_) => return Err(format!("session-check {program} context query unavailable")),
    };
    if result.timed_out || !result.status.is_some_and(|status| status.success()) {
        // Captured configuration may contain secrets; never print it.
        return Err(format!("session-check {program} context query failed"));
    }
    Ok(result.stdout)
}

fn program_digest(program: OsString) -> Result<Vec<u8>, String> {
    let path = PathBuf::from(&program);
    let paths = if path.components().count() > 1 || path.is_absolute() {
        vec![path]
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|dir| dir.join(&program))
            .collect()
    };
    for path in paths {
        for suffix in if cfg!(windows) {
            &["", ".exe", ".cmd", ".bat"][..]
        } else {
            &[""][..]
        } {
            let mut candidate = path.as_os_str().to_os_string();
            candidate.push(suffix);
            let candidate = PathBuf::from(candidate);
            if candidate.is_file() {
                let canonical = fs::canonicalize(candidate).map_err(|e| e.to_string())?;
                let mut result = canonical.as_os_str().as_encoded_bytes().to_vec();
                result.push(0);
                result.extend(optional_file(&canonical)?);
                return Ok(result);
            }
        }
    }
    Err("session-check cannot fingerprint a selected tool".into())
}

fn optional_file(path: &Path) -> Result<Vec<u8>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok([vec![1], Sha256::digest(&bytes).to_vec()].concat()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![0]),
        Err(e) => Err(format!("session-check configuration read: {e}")),
    }
}

/// Policy enumerates live files rather than only Git's tracked set. Include
/// ignored source inputs too, using that owner's exact generated-path boundary.
fn source_files(root: &Path, tracked: &[u8]) -> Result<Vec<u8>, String> {
    fn visit(root: &Path, path: &Path, files: &mut BTreeSet<String>) -> Result<(), String> {
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
        let relative = relative
            .to_str()
            .ok_or("session-check needs UTF-8 source paths")?;
        if repo_policy::should_skip_path(&relative.replace('\\', "/")) {
            return Ok(());
        }
        let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("session-check cannot freeze symlinked live policy inputs".into());
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
                visit(root, &entry.map_err(|e| e.to_string())?.path(), files)?;
            }
        } else if metadata.is_file() {
            files.insert(relative.to_string());
        } else {
            return Err("session-check cannot freeze non-file live policy inputs".into());
        }
        Ok(())
    }
    let mut files = BTreeSet::new();
    visit(root, root, &mut files)?;
    for path in tracked.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        files.insert(
            std::str::from_utf8(path)
                .map_err(|error| format!("session-check needs UTF-8 source paths: {error}"))?
                .to_string(),
        );
    }
    Ok(files
        .into_iter()
        .flat_map(|path| path.into_bytes().into_iter().chain([0]))
        .collect())
}

fn source_digest(root: &Path, files: &[u8]) -> Result<Vec<u8>, String> {
    let mut digest = Sha256::new();
    for name in files.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let name = std::str::from_utf8(name)
            .map_err(|error| format!("session-check needs UTF-8 source paths: {error}"))?;
        let path = root.join(name);
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("session-check cannot freeze symlink/submodule source inputs; use the original commands".into());
        }
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            digest.update(metadata.permissions().mode().to_le_bytes());
        }
        #[cfg(not(unix))]
        digest.update([u8::from(metadata.permissions().readonly())]);
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    Ok(digest.finalize().to_vec())
}

#[cfg(test)]
#[path = "session_check/tests.rs"]
mod tests;
