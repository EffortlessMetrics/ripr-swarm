//! Lockstep publish rehearsal for `ripr` + `ripr-contract` (ADR 0027).
//!
//! `ripr` depends on `ripr-contract` with an exact `=x.y.z` requirement and
//! the two crates publish in lockstep. Until `ripr-contract` exists on
//! crates.io, a bare `cargo publish -p ripr --dry-run` (and even
//! `cargo package -p ripr`) fails with `no matching package named
//! ripr-contract found`, even though the pair is publishable in order.
//! This rehearsal proves the ordered publish without publishing anything:
//!
//! 1. `cargo publish -p ripr-contract --dry-run` (the first publish step,
//!    including its verify build).
//! 2. Package `ripr` against a vendored source tree (`cargo vendor` plus
//!    the packaged `ripr-contract` crate), which is the only local source
//!    that can satisfy the exact registry requirement pre-publish.
//! 3. Compile the packaged `ripr` against the packaged `ripr-contract`
//!    via a `[patch.crates-io]` path entry, after a fresh
//!    `cargo generate-lockfile` (the unlocked-install resolution).

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::Digest as _;

pub(crate) fn run() -> Result<(), String> {
    let status = Command::new("cargo")
        .args(["publish", "-p", "ripr-contract", "--dry-run"])
        .status()
        .map_err(|e| format!("ripr-contract publish dry-run failed to start: {e}"))?;
    if !status.success() {
        return Err("ripr-contract publish dry-run failed".to_string());
    }

    let version = pkgid_version("ripr")?;
    let scratch =
        std::env::temp_dir().join(format!("ripr-publish-rehearsal-{}", std::process::id()));
    if scratch.exists() {
        fs::remove_dir_all(&scratch)
            .map_err(|e| format!("cannot clear {}: {e}", scratch.display()))?;
    }
    fs::create_dir_all(&scratch)
        .map_err(|e| format!("cannot create {}: {e}", scratch.display()))?;
    let result = rehearse(&scratch, &version);
    if result.is_ok() {
        let _ = fs::remove_dir_all(&scratch);
    } else {
        eprintln!("rehearsal scratch retained at {}", scratch.display());
    }
    result
}

/// Package `ripr-contract` and then `ripr` against a vendored source tree
/// holding the packaged contract. Returns the two `.crate` paths. This is
/// the only local route that can produce the `ripr` package before
/// `ripr-contract` exists on crates.io.
pub(crate) fn vendored_ripr_crate(
    scratch: &Path,
    version: &str,
) -> Result<(PathBuf, PathBuf), String> {
    let target = scratch.join("target");
    let target_arg = target.to_string_lossy().into_owned();

    run_cargo(
        &[
            "package",
            "-p",
            "ripr-contract",
            "--no-verify",
            "--allow-dirty",
        ],
        &[("CARGO_TARGET_DIR", target_arg.as_str())],
        None,
    )?;
    let contract_crate = pick_crate_file(&target.join("package"), "ripr-contract", version)?;
    let contract_sha = sha256_hex(&contract_crate)?;

    let vendor = scratch.join("vendor");
    let vendor_arg = vendor.to_string_lossy().into_owned();
    run_cargo(&["vendor", vendor_arg.as_str()], &[], None)?;
    let vendored_contract = vendor.join(format!("ripr-contract-{version}"));
    extract_crate(
        &contract_crate,
        &vendored_contract,
        &format!("ripr-contract-{version}"),
    )?;
    fs::write(
        vendored_contract.join(".cargo-checksum.json"),
        checksum_json(&contract_sha),
    )
    .map_err(|e| format!("cannot write vendored checksum: {e}"))?;

    // `cargo package` re-resolves and may rewrite Cargo.lock; restore it.
    let lock_path = PathBuf::from("Cargo.lock");
    let lock_backup = fs::read(&lock_path).ok();
    let vendor_cfg = vendor.to_string_lossy().replace('\\', "/");
    let packaged = run_cargo(
        &["package", "-p", "ripr", "--no-verify", "--allow-dirty"],
        &[("CARGO_TARGET_DIR", target_arg.as_str())],
        Some(&[
            "source.crates-io.replace-with=\"vendored\"".to_string(),
            format!("source.vendored.directory=\"{vendor_cfg}\""),
        ]),
    );
    if let Some(bytes) = lock_backup {
        let _ = fs::write(&lock_path, bytes);
    }
    packaged?;
    let ripr_crate = pick_crate_file(&target.join("package"), "ripr", version)?;
    Ok((contract_crate, ripr_crate))
}

