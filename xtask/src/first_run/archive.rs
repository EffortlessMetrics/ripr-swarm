//! Released-archive install route for the first-run walk (#5311).
//!
//! `--install-archive VERSION` does what the README's prebuilt instructions
//! do: download the checksummed server archive for this host's target, verify
//! it against its `.sha256` sidecar, and extract `ripr` into a directory the
//! walk owns. The walk then runs that exact executable by absolute path, so a
//! `ripr` earlier on `PATH` cannot stand in for the install.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::reports::release_server::{server_asset_name, sha256_file};

pub(super) const STEP: &str = "install_archive";
const BASE_URL: &str = "https://github.com/EffortlessMetrics/ripr/releases/download";

/// Server archive targets, matching `release_server`'s published assets.
const TARGETS: [(&str, &str, &str); 5] = [
    ("linux", "x86_64", "x86_64-unknown-linux-gnu"),
    ("linux", "aarch64", "aarch64-unknown-linux-gnu"),
    ("macos", "x86_64", "x86_64-apple-darwin"),
    ("macos", "aarch64", "aarch64-apple-darwin"),
    ("windows", "x86_64", "x86_64-pc-windows-msvc"),
];

/// What the walk records about the archive route.
pub(super) struct ArchiveInstall {
    pub executable: PathBuf,
    pub version: String,
    pub target: &'static str,
    pub asset: String,
    pub sha256: String,
    pub executable_bytes: u64,
}

/// The release target for a host, or an error naming the unsupported host
/// rather than guessing the nearest archive.
pub(super) fn target_for(os: &str, arch: &str) -> Result<&'static str, String> {
    TARGETS
        .iter()
        .find(|(o, a, _)| *o == os && *a == arch)
        .map(|(_, _, target)| *target)
        .ok_or_else(|| format!("no released ripr archive for {os}/{arch}"))
}

fn archive_extension(target: &str) -> &'static str {
    if target.contains("windows") {
        "zip"
    } else {
        "tar.gz"
    }
}

pub(super) fn executable_name(target: &str) -> &'static str {
    if target.contains("windows") {
        "ripr.exe"
    } else {
        "ripr"
    }
}

/// The digest from a `.sha256` sidecar: one 64-hex-digit field, optionally
/// followed by a file name that must be this asset. Anything else is refused,
/// so an HTML error page or another asset's sidecar cannot validate a file.
pub(super) fn parse_sidecar(text: &str, asset: &str) -> Result<String, String> {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let line = lines
        .next()
        .ok_or_else(|| "the checksum sidecar is empty".to_string())?;
    if lines.next().is_some() {
        return Err("the checksum sidecar has more than one entry".to_string());
    }
    let mut fields = line.split_whitespace();
    let digest = fields.next().unwrap_or_default().to_ascii_lowercase();
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("the checksum sidecar does not start with a SHA-256 digest".to_string());
    }
    if let Some(name) = fields.next() {
        let name = name.trim_start_matches('*');
        if name != asset || fields.next().is_some() {
            return Err(format!(
                "the checksum sidecar names `{name}`, not the downloaded `{asset}`"
            ));
        }
    }
    Ok(digest)
}

/// Download, verify and extract. `acquire` copies a URL to a path; production
/// delegates to the walk and tests substitute local bytes.
pub(super) fn install(
    version: &str,
    target: &'static str,
    work: &Path,
    acquire: &dyn Fn(&str, &Path) -> Result<(), String>,
) -> Result<ArchiveInstall, String> {
    let asset = server_asset_name(version, target, archive_extension(target));
    let base = format!("{BASE_URL}/v{version}");
    let download = work.join("download");
    fs::create_dir_all(&download)
        .map_err(|err| format!("failed to create {}: {err}", download.display()))?;
    let archive = download.join(&asset);
    let sidecar = download.join(format!("{asset}.sha256"));
    acquire(&format!("{base}/{asset}"), &archive)?;
    acquire(&format!("{base}/{asset}.sha256"), &sidecar)?;

    let expected = parse_sidecar(
        &fs::read_to_string(&sidecar)
            .map_err(|err| format!("failed to read {}: {err}", sidecar.display()))?,
        &asset,
    )?;
    let actual = sha256_file(&archive)?;
    if actual != expected {
        return Err(format!(
            "checksum mismatch for {asset}: sidecar says {expected}, downloaded file is {actual}"
        ));
    }

    let bin = work.join("bin");
    fs::create_dir_all(&bin).map_err(|err| format!("failed to create {}: {err}", bin.display()))?;
    let executable = bin.join(executable_name(target));
    extract_executable(&archive, target, &executable)?;
    let executable_bytes = fs::metadata(&executable)
        .map_err(|err| format!("failed to stat {}: {err}", executable.display()))?
        .len();
    Ok(ArchiveInstall {
        executable,
        version: version.to_string(),
        target,
        asset,
        sha256: actual,
        executable_bytes,
    })
}

