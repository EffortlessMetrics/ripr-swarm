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
//! 2. Package `ripr-contract` and `ripr` in one multi-package `cargo
//!    package`, which resolves the exact requirement against the contract
//!    crate it just packaged.
//! 3. Compile the packaged `ripr` against the packaged `ripr-contract`
//!    via a `[patch.crates-io]` path entry, after a fresh
//!    `cargo generate-lockfile` (the unlocked-install resolution).
//!
//! `ripr` is packaged from the publish stage (`publish_stage`, #14198): a
//! copy of the workspace whose `crates/ripr/src` has its inline
//! `#[cfg(test)]` modules removed, so the archive fits the crates.io upload
//! limit. A passing rehearsal leaves that stage at `target/publish-stage`;
//! the real `cargo publish` runs from there, so it packages the rehearsed,
//! verified source tree (`publish_stage::verify_stage`).

use sha2::Digest as _;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn run() -> Result<(), String> {
    // A receipt is evidence of a completed rehearsal. Clear any earlier one
    // before the first publish step, so a run that fails anywhere cannot
    // leave a stale receipt that satisfies the documented pre-publish gate.
    clear_publish_receipt()?;

    let version = pkgid_version("ripr")?;
    // Refuse a drifted pin before the contract dry-run, not after it.
    verify_workspace_lockstep_edge(&version)?;

    let status = Command::new("cargo")
        .args(["publish", "-p", "ripr-contract", "--dry-run"])
        .status()
        .map_err(|e| format!("ripr-contract publish dry-run failed to start: {e}"))?;
    if !status.success() {
        return Err("ripr-contract publish dry-run failed".to_string());
    }

    // Under `target/`, beside `PUBLISH_STAGE_DIR`, so keeping the stage is a
    // same-filesystem rename even when the system temp dir is another mount.
    // Absolute, because the rehearsal's cargo commands run inside the stage.
    let scratch = std::env::current_dir()
        .map_err(|e| format!("cannot read the current directory: {e}"))?
        .join("target")
        .join(format!("ripr-publish-rehearsal-{}", std::process::id()));
    if scratch.exists() {
        fs::remove_dir_all(&scratch)
            .map_err(|e| format!("cannot clear {}: {e}", scratch.display()))?;
    }
    fs::create_dir_all(&scratch)
        .map_err(|e| format!("cannot create {}: {e}", scratch.display()))?;
    // The receipt is written last, after the stage is retained. A receipt is
    // evidence of a completed rehearsal and `docs/RELEASE.md` trusts it as the
    // pre-publish gate, so writing it before `keep_stage` would leave a
    // current success receipt beside a stage that was never retained.
    let result = rehearse(&scratch, &version).and_then(|receipt| {
        let identity = receipt.stage_identity();
        finish_rehearsal(
            &scratch,
            Path::new(PUBLISH_STAGE_DIR),
            Path::new(PUBLISH_RECEIPT),
            &receipt,
            |kept| crate::publish_stage::verify_stage_matches(kept, &identity),
        )
    });
    if result.is_ok() {
        let _ = fs::remove_dir_all(&scratch);
    } else {
        eprintln!("rehearsal scratch retained at {}", scratch.display());
    }
    result
}

/// Where a passing rehearsal leaves the stage it packaged, for the real
/// publish to run from.
pub(crate) const PUBLISH_STAGE_DIR: &str = "target/publish-stage";