fn rehearse(scratch: &Path, version: &str) -> Result<(), String> {
    verify_workspace_lockstep_edge(version)?;
    let (contract_crate, ripr_crate) = vendored_ripr_crate(scratch, version)?;
    let contract_sha = sha256_hex(&contract_crate)?;
    let ripr_sha = sha256_hex(&ripr_crate)?;
    // Leave the packaged crates where `cargo package` would put them, so
    // callers (CI release-proof) can consume the exact rehearsed archives.
    let package_out = Path::new("target/package");
    fs::create_dir_all(package_out)
        .map_err(|e| format!("cannot create {}: {e}", package_out.display()))?;
    for krate in [&contract_crate, &ripr_crate] {
        let name = krate
            .file_name()
            .ok_or_else(|| format!("crate path has no file name: {}", krate.display()))?;
        fs::copy(krate, package_out.join(name))
            .map_err(|e| format!("cannot copy {}: {e}", krate.display()))?;
    }
    let receipt = PublishReceipt::capture(
        version,
        &contract_crate,
        &contract_sha,
        &ripr_crate,
        &ripr_sha,
    )?;
    let receipt_path = package_out.join("publish-receipt.json");
    let receipt_json = serde_json::to_string_pretty(&receipt)
        .map_err(|e| format!("cannot serialize publish receipt: {e}"))?;
    fs::write(&receipt_path, format!("{receipt_json}\n"))
        .map_err(|e| format!("cannot write {}: {e}", receipt_path.display()))?;

    let extract = scratch.join("extract");
    let contract_dir = format!("ripr-contract-{version}");
    extract_crate(&contract_crate, &extract.join(&contract_dir), &contract_dir)?;
    let ripr_dirname = format!("ripr-{version}");
    let ripr_dir = extract.join(&ripr_dirname);
    extract_crate(&ripr_crate, &ripr_dir, &ripr_dirname)?;

    let manifest = ripr_dir.join("Cargo.toml");
    let text = fs::read_to_string(&manifest)
        .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?;
    fs::write(&manifest, append_patch(&text, &contract_dir)?)
        .map_err(|e| format!("cannot write {}: {e}", manifest.display()))?;
    let lock = ripr_dir.join("Cargo.lock");
    if lock.exists() {
        fs::remove_file(&lock).map_err(|e| format!("cannot remove lockfile: {e}"))?;
    }
    for args in [
        vec!["generate-lockfile"],
        vec!["check", "--target-dir", "check-target"],
    ] {
        let status = Command::new("cargo")
            .args(&args)
            .current_dir(&ripr_dir)
            .status()
            .map_err(|e| format!("cargo {} failed to start: {e}", args[0]))?;
        if !status.success() {
            return Err(format!(
                "packaged ripr failed `cargo {}` against packaged ripr-contract",
                args.join(" ")
            ));
        }
    }
    println!(
        "publish rehearsal ok: ripr-contract {version} ({contract_sha}) dry-run passed; packaged ripr {version} ({ripr_sha}) packaged against vendored ripr-contract and checks against the packaged crate; receipt={}",
        receipt_path.display()
    );
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct PublishReceipt {
    pub(crate) version: String,
    pub(crate) source_commit: String,
    pub(crate) source_dirty: bool,
    pub(crate) contract_filename: String,
    pub(crate) contract_sha256: String,
    pub(crate) ripr_filename: String,
    pub(crate) ripr_sha256: String,
}

impl PublishReceipt {
    fn capture(
        version: &str,
        contract_crate: &Path,
        contract_sha256: &str,
        ripr_crate: &Path,
        ripr_sha256: &str,
    ) -> Result<Self, String> {
        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .map_err(|e| format!("git rev-parse HEAD failed: {e}"))?;
        let source_commit = if head.status.success() {
            String::from_utf8_lossy(&head.stdout).trim().to_string()
        } else {
            "unknown".to_string()
        };
        let status = Command::new("git")
            .args(["status", "--porcelain"])
            .output()
            .map_err(|e| format!("git status --porcelain failed: {e}"))?;
        let source_dirty = !status.status.success() || !status.stdout.is_empty();
        let file_name = |p: &Path| -> Result<String, String> {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .ok_or_else(|| format!("crate path has no file name: {}", p.display()))
        };
        Ok(Self {
            version: version.to_string(),
            source_commit,
            source_dirty,
            contract_filename: file_name(contract_crate)?,
            contract_sha256: contract_sha256.to_string(),
            ripr_filename: file_name(ripr_crate)?,
            ripr_sha256: ripr_sha256.to_string(),
        })
    }
}

fn verify_workspace_lockstep_edge(workspace_version: &str) -> Result<(), String> {
    let ripr_manifest = fs::read_to_string("crates/ripr/Cargo.toml")
        .map_err(|e| format!("cannot read crates/ripr/Cargo.toml: {e}"))?;
    let contract_manifest = fs::read_to_string("crates/ripr-contract/Cargo.toml")
        .map_err(|e| format!("cannot read crates/ripr-contract/Cargo.toml: {e}"))?;
    verify_lockstep_contract_edge(&ripr_manifest, &contract_manifest, workspace_version)
}

fn manifest_package_version(
    doc: &toml::Value,
    workspace_version: &str,
    label: &str,
) -> Result<String, String> {
    let package = doc
        .get("package")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("{label} missing [package] table"))?;
    match package.get("version") {
        Some(toml::Value::String(v)) => Ok(v.clone()),
        Some(toml::Value::Table(t))
            if t.get("workspace").and_then(toml::Value::as_bool) == Some(true) =>
        {
            Ok(workspace_version.to_string())
        }
        _ => Err(format!(
            "{label} has missing or non-workspace package.version"
        )),
    }
}

