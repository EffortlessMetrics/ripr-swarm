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

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn run() -> Result<(), String> {
    let status = Command::new("cargo")
        .args(["publish", "-p", "ripr-contract", "--dry-run"])
        .status()
        .map_err(|e| format!("ripr-contract publish dry-run failed to start: {e}"))?;
    if !status.success() {
        return Err("ripr-contract publish dry-run failed".to_string());
    }

    let version = pkgid_version("ripr")?;
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
    let result = rehearse(&scratch, &version).and_then(|()| keep_stage(&scratch));
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

/// Move the rehearsed stage to [`PUBLISH_STAGE_DIR`], replacing an older one.
fn keep_stage(scratch: &Path) -> Result<(), String> {
    let kept = Path::new(PUBLISH_STAGE_DIR);
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
    println!(
        "publish stage kept at {PUBLISH_STAGE_DIR}: run the real `cargo publish --allow-dirty -p ripr-contract` and then `cargo publish --allow-dirty -p ripr` from there"
    );
    Ok(())
}

/// Package `ripr-contract` and `ripr` in one multi-package `cargo package`,
/// which produces the `ripr` package before `ripr-contract` exists on
/// crates.io. Returns the two `.crate` paths. Both crates are packaged from the
/// publish stage under `scratch/stage`, whose `ripr` source has no inline
/// test modules (#14198).
pub(crate) fn staged_crates(scratch: &Path, version: &str) -> Result<(PathBuf, PathBuf), String> {
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
    let crates = packaged?;
    // The packaged bytes are only as trustworthy as the stage they came from;
    // prove it is still HEAD plus the strip before anyone consumes them.
    crate::publish_stage::verify_stage(&stage)?;
    Ok(crates)
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

fn rehearse(scratch: &Path, version: &str) -> Result<(), String> {
    let (contract_crate, ripr_crate) = staged_crates(scratch, version)?;
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
        "publish rehearsal ok: ripr-contract {version} dry-run passed; packaged ripr {version} packaged with ripr-contract and checks against the packaged crate"
    );
    Ok(())
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
}