/// Retain the stage, prove it is still the strip the receipt records, then
/// write the receipt. The order is the contract: a receipt must never exist
/// for a stage that was not retained or that no longer matches the strip the
/// archives were built from, because `docs/RELEASE.md` reads it as the
/// pre-publish gate.
fn finish_rehearsal(
    scratch: &Path,
    kept: &Path,
    receipt_path: &Path,
    receipt: &PublishReceipt,
    verify_kept: impl Fn(&Path) -> Result<(), String>,
) -> Result<(), String> {
    keep_stage_at(scratch, kept)?;
    verify_kept(kept)?;
    write_publish_receipt(receipt_path, receipt)?;
    println!(
        "publish rehearsal ok: ripr-contract {} ({}) dry-run passed; packaged ripr {} ({}) packaged with ripr-contract and checks against the packaged crate; stage kept at {}; receipt={}",
        receipt.version,
        receipt.contract_sha256,
        receipt.version,
        receipt.ripr_sha256,
        kept.display(),
        receipt_path.display()
    );
    Ok(())
}

/// Move the rehearsed stage to `kept`, replacing an older one.
fn keep_stage_at(scratch: &Path, kept: &Path) -> Result<(), String> {
    if kept.exists() {
        fs::remove_dir_all(kept).map_err(|e| format!("cannot clear {}: {e}", kept.display()))?;
    }
    if let Some(parent) = kept.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let stage = scratch.join("stage");
    fs::rename(&stage, kept).map_err(|e| {
        format!(
            "cannot move the publish stage {} to {}: {e}",
            stage.display(),
            kept.display()
        )
    })?;
    Ok(())
}

/// Package `ripr-contract` and `ripr` in one multi-package `cargo package`,
/// which produces the `ripr` package before `ripr-contract` exists on
/// crates.io. Returns the two `.crate` paths. Both crates are packaged from the
/// publish stage under `scratch/stage`, whose `ripr` source has no inline
/// test modules (#14198).
pub(crate) fn staged_crates(scratch: &Path, version: &str) -> Result<(PathBuf, PathBuf), String> {
    let (contract_crate, ripr_crate, _stage) = staged_crates_with_report(scratch, version)?;
    Ok((contract_crate, ripr_crate))
}

/// [`staged_crates`], also returning the stage report so the receipt can
/// name the source commit and the transform the archives were built from.
fn staged_crates_with_report(
    scratch: &Path,
    version: &str,
) -> Result<(PathBuf, PathBuf, crate::publish_stage::StageReport), String> {
    let workspace =
        std::env::current_dir().map_err(|e| format!("cannot read the current directory: {e}"))?;
    // The cargo commands below run inside the stage, so every path handed to
    // them must be absolute; `release-readiness` passes a relative scratch.
    let scratch = workspace.join(scratch);
    let stage = scratch.join("stage");
    let report = crate::publish_stage::stage_workspace(&workspace, &stage)?;
    println!(
        "publish stage: HEAD {} checked out; {} inline #[cfg(test)] modules ({} bytes) stripped from {} ripr source files",
        report.commit, report.stripped_modules, report.stripped_bytes, report.stripped_files
    );
    // `cargo package` re-resolves and may rewrite Cargo.lock; restore the
    // committed one afterwards, so the kept stage differs from HEAD only by
    // the strip and its marker.
    let lock_path = stage.join("Cargo.lock");
    let lock_backup = fs::read(&lock_path).ok();
    let packaged = package_from_stage(&scratch, &stage, version);
    if let Some(bytes) = lock_backup {
        fs::write(&lock_path, bytes)
            .map_err(|e| format!("cannot restore {}: {e}", lock_path.display()))?;
    }
    let (contract_crate, ripr_crate) = packaged?;
    // The packaged bytes are only as trustworthy as the stage they came from;
    // prove it is still HEAD plus the exact strip the report recorded before
    // anyone consumes them. A count alone would accept a stage whose stripped
    // file was restored to HEAD, because Git stops reporting that path.
    crate::publish_stage::verify_stage_matches(&stage, &report.identity())?;
    Ok((contract_crate, ripr_crate, report))
}

