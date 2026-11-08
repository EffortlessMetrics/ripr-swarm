//! Produce the whole no-publish package family (one npm native package and one
//! Python wheel per policy target) from the artifacts of one
//! server-archive qualification run. Every target must succeed; a missing,
//! ambiguous or failing target fails the command instead of shipping a subset.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;

use super::{npm_native, wheel_native};
use crate::policy::distribution::{TargetContract, load_distribution_contract};

/// Largest single payload file accepted from an archive (the executable is
/// tens of megabytes; this bounds a hostile or corrupt archive).
const MAX_PAYLOAD_FILE_BYTES: u64 = 512 * 1024 * 1024;

struct TargetInputs {
    archive: PathBuf,
    payload_identity: PathBuf,
    producer_receipt: PathBuf,
    build_identity: PathBuf,
}

/// Find exactly one regular file named `name` under `root`.
fn find_unique(root: &Path, name: &str) -> Result<PathBuf, String> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
            let path = entry
                .map_err(|e| format!("read entry under {}: {e}", dir.display()))?
                .path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|e| format!("stat {}: {e}", path.display()))?;
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() && path.file_name().and_then(|n| n.to_str()) == Some(name)
            {
                found.push(path);
            }
        }
    }
    match found.len() {
        1 => found.pop().ok_or_else(|| format!("{name} vanished")),
        0 => Err(format!("{} has no {name}", root.display())),
        count => Err(format!("{} has {count} files named {name}", root.display())),
    }
}

fn target_inputs(
    artifacts: &Path,
    version: &str,
    target: &TargetContract,
) -> Result<TargetInputs, String> {
    let rust_target = &target.rust_target;
    let payload_root = artifacts.join(format!("ripr-final-native-payload-{rust_target}"));
    Ok(TargetInputs {
        archive: find_unique(
            &artifacts.join(format!("ripr-server-qualification-{rust_target}")),
            &format!("ripr-server-v{version}-{rust_target}.{}", target.archive),
        )?,
        payload_identity: find_unique(&payload_root, "payload-identity.json")?,
        producer_receipt: find_unique(&payload_root, "producer-build.json")?,
        build_identity: find_unique(
            &artifacts.join(format!("ripr-server-build-identity-{rust_target}")),
            &format!("{rust_target}.json"),
        )?,
    })
}

/// A flat archive member name: one normal path component, no separators.
fn flat_member(name: &str) -> Result<&str, String> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', ':'])
        || name.starts_with('.')
    {
        return Err(format!(
            "archive member `{name}` is not a flat payload file"
        ));
    }
    Ok(name)
}

fn write_member(
    dir: &Path,
    seen: &mut BTreeSet<String>,
    name: &str,
    size: u64,
    reader: &mut dyn Read,
) -> Result<PathBuf, String> {
    let name = flat_member(name.strip_prefix("./").unwrap_or(name))?;
    if size > MAX_PAYLOAD_FILE_BYTES {
        return Err(format!(
            "archive member `{name}` is {size} bytes, over the limit"
        ));
    }
    if !seen.insert(name.to_string()) {
        return Err(format!("archive member `{name}` appears twice"));
    }
    let path = dir.join(name);
    let mut out = File::options()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("create {}: {e}", path.display()))?;
    let copied = std::io::copy(&mut reader.take(MAX_PAYLOAD_FILE_BYTES + 1), &mut out)
        .map_err(|e| format!("extract {name}: {e}"))?;
    if copied != size {
        return Err(format!(
            "archive member `{name}` declared {size} bytes, read {copied}"
        ));
    }
    Ok(path)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777))
        .map_err(|e| format!("chmod {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<(), String> {
    Ok(())
}

/// Extract a flat `.tar.gz` or `.zip` server archive into an empty directory,
/// keeping tar file modes so the executable bit is checked downstream.
pub(super) fn extract_flat_archive(archive: &Path, kind: &str, dir: &Path) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    let file = File::open(archive).map_err(|e| format!("open {}: {e}", archive.display()))?;
    match kind {
        "tar.gz" => {
            let mut tar = tar::Archive::new(GzDecoder::new(file));
            for entry in tar
                .entries()
                .map_err(|e| format!("read tar entries: {e}"))?
            {
                let mut entry = entry.map_err(|e| format!("read tar entry: {e}"))?;
                let name = entry
                    .path()
                    .map_err(|e| format!("read tar path: {e}"))?
                    .to_string_lossy()
                    .into_owned();
                if entry.header().entry_type().is_dir() && (name == "./" || name == ".") {
                    continue;
                }
                if !entry.header().entry_type().is_file() {
                    return Err(format!("archive member `{name}` is not a regular file"));
                }
                let size = entry
                    .header()
                    .size()
                    .map_err(|e| format!("size {name}: {e}"))?;
                let mode = entry
                    .header()
                    .mode()
                    .map_err(|e| format!("mode {name}: {e}"))?;
                let path = write_member(dir, &mut seen, &name, size, &mut entry)?;
                set_mode(&path, mode)?;
            }
        }
        "zip" => {
            let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("read zip: {e}"))?;
            for index in 0..zip.len() {
                let mut entry = zip
                    .by_index(index)
                    .map_err(|e| format!("read zip entry {index}: {e}"))?;
                let name = entry
                    .name()
                    .map_err(|e| format!("read zip entry {index} name: {e}"))?
                    .to_string();
                if !entry.is_file() {
                    return Err(format!("archive member `{name}` is not a regular file"));
                }
                let size = entry.size();
                write_member(dir, &mut seen, &name, size, &mut entry)?;
            }
        }
        other => return Err(format!("unsupported archive kind `{other}`")),
    }
    if seen.is_empty() {
        return Err(format!("{} is empty", archive.display()));
    }
    Ok(())
}