pub(crate) fn verify_lockstep_contract_edge(
    ripr_manifest: &str,
    contract_manifest: &str,
    workspace_version: &str,
) -> Result<(), String> {
    let ripr_doc: toml::Value =
        toml::from_str(ripr_manifest).map_err(|e| format!("invalid ripr Cargo.toml: {e}"))?;
    let contract_doc: toml::Value = toml::from_str(contract_manifest)
        .map_err(|e| format!("invalid ripr-contract Cargo.toml: {e}"))?;

    let ripr_ver =
        manifest_package_version(&ripr_doc, workspace_version, "crates/ripr/Cargo.toml")?;
    let contract_ver = manifest_package_version(
        &contract_doc,
        workspace_version,
        "crates/ripr-contract/Cargo.toml",
    )?;
    if ripr_ver != workspace_version || contract_ver != workspace_version {
        return Err(format!(
            "lockstep version mismatch: workspace={workspace_version}, ripr={ripr_ver}, ripr-contract={contract_ver}"
        ));
    }

    let expected_req = format!("={workspace_version}");
    let contract_dep = ripr_doc
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .and_then(|deps| deps.get("ripr-contract"))
        .and_then(toml::Value::as_table)
        .ok_or_else(|| "crates/ripr/Cargo.toml missing [dependencies.ripr-contract]".to_string())?;
    let actual_req = contract_dep
        .get("version")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| {
            "crates/ripr/Cargo.toml ripr-contract dependency missing version".to_string()
        })?;
    if actual_req != expected_req {
        return Err(format!(
            "crates/ripr/Cargo.toml ripr-contract version `{actual_req}` != expected `{expected_req}`"
        ));
    }
    Ok(())
}