fn package_from_stage(
    scratch: &Path,
    stage: &Path,
    version: &str,
) -> Result<(PathBuf, PathBuf), String> {
    let target = scratch.join("target");
    let target_arg = target.to_string_lossy().into_owned();
    // One multi-package invocation: Cargo resolves `ripr`'s exact
    // `ripr-contract` requirement against the contract it just packaged
    // (its `target/package/tmp-registry`), so no vendored source is needed
    // before `ripr-contract` is on crates.io (#15204). The unlocked compile in
    // `rehearse` and the `package-install` check build what was packaged.
    run_cargo(
        &[
            "package",
            "-p",
            "ripr-contract",
            "-p",
            "ripr",
            "--no-verify",
            "--allow-dirty",
        ],
        &[("CARGO_TARGET_DIR", target_arg.as_str())],
        stage,
    )?;
    let contract_crate = pick_crate_file(&target.join("package"), "ripr-contract", version)?;
    let ripr_crate = pick_crate_file(&target.join("package"), "ripr", version)?;
    Ok((contract_crate, ripr_crate))
}

fn rehearse(scratch: &Path, version: &str) -> Result<PublishReceipt, String> {
    // The caller verified the lockstep pin before the contract dry-run.
    let (contract_crate, ripr_crate, stage) = staged_crates_with_report(scratch, version)?;
    crate::source_package_contents::check_crate_file("ripr-contract", &contract_crate, version)?;
    crate::source_package_contents::check_crate_file("ripr", &ripr_crate, version)?;
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

    upload_size_guard(&contract_crate, &ripr_crate)?;

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
    let verified = verify_packaged_pair(&ripr_dir)?;
    // Only a pair that passed the packaged check gets a receipt. The caller
    // writes it after the stage is retained, so a failed retention leaves no
    // receipt for `docs/RELEASE.md` to trust.
    PublishReceipt::capture(
        version,
        &stage,
        &contract_crate,
        &contract_sha,
        &ripr_crate,
        &ripr_sha,
        verified,
    )
}

/// Proof that the packaged `ripr` generated a lockfile and compiled against
/// the packaged `ripr-contract`. Only `verify_packaged_pair` returns one, so
/// a receipt cannot be written for a pair that was never checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VerifiedPair(());

fn verify_packaged_pair(ripr_dir: &Path) -> Result<VerifiedPair, String> {
    verify_packaged_pair_with(ripr_dir, |dir, args| {
        let status = Command::new("cargo")
            .args(args)
            .current_dir(dir)
            .status()
            .map_err(|e| format!("cargo {} failed to start: {e}", args[0]))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "packaged ripr failed `cargo {}` against packaged ripr-contract",
                args.join(" ")
            ))
        }
    })
}

fn verify_packaged_pair_with(
    ripr_dir: &Path,
    run: impl Fn(&Path, &[&str]) -> Result<(), String>,
) -> Result<VerifiedPair, String> {
    for args in [
        vec!["generate-lockfile"],
        vec!["check", "--target-dir", "check-target"],
    ] {
        run(ripr_dir, &args)?;
    }
    Ok(VerifiedPair(()))
}

fn write_publish_receipt(path: &Path, receipt: &PublishReceipt) -> Result<(), String> {
    let receipt_json = serde_json::to_string_pretty(receipt)
        .map_err(|e| format!("cannot serialize publish receipt: {e}"))?;
    fs::write(path, format!("{receipt_json}\n"))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn read_publish_receipt(path: &Path) -> Result<PublishReceipt, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("cannot parse {}: {e}", path.display()))
}