pub(super) fn produce_family(artifacts: &Path, out_dir: &Path) -> Result<(), String> {
    let contract = load_distribution_contract()?;
    let version = super::payload::workspace_version()?;
    let staging = std::env::temp_dir().join(format!(
        "ripr-package-family-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    fs::create_dir(&staging).map_err(|e| format!("create {}: {e}", staging.display()))?;
    let npm_out = out_dir.join("npm");
    let wheel_out = out_dir.join("wheels");
    let mut failures = Vec::new();
    for target in &contract.target {
        let result = (|| {
            let inputs = target_inputs(artifacts, &version, target)?;
            let payload_dir = staging.join(&target.rust_target);
            fs::create_dir(&payload_dir)
                .map_err(|e| format!("create {}: {e}", payload_dir.display()))?;
            extract_flat_archive(&inputs.archive, &target.archive, &payload_dir)?;
            npm_native::produce(
                &payload_dir,
                &inputs.payload_identity,
                &inputs.producer_receipt,
                &inputs.build_identity,
                &inputs.archive,
                &npm_out,
            )?;
            wheel_native::produce(
                &payload_dir,
                &inputs.payload_identity,
                &inputs.producer_receipt,
                &inputs.build_identity,
                &inputs.archive,
                &wheel_out,
            )
        })();
        match result {
            Ok(()) => eprintln!("packaged {}", target.rust_target),
            Err(error) => failures.push(format!("{}: {error}", target.rust_target)),
        }
    }
    let cleanup =
        fs::remove_dir_all(&staging).map_err(|e| format!("remove {}: {e}", staging.display()));
    if !failures.is_empty() {
        return Err(format!(
            "{} of {} targets failed to package:\n- {}",
            failures.len(),
            contract.target.len(),
            failures.join("\n- ")
        ));
    }
    cleanup?;
    eprintln!(
        "packaged all {} targets into {}",
        contract.target.len(),
        out_dir.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, GzBuilder};
    use std::io::Write;

    fn temp(label: &str) -> Result<PathBuf, String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-packages-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        fs::create_dir(&dir).map_err(|e| e.to_string())?;
        Ok(dir)
    }

    fn tar_gz(path: &Path, members: &[(&str, &[u8], u32)]) -> Result<(), String> {
        let encoder = GzBuilder::new().write(
            File::create(path).map_err(|e| e.to_string())?,
            Compression::default(),
        );
        let mut tar = tar::Builder::new(encoder);
        for (name, bytes, mode) in members {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(*mode);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            tar.append_data(&mut header, name, *bytes)
                .map_err(|e| e.to_string())?;
        }
        tar.into_inner()
            .map_err(|e| e.to_string())?
            .finish()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn zip_file(path: &Path, members: &[(&str, &[u8])]) -> Result<(), String> {
        let mut zip = zip::ZipWriter::new(File::create(path).map_err(|e| e.to_string())?);
        for (name, bytes) in members {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .map_err(|e| e.to_string())?;
            zip.write_all(bytes).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn extracts_flat_tar_with_modes_and_flat_zip() -> Result<(), String> {
        let root = temp("extract")?;
        let tarball = root.join("a.tar.gz");
        tar_gz(
            &tarball,
            &[("./ripr", b"native", 0o755), ("LICENSE-MIT", b"mit", 0o644)],
        )?;
        let out = root.join("tar");
        fs::create_dir(&out).map_err(|e| e.to_string())?;
        extract_flat_archive(&tarball, "tar.gz", &out)?;
        assert_eq!(
            fs::read(out.join("ripr")).map_err(|e| e.to_string())?,
            b"native"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(out.join("ripr"))
                .map_err(|e| e.to_string())?
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        let zipped = root.join("a.zip");
        zip_file(&zipped, &[("ripr.exe", b"native"), ("LICENSE-MIT", b"mit")])?;
        let out = root.join("zip");
        fs::create_dir(&out).map_err(|e| e.to_string())?;
        extract_flat_archive(&zipped, "zip", &out)?;
        assert_eq!(
            fs::read(out.join("ripr.exe")).map_err(|e| e.to_string())?,
            b"native"
        );
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn rejects_nested_traversing_duplicate_and_unknown_members() -> Result<(), String> {
        let root = temp("reject")?;
        type Members<'a> = Vec<(&'a str, &'a [u8], u32)>;
        let cases: [(&str, Members); 3] = [
            ("nested", vec![("bin/ripr", b"x", 0o755)]),
            ("drive", vec![("c:ripr", b"x", 0o644)]),
            ("hidden", vec![(".ripr", b"x", 0o644)]),
        ];
        for (label, members) in cases {
            let tarball = root.join(format!("{label}.tar.gz"));
            tar_gz(&tarball, &members)?;
            let out = root.join(label);
            fs::create_dir(&out).map_err(|e| e.to_string())?;
            let Err(_) = extract_flat_archive(&tarball, "tar.gz", &out) else {
                return Err(format!("{label} archive accepted"));
            };
        }
        let duplicate = root.join("duplicate.zip");
        zip_file(&duplicate, &[("ripr.exe", b"a"), ("sub\\ripr.exe", b"b")])?;
        let out = root.join("duplicate");
        fs::create_dir(&out).map_err(|e| e.to_string())?;
        let Err(_) = extract_flat_archive(&duplicate, "zip", &out) else {
            return Err("backslash-nested zip member accepted".to_string());
        };
        let empty = root.join("empty.tar.gz");
        tar_gz(&empty, &[])?;
        let out = root.join("empty");
        fs::create_dir(&out).map_err(|e| e.to_string())?;
        let Err(_) = extract_flat_archive(&empty, "tar.gz", &out) else {
            return Err("empty archive accepted".to_string());
        };
        let Err(_) = extract_flat_archive(&empty, "rar", &out) else {
            return Err("unknown archive kind accepted".to_string());
        };
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn missing_or_ambiguous_target_inputs_fail() -> Result<(), String> {
        let root = temp("inputs")?;
        let target = TargetContract {
            rust_target: "x86_64-unknown-linux-gnu".into(),
            executable: "ripr".into(),
            archive: "tar.gz".into(),
            wheel_family: "manylinux".into(),
            wheel_platform_tag: "manylinux_2_34_x86_64".into(),
            npm_package: "@effortlessmetrics/ripr-linux-x64-gnu".into(),
            npm_os: "linux".into(),
            npm_cpu: "x64".into(),
            npm_libc: Some("glibc".into()),
            compatibility_state: "unqualified".into(),
            qualification_issue: 4489,
        };
        let Err(_) = target_inputs(&root, "0.11.0", &target) else {
            return Err("missing artifacts accepted".to_string());
        };
        let archive_dir = root.join("ripr-server-qualification-x86_64-unknown-linux-gnu");
        let payload_dir = root.join("ripr-final-native-payload-x86_64-unknown-linux-gnu");
        let build_dir = root.join("ripr-server-build-identity-x86_64-unknown-linux-gnu");
        for dir in [&archive_dir, &payload_dir, &build_dir] {
            fs::create_dir_all(dir.join("v0.11.0")).map_err(|e| e.to_string())?;
        }
        let archive_name = "ripr-server-v0.11.0-x86_64-unknown-linux-gnu.tar.gz";
        fs::write(archive_dir.join(archive_name), b"a").map_err(|e| e.to_string())?;
        fs::write(payload_dir.join("payload-identity.json"), b"{}").map_err(|e| e.to_string())?;
        fs::write(payload_dir.join("producer-build.json"), b"{}").map_err(|e| e.to_string())?;
        fs::write(build_dir.join("x86_64-unknown-linux-gnu.json"), b"{}")
            .map_err(|e| e.to_string())?;
        let found = target_inputs(&root, "0.11.0", &target)?;
        assert_eq!(found.archive, archive_dir.join(archive_name));
        fs::write(
            payload_dir.join("v0.11.0").join("payload-identity.json"),
            b"{}",
        )
        .map_err(|e| e.to_string())?;
        let Err(_) = target_inputs(&root, "0.11.0", &target) else {
            return Err("ambiguous payload identity accepted".to_string());
        };
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
    }
}