/// Copy only the archive's root `ripr` entry. Other members, nested paths and
/// links are never written, so a hostile archive cannot place files elsewhere.
fn extract_executable(archive: &Path, target: &str, dest: &Path) -> Result<(), String> {
    let wanted = executable_name(target);
    let file = fs::File::open(archive)
        .map_err(|err| format!("failed to open {}: {err}", archive.display()))?;
    if archive_extension(target) == "zip" {
        let mut zip =
            zip::ZipArchive::new(file).map_err(|err| format!("not a readable zip: {err}"))?;
        let mut entry = zip
            .by_name(wanted)
            .map_err(|err| format!("the archive has no root `{wanted}` entry: {err}"))?;
        return write_executable(&mut entry, dest);
    }
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let entries = tar
        .entries()
        .map_err(|err| format!("not a readable tar.gz: {err}"))?;
    for entry in entries {
        let mut entry = entry.map_err(|err| format!("unreadable archive entry: {err}"))?;
        let path = entry
            .path()
            .map_err(|err| format!("unreadable archive entry path: {err}"))?
            .into_owned();
        // Release archives are built with `tar -C dir .`, so the entry is `./ripr`.
        let root_relative: PathBuf = path
            .components()
            .filter(|part| !matches!(part, std::path::Component::CurDir))
            .collect();
        if entry.header().entry_type().is_file() && root_relative == Path::new(wanted) {
            return write_executable(&mut entry, dest);
        }
    }
    Err(format!("the archive has no root `{wanted}` entry"))
}

fn write_executable(from: &mut dyn Read, dest: &Path) -> Result<(), String> {
    let mut out = fs::File::create(dest)
        .map_err(|err| format!("failed to create {}: {err}", dest.display()))?;
    io::copy(from, &mut out).map_err(|err| format!("failed to write {}: {err}", dest.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dest, fs::Permissions::from_mode(0o755))
            .map_err(|err| format!("failed to mark {} executable: {err}", dest.display()))?;
    }
    Ok(())
}