/// Fail-closed pre-upload check for the product archive.
///
/// `cargo publish` repacks the stage, so the bytes it uploads are not the
/// rehearsed `.crate`. This packages `ripr` again from the kept stage in
/// registry mode (no vendor replacement, so `ripr-contract = "=x.y.z"`
/// resolves from crates.io, which is true only after the contract publish)
/// and refuses unless the fresh archive's digest equals the receipt's
/// `ripr_sha256`. Run it after the contract readback and immediately before
/// `cargo publish -p ripr`; the public readback afterwards confirms the
/// uploaded bytes are the checked ones.
pub(crate) fn precheck() -> Result<(), String> {
    let receipt = read_publish_receipt(Path::new(PUBLISH_RECEIPT))?;
    if receipt.verification != RehearsalVerification::PackagedCheckPassed {
        return Err(format!(
            "receipt verification is {:?}, not packaged-check-passed",
            receipt.verification
        ));
    }
    let stage = Path::new(PUBLISH_STAGE_DIR);
    if !stage.is_dir() {
        return Err(format!(
            "no retained publish stage at {}; run `cargo xtask publish-dry-run` first",
            stage.display()
        ));
    }
    // The stage must still be the strip the archives were built from, not a
    // self-consistent one whose stripped file was restored to HEAD.
    crate::publish_stage::verify_stage_matches(stage, &receipt.stage_identity())?;
    let target = std::env::current_dir()
        .map_err(|e| format!("cannot read the current directory: {e}"))?
        .join("target")
        .join("publish-precheck");
    if target.exists() {
        fs::remove_dir_all(&target)
            .map_err(|e| format!("cannot clear {}: {e}", target.display()))?;
    }
    let target_arg = target.to_string_lossy().into_owned();
    run_cargo(
        &["package", "-p", "ripr", "--allow-dirty"],
        &[("CARGO_TARGET_DIR", target_arg.as_str())],
        stage,
    )?;
    let fresh = pick_crate_file(&target.join("package"), "ripr", &receipt.version)?;
    let fresh_sha = sha256_hex(&fresh)?;
    require_rehearsed_product(&receipt, &fresh_sha)?;
    println!(
        "publish precheck ok: {} matches the rehearsed ripr_sha256 {}; publish from {} with `cargo publish --allow-dirty -p ripr`",
        fresh.display(),
        receipt.ripr_sha256,
        stage.display()
    );
    Ok(())
}

/// The pre-upload predicate: the bytes about to be uploaded must be the ones
/// the rehearsal qualified. A mismatch is a stop, not a warning.
fn require_rehearsed_product(receipt: &PublishReceipt, fresh_sha256: &str) -> Result<(), String> {
    if fresh_sha256 != receipt.ripr_sha256 {
        return Err(format!(
            "fresh registry-mode package has sha256 {fresh_sha256}, the receipt records {}; refusing to publish bytes the rehearsal did not qualify",
            receipt.ripr_sha256
        ));
    }
    Ok(())
}

/// Where the rehearsal leaves its evidence. `docs/RELEASE.md` reads this path.
const PUBLISH_RECEIPT: &str = "target/package/publish-receipt.json";

/// Remove any receipt from an earlier run. A receipt must never outlive the
/// rehearsal that wrote it, or a failed run would still satisfy the gate.
fn clear_publish_receipt() -> Result<(), String> {
    clear_publish_receipt_at(Path::new(PUBLISH_RECEIPT))
}

fn clear_publish_receipt_at(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot clear stale {}: {e}", path.display())),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct PublishReceipt {
    pub(crate) version: String,
    pub(crate) source_commit: String,
    pub(crate) source_dirty: bool,
    /// The commit the retained stage was checked out from, and the transform
    /// applied to it. A reader can tell which source tree the archives were
    /// built from, not only which checkout ran the rehearsal.
    pub(crate) stage_commit: String,
    pub(crate) stage_stripped_files: usize,
    pub(crate) stage_stripped_modules: usize,
    pub(crate) stage_stripped_bytes: usize,
    /// Fingerprint of the exact stripped path/content set. The counts alone
    /// cannot tell a stage whose stripped file was restored to `HEAD` from a
    /// clean one, so the pre-upload check compares this too.
    #[serde(default)]
    pub(crate) stage_strip_digest: String,
    pub(crate) contract_filename: String,
    pub(crate) contract_sha256: String,
    pub(crate) ripr_filename: String,
    pub(crate) ripr_sha256: String,
    /// What the rehearsal proved before this receipt was written. A receipt
    /// exists only for a pair that passed it, so a reader can tell a
    /// completed rehearsal from a copied pair.
    pub(crate) verification: RehearsalVerification,
}