/// Public crates.io state of `ripr-contract` before/during two-crate publication.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContractRegistryState {
    Absent,
    MatchingImmutable {
        version: String,
        sha256: String,
    },
    MismatchedImmutable {
        version: String,
        expected_sha256: String,
        actual_sha256: String,
    },
    PropagationTimeout {
        version: String,
    },
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PublicationDisposition {
    PublishContractThenProduct,
    ResumeProductOnly {
        version: String,
        contract_sha256: String,
    },
    PreserveCargoAndRecoverLaterChannel,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PublicationStopReason {
    UnpublishedInternalDependency,
    MismatchedImmutableContract {
        version: String,
        expected_sha256: String,
        actual_sha256: String,
    },
    PropagationTimeout {
        version: String,
    },
}

/// Evaluate the two-crate Cargo publication transaction (#14792).
#[cfg(test)]
pub(crate) fn evaluate_two_crate_publication(
    contract_state: ContractRegistryState,
    product_attempted_first: bool,
    cargo_both_published: bool,
    later_channel_failed: bool,
) -> Result<PublicationDisposition, PublicationStopReason> {
    if cargo_both_published && later_channel_failed {
        return Ok(PublicationDisposition::PreserveCargoAndRecoverLaterChannel);
    }
    match contract_state {
        ContractRegistryState::MismatchedImmutable {
            version,
            expected_sha256,
            actual_sha256,
        } => Err(PublicationStopReason::MismatchedImmutableContract {
            version,
            expected_sha256,
            actual_sha256,
        }),
        ContractRegistryState::PropagationTimeout { version } => {
            Err(PublicationStopReason::PropagationTimeout { version })
        }
        ContractRegistryState::Absent => {
            if product_attempted_first {
                Err(PublicationStopReason::UnpublishedInternalDependency)
            } else {
                Ok(PublicationDisposition::PublishContractThenProduct)
            }
        }
        ContractRegistryState::MatchingImmutable { version, sha256 } => {
            Ok(PublicationDisposition::ResumeProductOnly {
                version,
                contract_sha256: sha256,
            })
        }
    }
}

fn run_cargo(
    args: &[&str],
    envs: &[(&str, &str)],
    configs: Option<&[String]>,
) -> Result<(), String> {
    let mut command = Command::new("cargo");
    if let Some(configs) = configs {
        for config in configs {
            command.args(["--config", config]);
        }
    }
    command.args(args).envs(envs.iter().copied());
    let status = command
        .status()
        .map_err(|e| format!("cargo {} failed to start: {e}", args.join(" ")))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cargo {} failed", args.join(" ")))
    }
}

pub(crate) fn extract_crate(krate: &Path, dest: &Path, top_dir: &str) -> Result<(), String> {
    let file =
        fs::File::open(krate).map_err(|e| format!("cannot open {}: {e}", krate.display()))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    fs::create_dir_all(dest).map_err(|e| format!("cannot create {}: {e}", dest.display()))?;
    let prefix = format!("{top_dir}/");
    for entry in archive
        .entries()
        .map_err(|e| format!("cannot read {}: {e}", krate.display()))?
    {
        let mut entry = entry.map_err(|e| format!("bad entry in {}: {e}", krate.display()))?;
        let path = entry
            .path()
            .map_err(|e| format!("bad entry path in {}: {e}", krate.display()))?
            .into_owned();
        let path_text = path.to_string_lossy().into_owned();
        let Some(relative) = path_text.strip_prefix(&prefix) else {
            continue;
        };
        if relative.is_empty() {
            continue;
        }
        let out = dest.join(relative);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("cannot extract {}: {e}", path.display()))?;
        fs::write(&out, buf).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    }
    Ok(())
}

fn sha256_hex(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok(format!("{:x}", sha2::Sha256::digest(&bytes)))
}