/// The version `ripr --version` printed must be the requested release, so a
/// wrong or ambient executable cannot count as the installed one.
pub(super) fn check_reported_version(reported: &str, version: &str) -> Result<(), String> {
    let wanted = format!("ripr {version}");
    if reported.trim() == wanted {
        Ok(())
    } else {
        Err(format!(
            "the extracted executable reports `{}`, expected `{wanted}`",
            reported.trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;

    fn refused<T>(result: Result<T, String>) -> bool {
        matches!(result, Err(reason) if !reason.is_empty())
    }

    const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn tar_gz(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
        for (name, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, name, *data)
                .map_err(|err| err.to_string())?;
        }
        builder
            .into_inner()
            .and_then(GzEncoder::finish)
            .map_err(|err| err.to_string())
    }

    fn sha256_hex(bytes: &[u8]) -> Result<String, String> {
        let path = std::env::temp_dir().join(format!(
            "first-run-archive-hash-{}-{}",
            std::process::id(),
            bytes.len()
        ));
        fs::write(&path, bytes).map_err(|err| err.to_string())?;
        let digest = sha256_file(&path);
        let _ = fs::remove_file(&path);
        digest
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("first-run-archive-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// Serves `archive` and a sidecar from memory, like a release page would.
    fn serve<'a>(
        archive: &'a [u8],
        sidecar: String,
    ) -> impl Fn(&str, &Path) -> Result<(), String> + 'a {
        move |url: &str, dest: &Path| {
            if url.ends_with(".sha256") {
                fs::write(dest, &sidecar).map_err(|err| err.to_string())
            } else if url.contains("/v0.10.0/ripr-server-v0.10.0-x86_64-unknown-linux-gnu.tar.gz") {
                fs::write(dest, archive).map_err(|err| err.to_string())
            } else {
                Err(format!("404 for {url}"))
            }
        }
    }

    const TARGET: &str = "x86_64-unknown-linux-gnu";
    const ASSET: &str = "ripr-server-v0.10.0-x86_64-unknown-linux-gnu.tar.gz";

    #[test]
    fn hosts_map_to_released_targets_and_unknown_hosts_are_refused() -> Result<(), String> {
        assert_eq!(target_for("linux", "x86_64")?, "x86_64-unknown-linux-gnu");
        assert_eq!(target_for("macos", "aarch64")?, "aarch64-apple-darwin");
        assert_eq!(target_for("windows", "x86_64")?, "x86_64-pc-windows-msvc");
        assert!(refused(target_for("windows", "aarch64")));
        assert!(refused(target_for("freebsd", "x86_64")));
        assert_eq!(executable_name("x86_64-pc-windows-msvc"), "ripr.exe");
        Ok(())
    }

    #[test]
    fn a_sidecar_must_be_one_digest_for_this_asset() -> Result<(), String> {
        assert_eq!(parse_sidecar(&format!("{DIGEST}\n"), ASSET)?, DIGEST);
        assert_eq!(
            parse_sidecar(&format!("{}  {ASSET}\n", DIGEST.to_uppercase()), ASSET)?,
            DIGEST
        );
        assert!(refused(parse_sidecar("", ASSET)));
        assert!(refused(parse_sidecar("<html>Not Found</html>", ASSET)));
        assert!(refused(parse_sidecar(
            &format!("{DIGEST}  other.tar.gz"),
            ASSET
        )));
        assert!(refused(parse_sidecar(
            &format!("{DIGEST}\n{DIGEST}\n"),
            ASSET
        )));
        assert!(refused(parse_sidecar(&DIGEST[..63], ASSET)));
        Ok(())
    }

    #[test]
    fn a_verified_archive_yields_the_root_executable_with_its_identity() -> Result<(), String> {
        let archive = tar_gz(&[("./README-server.txt", b"hi"), ("./ripr", b"#!fake ripr")])?;
        let sidecar = format!("{}  {ASSET}\n", sha256_hex(&archive)?);
        let work = scratch("ok");
        let installed = install("0.10.0", TARGET, &work, &serve(&archive, sidecar))?;
        assert_eq!(installed.executable, work.join("bin").join("ripr"));
        assert_eq!(
            fs::read(&installed.executable).map_err(|e| e.to_string())?,
            b"#!fake ripr"
        );
        assert_eq!(installed.asset, ASSET);
        assert_eq!(installed.sha256, sha256_hex(&archive)?);
        assert_eq!(installed.executable_bytes, 11);
        let _ = fs::remove_dir_all(&work);
        Ok(())
    }

    #[test]
    fn a_corrupt_checksum_or_missing_asset_installs_nothing() -> Result<(), String> {
        let archive = tar_gz(&[("ripr", b"#!fake ripr")])?;
        let work = scratch("corrupt");
        let wrong = format!("{DIGEST}  {ASSET}\n");
        let err = install("0.10.0", TARGET, &work, &serve(&archive, wrong))
            .err()
            .ok_or("a wrong digest was accepted")?;
        assert!(err.contains("checksum mismatch"), "{err}");
        assert!(
            !work.join("bin").exists(),
            "nothing is extracted before verification"
        );

        let err = install("0.9.9", TARGET, &work, &serve(&archive, String::new()))
            .err()
            .ok_or("a missing asset was accepted")?;
        assert!(err.contains("404"), "{err}");

        // The sidecar of another release's asset cannot validate this one.
        let other = format!(
            "{}  ripr-server-v0.9.0-{TARGET}.tar.gz\n",
            sha256_hex(&archive)?
        );
        let err = install("0.10.0", TARGET, &work, &serve(&archive, other))
            .err()
            .ok_or("another asset's sidecar was accepted")?;
        assert!(err.contains("not the downloaded"), "{err}");
        let _ = fs::remove_dir_all(&work);
        Ok(())
    }

    #[test]
    fn an_archive_without_a_root_executable_is_refused_and_paths_are_not_followed()
    -> Result<(), String> {
        let archive = tar_gz(&[("nested/ripr", b"x"), ("../ripr", b"y")]);
        // `tar::Builder` refuses `..`; the nested layout alone must not match.
        let archive = match archive {
            Ok(bytes) => bytes,
            Err(_) => tar_gz(&[("nested/ripr", b"x")])?,
        };
        let sidecar = format!("{}  {ASSET}\n", sha256_hex(&archive)?);
        let work = scratch("layout");
        let err = install("0.10.0", TARGET, &work, &serve(&archive, sidecar))
            .err()
            .ok_or("a nested executable was accepted")?;
        assert!(err.contains("no root `ripr` entry"), "{err}");
        let _ = fs::remove_dir_all(&work);
        Ok(())
    }

    #[test]
    fn a_link_named_ripr_is_not_the_executable() -> Result<(), String> {
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o755);
        builder
            .append_link(&mut header, "./ripr", "/bin/sh")
            .map_err(|err| err.to_string())?;
        let archive = builder
            .into_inner()
            .and_then(GzEncoder::finish)
            .map_err(|err| err.to_string())?;
        let sidecar = format!("{}  {ASSET}\n", sha256_hex(&archive)?);
        let work = scratch("link");
        let err = install("0.10.0", TARGET, &work, &serve(&archive, sidecar))
            .err()
            .ok_or("a symlink was accepted as the executable")?;
        assert!(err.contains("no root `ripr` entry"), "{err}");
        let _ = fs::remove_dir_all(&work);
        Ok(())
    }

    #[test]
    fn only_the_requested_version_string_counts_as_the_installed_executable() {
        assert_eq!(check_reported_version("ripr 0.10.0\n", "0.10.0"), Ok(()));
        assert!(refused(check_reported_version("ripr 0.11.0-dev", "0.10.0")));
        assert!(refused(check_reported_version("ripr 0.10.0-rc1", "0.10.0")));
        assert!(refused(check_reported_version("", "0.10.0")));
    }
}