/// The verification a receipt attests. Serialized as a stable token so a
/// release script can require it rather than infer success from the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RehearsalVerification {
    /// The packaged `ripr` generated a lockfile and compiled against the
    /// packaged `ripr-contract`.
    PackagedCheckPassed,
}

impl PublishReceipt {
    fn capture(
        version: &str,
        stage: &crate::publish_stage::StageReport,
        contract_crate: &Path,
        contract_sha256: &str,
        ripr_crate: &Path,
        ripr_sha256: &str,
        _verified: VerifiedPair,
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
            stage_commit: stage.commit.clone(),
            stage_stripped_files: stage.stripped_files,
            stage_stripped_modules: stage.stripped_modules,
            stage_stripped_bytes: stage.stripped_bytes,
            stage_strip_digest: stage.strip_digest.clone(),
            contract_filename: file_name(contract_crate)?,
            contract_sha256: contract_sha256.to_string(),
            ripr_filename: file_name(ripr_crate)?,
            ripr_sha256: ripr_sha256.to_string(),
            verification: RehearsalVerification::PackagedCheckPassed,
        })
    }

    /// The strip identity the retained stage must still reproduce.
    pub(crate) fn stage_identity(&self) -> crate::publish_stage::StripIdentity {
        crate::publish_stage::StripIdentity {
            files: self.stage_stripped_files,
            modules: self.stage_stripped_modules,
            bytes: self.stage_stripped_bytes,
            digest: self.stage_strip_digest.clone(),
        }
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
    // Both manifests use `version.workspace = true`, so this equality is
    // vacuous for the current tree. The load-bearing check is the exact
    // `=x.y.z` requirement on the dependency below: a drifted pin is what
    // actually breaks the lockstep publish.
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

/// Fail the rehearsal when a packaged crate is over the crates.io default
/// upload limit, with the same rule `release-readiness` applies to these
/// archives (`package-upload-size`), so every lane that rehearses the publish
/// also refuses a crate the registry would refuse (#14198).
fn upload_size_guard(contract_crate: &Path, ripr_crate: &Path) -> Result<(), String> {
    let mut sizes = Vec::new();
    for krate in [contract_crate, ripr_crate] {
        let bytes = fs::metadata(krate)
            .map_err(|e| format!("cannot measure {}: {e}", krate.display()))?
            .len();
        let name = krate
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| format!("crate path has no file name: {}", krate.display()))?;
        println!(
            "packaged {name}: {bytes} bytes (crates.io default upload limit {})",
            crate::reports::release::CRATES_IO_DEFAULT_MAX_UPLOAD_BYTES
        );
        sizes.push((name, bytes));
    }
    let problems = crate::reports::release::upload_size_problems(
        &sizes,
        crate::reports::release::CRATES_IO_DEFAULT_MAX_UPLOAD_BYTES,
    );
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}