fn pkgid_version(package: &str) -> Result<String, String> {
    let output = Command::new("cargo")
        .args(["pkgid", "-p", package])
        .output()
        .map_err(|e| format!("cargo pkgid -p {package} failed to start: {e}"))?;
    if !output.status.success() {
        return Err(format!("cargo pkgid -p {package} failed"));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_pkgid_version(text.trim())
}

pub(crate) fn parse_pkgid_version(pkgid: &str) -> Result<String, String> {
    pkgid
        .rsplit('#')
        .next()
        .and_then(|tail| tail.rsplit('@').next())
        .map(str::trim)
        .filter(|v| !v.is_empty() && v.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(str::to_string)
        .ok_or_else(|| format!("cannot parse version from pkgid `{pkgid}`"))
}

pub(crate) fn pick_crate_file(dir: &Path, package: &str, version: &str) -> Result<PathBuf, String> {
    let expected = dir.join(format!("{package}-{version}.crate"));
    if expected.is_file() {
        return Ok(expected);
    }
    Err(format!(
        "packaged crate {} not found (cargo package did not produce it)",
        expected.display()
    ))
}

pub(crate) fn checksum_json(sha256: &str) -> String {
    format!("{{\"files\":{{}},\"package\":\"{sha256}\"}}")
}

pub(crate) fn append_patch(manifest: &str, contract_dir: &str) -> Result<String, String> {
    if manifest.contains("[patch.crates-io]") {
        return Err(
            "packaged ripr Cargo.toml already has a [patch.crates-io] section; refusing to guess"
                .to_string(),
        );
    }
    let mut out = manifest.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!(
        "\n[patch.crates-io]\nripr-contract = {{ path = \"../{contract_dir}\" }}\n\n[workspace]\n"
    ));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pkgid_version() -> Result<(), String> {
        assert_eq!(
            parse_pkgid_version(
                "registry+https://github.com/rust-lang/crates.io-index#ripr@0.11.0"
            )?,
            "0.11.0"
        );
        assert_eq!(parse_pkgid_version("path+file:///repo#0.11.0")?, "0.11.0");
        assert!(matches!(parse_pkgid_version("no-version-here"), Err(e) if !e.is_empty()));
        Ok(())
    }

    #[test]
    fn appends_patch_once_and_refuses_second() -> Result<(), String> {
        let manifest = "[package]\nname = \"ripr\"\n";
        let patched = append_patch(manifest, "ripr-contract-0.11.0")?;
        assert!(patched.contains("[patch.crates-io]"));
        assert!(patched.contains("ripr-contract = { path = \"../ripr-contract-0.11.0\" }"));
        assert!(patched.ends_with("[workspace]\n"));
        assert!(matches!(append_patch(&patched, "ripr-contract-0.11.0"), Err(e) if !e.is_empty()));
        Ok(())
    }

    #[test]
    fn checksum_json_names_the_package_digest() {
        assert_eq!(
            checksum_json("abc123"),
            "{\"files\":{},\"package\":\"abc123\"}"
        );
    }

    #[test]
    fn extracts_crate_stripping_top_dir() -> Result<(), String> {
        let io = |e: std::io::Error| e.to_string();
        let dir = std::env::temp_dir().join(format!("ripr-extract-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(io)?;
        // Build a tiny .crate (tar.gz) with one file under the top dir.
        let crate_path = dir.join("demo-1.0.0.crate");
        {
            let file = fs::File::create(&crate_path).map_err(io)?;
            let gz = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
            let mut builder = tar::Builder::new(gz);
            let data = b"hello";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            builder
                .append_data(&mut header, "demo-1.0.0/src/lib.rs", &data[..])
                .map_err(io)?;
            builder.into_inner().map_err(io)?.finish().map_err(io)?;
        }
        let dest = dir.join("demo-1.0.0");
        extract_crate(&crate_path, &dest, "demo-1.0.0")?;
        assert_eq!(fs::read(dest.join("src/lib.rs")).map_err(io)?, b"hello");
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn picks_exact_crate_file() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-pick-test-{}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = dir.join("ripr-0.11.0.crate");
        fs::write(&file, b"x").map_err(|e| e.to_string())?;
        assert_eq!(pick_crate_file(&dir, "ripr", "0.11.0")?, file);
        assert!(matches!(pick_crate_file(&dir, "ripr", "0.10.0"), Err(e) if !e.is_empty()));
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn lockstep_contract_edge_accepts_exact_pin_and_rejects_drift() -> Result<(), String> {
        let root_manifest =
            fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml"))
                .map_err(|e| format!("read root Cargo.toml: {e}"))?;
        let root_doc: toml::Value =
            toml::from_str(&root_manifest).map_err(|e| format!("parse root Cargo.toml: {e}"))?;
        let workspace_version = root_doc
            .get("workspace")
            .and_then(|w| w.get("package"))
            .and_then(|p| p.get("version"))
            .and_then(toml::Value::as_str)
            .ok_or_else(|| "missing workspace.package.version".to_string())?;

        let ripr_manifest = fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../crates/ripr/Cargo.toml"
        ))
        .map_err(|e| format!("read ripr Cargo.toml: {e}"))?;
        let contract_manifest = fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../crates/ripr-contract/Cargo.toml"
        ))
        .map_err(|e| format!("read ripr-contract Cargo.toml: {e}"))?;

        verify_lockstep_contract_edge(&ripr_manifest, &contract_manifest, workspace_version)?;

        let weakened = ripr_manifest.replace(
            &format!("={workspace_version}"),
            &format!("^{workspace_version}"),
        );
        let Err(err) =
            verify_lockstep_contract_edge(&weakened, &contract_manifest, workspace_version)
        else {
            return Err("non-exact ripr-contract version requirement must fail".to_string());
        };
        assert!(err.contains("!="), "unexpected error: {err}");
        Ok(())
    }

    #[test]
    fn two_crate_publication_state_machine_enforces_order_and_recovery() {
        assert_eq!(
            evaluate_two_crate_publication(ContractRegistryState::Absent, false, false, false),
            Ok(PublicationDisposition::PublishContractThenProduct)
        );
        // Negative control: attempting ripr before ripr-contract is rejected.
        assert_eq!(
            evaluate_two_crate_publication(ContractRegistryState::Absent, true, false, false),
            Err(PublicationStopReason::UnpublishedInternalDependency)
        );
        // Contract-only partial completion: resume product-only publish.
        assert_eq!(
            evaluate_two_crate_publication(
                ContractRegistryState::MatchingImmutable {
                    version: "0.11.0".to_string(),
                    sha256: "abc".to_string(),
                },
                false,
                false,
                false,
            ),
            Ok(PublicationDisposition::ResumeProductOnly {
                version: "0.11.0".to_string(),
                contract_sha256: "abc".to_string(),
            })
        );
        // Mismatched immutable contract: stop and bump coordinated version.
        assert_eq!(
            evaluate_two_crate_publication(
                ContractRegistryState::MismatchedImmutable {
                    version: "0.11.0".to_string(),
                    expected_sha256: "abc".to_string(),
                    actual_sha256: "def".to_string(),
                },
                false,
                false,
                false,
            ),
            Err(PublicationStopReason::MismatchedImmutableContract {
                version: "0.11.0".to_string(),
                expected_sha256: "abc".to_string(),
                actual_sha256: "def".to_string(),
            })
        );
        // Propagation timeout: stop, do not publish around edge.
        assert_eq!(
            evaluate_two_crate_publication(
                ContractRegistryState::PropagationTimeout {
                    version: "0.11.0".to_string(),
                },
                false,
                false,
                false,
            ),
            Err(PublicationStopReason::PropagationTimeout {
                version: "0.11.0".to_string(),
            })
        );
        // Later channel failed after Cargo succeeded: preserve Cargo publication.
        assert_eq!(
            evaluate_two_crate_publication(
                ContractRegistryState::MatchingImmutable {
                    version: "0.11.0".to_string(),
                    sha256: "abc".to_string(),
                },
                false,
                true,
                true,
            ),
            Ok(PublicationDisposition::PreserveCargoAndRecoverLaterChannel)
        );
    }

    #[test]
    fn release_guide_binds_to_lockstep_contract_dependency() -> Result<(), String> {
        let ripr_manifest = fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../crates/ripr/Cargo.toml"
        ))
        .map_err(|e| format!("read ripr Cargo.toml: {e}"))?;
        let release_doc =
            fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/RELEASE.md"))
                .map_err(|e| format!("read docs/RELEASE.md: {e}"))?;

        if ripr_manifest.contains("ripr-contract") {
            // Standalone product dry-run must not appear as an instruction line.
            for line in release_doc.lines() {
                assert_ne!(
                    line.trim(),
                    "cargo publish -p ripr --dry-run",
                    "docs/RELEASE.md must not instruct standalone `cargo publish -p ripr --dry-run`"
                );
            }
            for required in [
                "cargo xtask publish-dry-run",
                "cargo search ripr-contract",
                "crates/ripr-contract",
                "cargo publish -p ripr-contract --locked",
                "cargo publish -p ripr --locked",
                "Partial-publication recovery",
            ] {
                assert!(
                    release_doc.contains(required),
                    "docs/RELEASE.md missing lockstep publication requirement `{required}`"
                );
            }
        }
        Ok(())
    }
}