fn run_cargo(args: &[&str], envs: &[(&str, &str)], cwd: &Path) -> Result<(), String> {
    let mut command = Command::new("cargo");
    command.current_dir(cwd);
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

    fn sample_receipt() -> PublishReceipt {
        PublishReceipt {
            version: "0.11.0".to_string(),
            source_commit: "abc".to_string(),
            source_dirty: false,
            stage_commit: "abc".to_string(),
            stage_stripped_files: 3,
            stage_stripped_modules: 7,
            stage_stripped_bytes: 1024,
            stage_strip_digest: "d".repeat(64),
            contract_filename: "ripr-contract-0.11.0.crate".to_string(),
            contract_sha256: "c".to_string(),
            ripr_filename: "ripr-0.11.0.crate".to_string(),
            ripr_sha256: "r".to_string(),
            verification: RehearsalVerification::PackagedCheckPassed,
        }
    }

    #[test]
    fn receipt_names_the_verification_it_completed() -> Result<(), String> {
        let json = serde_json::to_string(&sample_receipt()).map_err(|e| e.to_string())?;
        assert!(
            json.contains("\"verification\":\"packaged-check-passed\""),
            "a receipt must name the verification it attests: {json}"
        );
        Ok(())
    }

    #[test]
    fn a_stale_receipt_does_not_survive_into_a_new_rehearsal() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-receipt-clear-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("publish-receipt.json");
        fs::write(&path, b"{\"version\":\"0.10.0\"}").map_err(|e| e.to_string())?;
        clear_publish_receipt_at(&path)?;
        assert!(
            !path.exists(),
            "a receipt from an earlier run must not satisfy the gate for a new one"
        );
        // Clearing an absent receipt is not an error.
        clear_publish_receipt_at(&path)?;
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn a_failed_verification_leaves_no_receipt() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-receipt-fail-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("publish-receipt.json");
        clear_publish_receipt_at(&path)?;

        // The packaged check fails, so the write is never reached. This is the
        // same `?`-then-write ordering `rehearse` uses.
        let outcome = verify_packaged_pair_with(&dir, |_, args| {
            Err(format!("packaged ripr failed `cargo {}`", args.join(" ")))
        })
        .and_then(|verified| write_publish_receipt(&path, &sample_receipt()).map(|()| verified));
        assert!(
            outcome.is_err(),
            "a failing check must not produce a verified pair"
        );
        assert!(
            !path.exists(),
            "a failed verification must leave no receipt for the gate to trust"
        );
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn a_passed_verification_writes_the_receipt() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-receipt-ok-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("publish-receipt.json");
        clear_publish_receipt_at(&path)?;

        let _verified = verify_packaged_pair_with(&dir, |_, _| Ok(()))?;
        write_publish_receipt(&path, &sample_receipt())?;
        assert!(
            path.exists(),
            "a passed verification must leave the receipt"
        );
        let written = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        assert!(
            written.contains("\"verification\": \"packaged-check-passed\""),
            "the written receipt must name its verification: {written}"
        );
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn a_failed_stage_retention_leaves_no_current_receipt() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-retain-fail-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let receipt_path = dir.join("publish-receipt.json");
        // Preseed an old receipt: the gate must not keep trusting it.
        fs::write(&receipt_path, b"{\"version\":\"0.10.0\"}").map_err(|e| e.to_string())?;
        clear_publish_receipt_at(&receipt_path)?;

        // The rehearsal's stage exists, but a regular file sits where the kept
        // directory must be removed, so retention fails after the archive
        // checks would have passed. This is the boundary the early Cargo
        // failure case never reaches.
        let scratch = dir.join("scratch");
        fs::create_dir_all(scratch.join("stage")).map_err(|e| e.to_string())?;
        let kept = dir.join("publish-stage");
        fs::write(&kept, b"not a directory").map_err(|e| e.to_string())?;

        let outcome =
            finish_rehearsal(
                &scratch,
                &kept,
                &receipt_path,
                &sample_receipt(),
                |_| Ok(()),
            );
        assert!(
            outcome.is_err(),
            "a failed stage retention must fail the rehearsal"
        );
        assert!(
            !receipt_path.exists(),
            "a failed retention must leave no current receipt for the gate to trust"
        );
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn a_kept_stage_that_lost_its_strip_leaves_no_receipt() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-retain-strip-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("scratch/stage/src")).map_err(|e| e.to_string())?;
        fs::write(dir.join("scratch/stage/src/lib.rs"), b"// staged").map_err(|e| e.to_string())?;
        let kept = dir.join("publish-stage");
        let receipt_path = dir.join("publish-receipt.json");

        // The stage is retained, but it no longer reproduces the recorded
        // strip (a stripped file was restored to HEAD). The receipt must not
        // be written, or the gate would trust a stage the archives did not
        // come from.
        let outcome = finish_rehearsal(
            &dir.join("scratch"),
            &kept,
            &receipt_path,
            &sample_receipt(),
            |_| Err("publish stage strips 1 file(s) but the rehearsal stripped 3".to_string()),
        );
        assert!(
            outcome.is_err(),
            "a kept stage that lost its strip must fail the rehearsal"
        );
        assert!(
            !receipt_path.exists(),
            "a stage that no longer matches the recorded strip must leave no receipt"
        );
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn the_pre_upload_check_refuses_bytes_the_rehearsal_did_not_qualify() {
        let receipt = sample_receipt();
        assert_eq!(require_rehearsed_product(&receipt, "r"), Ok(()));
        let refused = require_rehearsed_product(&receipt, "not-r");
        assert!(
            matches!(refused, Err(ref e) if e.contains("refusing to publish bytes the rehearsal did not qualify")),
            "a fresh package that differs from the receipt must be refused: {refused:?}"
        );
    }

    #[test]
    fn a_receipt_round_trips_through_the_pre_upload_reader() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-receipt-read-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("publish-receipt.json");
        let receipt = sample_receipt();
        write_publish_receipt(&path, &receipt)?;
        let read = read_publish_receipt(&path)?;
        assert_eq!(read, receipt);
        assert_eq!(
            read.stage_identity(),
            receipt.stage_identity(),
            "the pre-upload check must recover the recorded strip identity from the receipt"
        );
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn a_passed_retention_keeps_the_stage_and_records_both_digests() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-retain-ok-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("scratch/stage/src")).map_err(|e| e.to_string())?;
        fs::write(dir.join("scratch/stage/src/lib.rs"), b"// staged").map_err(|e| e.to_string())?;
        let kept = dir.join("publish-stage");
        let receipt_path = dir.join("publish-receipt.json");

        finish_rehearsal(
            &dir.join("scratch"),
            &kept,
            &receipt_path,
            &sample_receipt(),
            |_| Ok(()),
        )?;

        assert!(
            kept.join("src/lib.rs").exists(),
            "a passed retention must keep the stage the publish runs from"
        );
        let written = fs::read_to_string(&receipt_path).map_err(|e| e.to_string())?;
        assert!(
            written.contains("\"contract_sha256\": \"c\""),
            "the receipt must record the contract archive digest: {written}"
        );
        assert!(
            written.contains("\"ripr_sha256\": \"r\""),
            "the receipt must record the ripr archive digest: {written}"
        );
        assert!(
            written.contains("\"stage_commit\": \"abc\""),
            "the receipt must name the retained stage's source commit: {written}"
        );
        let _ = fs::remove_dir_all(&dir);
        Ok(())
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
            // Commands must appear as instruction lines, not merely as a
            // substring of prose: a required command inside a "do not run"
            // sentence would otherwise satisfy the check.
            for required in [
                "cargo xtask publish-dry-run",
                "cargo xtask publish-precheck",
                "cargo search ripr-contract",
            ] {
                assert!(
                    release_doc
                        .lines()
                        .any(|line| line.trim_start().starts_with(required)),
                    "docs/RELEASE.md must give `{required}` as an instruction line, not only mention it"
                );
            }
            // The real publish runs from the kept stage (#14198), so the guide
            // must name those exact commands. A prefix match would let the
            // `ripr-contract` line satisfy the `ripr` requirement.
            for required in [
                "cargo publish --allow-dirty -p ripr-contract",
                "cargo publish --allow-dirty -p ripr",
            ] {
                assert!(
                    release_doc.lines().any(|line| line.trim() == required),
                    "docs/RELEASE.md must give `{required}` as its own instruction line"
                );
            }
            for required in ["crates/ripr-contract", "Partial-publication recovery"] {
                assert!(
                    release_doc.contains(required),
                    "docs/RELEASE.md missing lockstep publication requirement `{required}`"
                );
            }
        }
        Ok(())
    }
}
