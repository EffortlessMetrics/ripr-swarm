use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::{Compression, GzBuilder, read::GzDecoder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{hex_lower, payload, producer, sha256_file};
use crate::policy::distribution::{DistributionContract, load_distribution_contract};

const TARGET: &str = "x86_64-unknown-linux-gnu";

fn verify_extracted_file(
    bytes: &[u8],
    mode: u32,
    expected_sha256: &str,
    executable: bool,
) -> Result<(), String> {
    let actual = hex_lower(&Sha256::digest(bytes));
    if actual != expected_sha256 {
        return Err(format!(
            "payload SHA-256 mismatch: expected {expected_sha256}, got {actual}"
        ));
    }
    if executable && mode & 0o111 == 0 {
        return Err(format!(
            "native executable mode {mode:o} has no execute bit"
        ));
    }
    Ok(())
}

fn package_path(identity: &payload::FinalNativePayloadIdentity, path: &str) -> String {
    if path == identity.executable {
        format!("bin/{path}")
    } else {
        path.to_owned()
    }
}

fn verify_notices(
    identity: &payload::FinalNativePayloadIdentity,
    source_root: &Path,
) -> Result<(), String> {
    for name in ["LICENSE-MIT", "LICENSE-APACHE"] {
        let item = identity
            .files
            .iter()
            .find(|item| item.path == name)
            .ok_or_else(|| format!("payload missing {name}"))?;
        let source_hash = sha256_file(&source_root.join(name))?;
        if item.sha256 != source_hash {
            return Err(format!(
                "payload {name} does not match the committed source license"
            ));
        }
    }
    let notice = identity
        .files
        .iter()
        .find(|item| item.path == "README-server.txt")
        .ok_or("payload missing README-server.txt")?;
    let expected = hex_lower(&Sha256::digest(
        super::release_server_readme(&identity.version).as_bytes(),
    ));
    if notice.sha256 != expected {
        return Err("payload notice does not match the versioned source notice".to_string());
    }
    Ok(())
}

fn manifest(
    identity: &payload::FinalNativePayloadIdentity,
    contract: &DistributionContract,
) -> Result<Value, String> {
    if identity.target != TARGET || identity.executable != "ripr" {
        return Err(format!(
            "native npm producer only admits {TARGET}/ripr, got {}/{}",
            identity.target, identity.executable
        ));
    }
    let target = contract
        .target
        .iter()
        .find(|row| row.rust_target == TARGET)
        .ok_or_else(|| format!("missing {TARGET} distribution policy"))?;
    if target.npm_package != "@effortlessmetrics/ripr-linux-x64-gnu"
        || target.npm_os != "linux"
        || target.npm_cpu != "x64"
        || target.npm_libc.as_deref() != Some("glibc")
    {
        return Err("Linux x64 npm metadata does not match the launcher contract".to_string());
    }
    let files: Vec<String> = identity
        .files
        .iter()
        .map(|file| package_path(identity, &file.path))
        .collect();
    Ok(json!({
        "name": target.npm_package,
        "version": identity.version,
        "description": "Native ripr executable for Linux x64 glibc",
        "license": "MIT OR Apache-2.0",
        "repository": {"type": "git", "url": "https://github.com/EffortlessMetrics/ripr.git"},
        "os": [target.npm_os], "cpu": [target.npm_cpu], "libc": [target.npm_libc],
        "files": files,
        "exports": {"./package.json": "./package.json"},
        "publishConfig": {"access": "public"},
        "riprNative": {"schemaVersion": 1, "product": "ripr", "target": TARGET, "executable": "bin/ripr"}
    }))
}

fn append(
    tar: &mut tar::Builder<flate2::write::GzEncoder<File>>,
    path: &str,
    bytes: &[u8],
    mode: u32,
) -> Result<(), String> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(mode);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_cksum();
    tar.append_data(&mut header, path, bytes)
        .map_err(|e| format!("pack {path}: {e}"))
}

#[cfg(unix)]
fn require_source_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)
        .map_err(|e| format!("stat {}: {e}", path.display()))?
        .permissions()
        .mode();
    if mode & 0o111 == 0 {
        return Err(format!(
            "source executable {} has no execute bit",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn require_source_executable(_path: &Path) -> Result<(), String> {
    Err("Linux native package mode must be verified on a Unix host".to_string())
}

fn pack(
    payload_dir: &Path,
    identity: &payload::FinalNativePayloadIdentity,
    contract: &DistributionContract,
    path: &Path,
) -> Result<(), String> {
    let mut metadata =
        serde_json::to_vec_pretty(&manifest(identity, contract)?).map_err(|e| e.to_string())?;
    metadata.push(b'\n');
    require_source_executable(&payload_dir.join(&identity.executable))?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("create {}: {e}", path.display()))?;
    let encoder = GzBuilder::new()
        .mtime(0)
        .write(file, Compression::default());
    let mut tar = tar::Builder::new(encoder);
    append(&mut tar, "package/package.json", &metadata, 0o644)?;
    for item in &identity.files {
        let bytes = fs::read(payload_dir.join(&item.path))
            .map_err(|e| format!("read {}: {e}", item.path))?;
        let executable = item.path == identity.executable;
        verify_extracted_file(
            &bytes,
            if executable { 0o755 } else { 0o644 },
            &item.sha256,
            executable,
        )?;
        append(
            &mut tar,
            &format!("package/{}", package_path(identity, &item.path)),
            &bytes,
            if executable { 0o755 } else { 0o644 },
        )?;
    }
    let encoder = tar.into_inner().map_err(|e| format!("finish tar: {e}"))?;
    encoder.finish().map_err(|e| format!("finish gzip: {e}"))?;
    Ok(())
}

fn verify_package(
    path: &Path,
    identity: &payload::FinalNativePayloadIdentity,
    contract: &DistributionContract,
) -> Result<(String, Vec<u8>), String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let digest = hex_lower(&Sha256::digest(&bytes));
    let temp = std::env::temp_dir().join(format!(
        "ripr-npm-verify-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    fs::create_dir(&temp).map_err(|e| format!("create verification directory: {e}"))?;
    let result = verify_package_inner(&bytes, identity, contract, &temp);
    let cleanup =
        fs::remove_dir_all(&temp).map_err(|e| format!("remove verification directory: {e}"));
    result.and(cleanup)?;
    Ok((digest, bytes))
}

fn verify_package_inner(
    bytes: &[u8],
    identity: &payload::FinalNativePayloadIdentity,
    contract: &DistributionContract,
    temp: &Path,
) -> Result<(), String> {
    let mut archive = tar::Archive::new(GzDecoder::new(Cursor::new(bytes)));
    let mut seen = BTreeSet::new();
    let mut expected = BTreeSet::from(["package/package.json".to_string()]);
    expected.extend(
        identity
            .files
            .iter()
            .map(|item| format!("package/{}", package_path(identity, &item.path))),
    );
    for entry in archive
        .entries()
        .map_err(|e| format!("read tar entries: {e}"))?
    {
        let mut entry = entry.map_err(|e| format!("read tar entry: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("read tar path: {e}"))?
            .to_string_lossy()
            .into_owned();
        if !expected.contains(&path)
            || !seen.insert(path.clone())
            || !entry.header().entry_type().is_file()
        {
            return Err(format!(
                "unexpected, duplicate or non-regular npm package entry: {path}"
            ));
        }
        let mode = entry
            .header()
            .mode()
            .map_err(|e| format!("entry mode {path}: {e}"))?;
        let expected_size = if path == "package/package.json" {
            65_536
        } else {
            identity
                .files
                .iter()
                .find(|item| format!("package/{}", package_path(identity, &item.path)) == path)
                .ok_or_else(|| format!("unknown payload {path}"))?
                .size_bytes
        };
        let size = entry
            .header()
            .size()
            .map_err(|e| format!("entry size {path}: {e}"))?;
        if (path == "package/package.json" && size > expected_size)
            || (path != "package/package.json" && size != expected_size)
        {
            return Err(format!("unexpected entry size for {path}: {size}"));
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| format!("read entry {path}: {e}"))?;
        if path == "package/package.json" {
            let actual: Value = serde_json::from_slice(&bytes)
                .map_err(|e| format!("invalid package metadata: {e}"))?;
            if actual != manifest(identity, contract)? {
                return Err("npm package metadata mismatch".to_string());
            }
        } else {
            let item = identity
                .files
                .iter()
                .find(|item| format!("package/{}", package_path(identity, &item.path)) == path)
                .ok_or_else(|| format!("unknown payload {path}"))?;
            verify_extracted_file(&bytes, mode, &item.sha256, item.path == identity.executable)?;
            fs::write(temp.join(&item.path), bytes)
                .map_err(|e| format!("write extracted payload {path}: {e}"))?;
        }
    }
    if seen != expected {
        return Err(format!(
            "npm inventory mismatch: expected {expected:?}, got {seen:?}"
        ));
    }
    payload::verify_payload_files(temp, identity)
}

fn ensure_archive_unchanged(path: &Path, verified_hash: &str) -> Result<(), String> {
    if sha256_file(path)? != verified_hash {
        return Err("npm tarball changed after verification".to_string());
    }
    Ok(())
}

fn verify_qualification(bytes: &[u8], expected: &Value) -> Result<(), String> {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let actual: Value = serde_json::from_slice(bytes)
        .map_err(|e| format!("parse selected-target build identity: {e}"))?;
    let fields = expected
        .as_object()
        .ok_or("invalid expected build identity")?;
    for (key, value) in fields {
        if actual.get(key) != Some(value) {
            return Err(format!("selected-target build identity mismatch: {key}"));
        }
    }
    Ok(())
}

fn promote_pair(
    package_bytes: &[u8],
    verified_hash: &str,
    receipt: &Path,
    output_package: &Path,
    output_receipt: &Path,
) -> Result<(), String> {
    let mut package = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(output_package)
        .map_err(|e| format!("create local package {}: {e}", output_package.display()))?;
    let write_result = (|| {
        package
            .write_all(package_bytes)
            .map_err(|e| format!("write local package: {e}"))?;
        package
            .seek(SeekFrom::Start(0))
            .map_err(|e| format!("seek local package: {e}"))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];
        loop {
            let count = package
                .read(&mut buffer)
                .map_err(|e| format!("read local package: {e}"))?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        if hex_lower(&hasher.finalize()) != verified_hash {
            return Err("promoted npm package does not match verified bytes".to_string());
        }
        Ok(())
    })();
    drop(package);
    if let Err(error) = write_result {
        fs::remove_file(output_package)
            .map_err(|e| format!("package write failed ({error}); remove partial package: {e}"))?;
        return Err(error);
    }
    if let Err(error) = fs::hard_link(receipt, output_receipt) {
        fs::remove_file(output_package).map_err(|e| {
            format!("receipt promotion failed ({error}); remove partial package: {e}")
        })?;
        return Err(format!(
            "publish local receipt {}: {error}",
            output_receipt.display()
        ));
    }
    Ok(())
}

pub(super) fn produce(
    payload_dir: &Path,
    identity_path: &Path,
    producer_path: &Path,
    build_identity_path: &Path,
    archive_path: &Path,
    out_dir: &Path,
) -> Result<(), String> {
    let identity_hash = sha256_file(identity_path)?;
    let identity = payload::verify_final_native_payload(payload_dir, identity_path)?;
    verify_notices(&identity, Path::new("."))?;
    let producer_bytes =
        fs::read(producer_path).map_err(|e| format!("read producer receipt: {e}"))?;
    let producer: producer::ProducerBuildReceipt = serde_json::from_slice(&producer_bytes)
        .map_err(|e| format!("parse producer receipt: {e}"))?;
    producer::validate_receipt_shape(&producer)?;
    producer::validate_version_line(
        &producer.executable_version_line,
        &producer.version,
        &producer.source_commit,
    )?;
    producer::bind_to_payload_identity(&producer, &identity)?;
    let producer_hash = hex_lower(&Sha256::digest(&producer_bytes));
    let build_bytes = fs::read(build_identity_path)
        .map_err(|e| format!("read selected-target build identity: {e}"))?;
    let archive_name = format!("ripr-server-v{}-{TARGET}.tar.gz", identity.version);
    if archive_path.file_name().and_then(|name| name.to_str()) != Some(archive_name.as_str()) {
        return Err(
            "qualified archive filename does not match the selected target/version".to_string(),
        );
    }
    let archive_hash = sha256_file(archive_path)?;
    let expected_qualification = json!({
        "target": TARGET, "candidate_sha": identity.source.commit_sha,
        "candidate_tree": identity.source.tree_sha, "version": identity.version,
        "archive_file": archive_name, "archive_sha256": archive_hash,
        "archive_readback_verified": true, "cargo_lock_sha256": identity.cargo_lock_sha256,
        "qualification_workflow_sha256": sha256_file(Path::new(".github/workflows/server-archive-qualification.yml"))?,
        "payload_identity_sha256": identity_hash, "producer_build_sha256": producer_hash,
        "payload_aggregate_sha256": identity.payload_aggregate_sha256,
        "payload_file_count": identity.files.len(), "selected_features": identity.selected_features,
        "native_runtime_evidence": identity.native_runtime_evidence,
        "producer_cargo_arguments": producer.cargo_arguments,
        "producer_executable_sha256": producer.executable_sha256,
        "producer_executable_version_line": producer.executable_version_line,
        "rustc_verbose_version": identity.toolchain.rustc_verbose_version,
        "cargo_verbose_version": identity.toolchain.cargo_verbose_version
    });
    verify_qualification(&build_bytes, &expected_qualification)?;
    let contract = load_distribution_contract()?;
    manifest(&identity, &contract)?;
    fs::create_dir_all(out_dir).map_err(|e| format!("create output directory: {e}"))?;
    let staging = out_dir.join(format!(
        ".npm-native-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    fs::create_dir(&staging).map_err(|e| format!("create npm staging directory: {e}"))?;
    let result = (|| {
        let package_name = format!(
            "effortlessmetrics-ripr-linux-x64-gnu-{}.tgz",
            identity.version
        );
        let package = staging.join(&package_name);
        pack(payload_dir, &identity, &contract, &package)?;
        let (verified_tarball_hash, verified_tarball_bytes) =
            verify_package(&package, &identity, &contract)?;
        if sha256_file(identity_path)? != identity_hash
            || payload::verify_final_native_payload(payload_dir, identity_path)? != identity
        {
            return Err("source payload or identity changed while packaging".to_string());
        }
        ensure_archive_unchanged(&package, &verified_tarball_hash)?;
        ensure_archive_unchanged(archive_path, &archive_hash)?;
        let receipt = json!({"schema_version": 1, "kind": "ripr_native_npm_package", "target": identity.target,
        "version": identity.version, "source_commit": identity.source.commit_sha,
        "source_tree": identity.source.tree_sha, "selected_features": identity.selected_features,
        "payload_aggregate_sha256": identity.payload_aggregate_sha256,
        "payload_identity_sha256": identity_hash, "producer_receipt_sha256": producer_hash,
        "build_identity_sha256": hex_lower(&Sha256::digest(&build_bytes)),
        "qualified_archive_sha256": archive_hash,
        "tarball": package.file_name().and_then(|s| s.to_str()).ok_or("invalid package file name")?,
        "tarball_sha256": verified_tarball_hash, "files": identity.files});
        let receipt_path = staging.join("npm-native-receipt.json");
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&receipt_path)
            .map_err(|e| format!("create {}: {e}", receipt_path.display()))?;
        writeln!(
            output,
            "{}",
            serde_json::to_string_pretty(&receipt).map_err(|e| e.to_string())?
        )
        .map_err(|e| e.to_string())?;
        let output_package = out_dir.join(&package_name);
        let output_receipt = out_dir.join("npm-native-receipt.json");
        promote_pair(
            &verified_tarball_bytes,
            &verified_tarball_hash,
            &receipt_path,
            &output_package,
            &output_receipt,
        )?;
        eprintln!(
            "wrote {} and {}",
            output_package.display(),
            output_receipt.display()
        );
        Ok(())
    })();
    let cleanup =
        fs::remove_dir_all(&staging).map_err(|e| format!("remove npm staging directory: {e}"));
    result.and(cleanup)
}

#[cfg(test)]
mod tests {
    use super::super::payload::{
        NativeRuntimeEvidenceIdentity, PayloadBuildEnvironmentIdentity, PayloadSourceIdentity,
        PayloadToolchainIdentity,
    };
    use super::*;
    use crate::policy::distribution::parse_distribution_contract;

    #[test]
    fn extracted_executable_requires_both_expected_bytes_and_executable_mode() -> Result<(), String>
    {
        let expected = hex_lower(&Sha256::digest(b"native"));
        verify_extracted_file(b"native", 0o755, &expected, true)?;
        let Err(_) = verify_extracted_file(b"wrong", 0o755, &expected, true) else {
            return Err("mismatched payload bytes accepted".to_string());
        };
        let Err(_) = verify_extracted_file(b"native", 0o644, &expected, true) else {
            return Err("missing execute bit accepted for executables".to_string());
        };
        verify_extracted_file(b"native", 0o644, &expected, false)?;
        Ok(())
    }

    #[test]
    fn qualification_requires_success_and_every_bound_field() -> Result<(), String> {
        let expected = json!({"archive_readback_verified": true, "target": TARGET,
            "version": "0.11.0", "producer_build_sha256": "producer", "archive_sha256": "archive"});
        let bytes = serde_json::to_vec(&expected).map_err(|e| e.to_string())?;
        verify_qualification(&bytes, &expected)?;
        for key in [
            "archive_readback_verified",
            "target",
            "version",
            "producer_build_sha256",
            "archive_sha256",
        ] {
            let mut wrong = expected.clone();
            wrong[key] = json!(null);
            let Err(_) = verify_qualification(
                &serde_json::to_vec(&wrong).map_err(|e| e.to_string())?,
                &expected,
            ) else {
                return Err(format!("accepted wrong {key}"));
            };
            wrong.as_object_mut().ok_or("expected object")?.remove(key);
            let Err(_) = verify_qualification(
                &serde_json::to_vec(&wrong).map_err(|e| e.to_string())?,
                &expected,
            ) else {
                return Err(format!("accepted missing {key}"));
            };
        }
        Ok(())
    }

    #[test]
    fn failed_receipt_promotion_removes_owned_package_and_allows_retry() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-npm-promotion-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        let source_receipt = root.join("source.json");
        let output_package = root.join("output.tgz");
        let output_receipt = root.join("output.json");
        let hash = hex_lower(&Sha256::digest(b"package"));
        fs::write(&source_receipt, b"receipt").map_err(|e| e.to_string())?;
        fs::write(&output_receipt, b"existing").map_err(|e| e.to_string())?;
        let Err(_) = promote_pair(
            b"package",
            &hash,
            &source_receipt,
            &output_package,
            &output_receipt,
        ) else {
            return Err("promotion over an existing receipt accepted".to_string());
        };
        assert!(!output_package.exists());
        assert_eq!(
            fs::read(&output_receipt).map_err(|e| e.to_string())?,
            b"existing"
        );
        fs::remove_file(&output_receipt).map_err(|e| e.to_string())?;
        promote_pair(
            b"package",
            &hash,
            &source_receipt,
            &output_package,
            &output_receipt,
        )?;
        assert_eq!(
            fs::read(&output_package).map_err(|e| e.to_string())?,
            b"package"
        );
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn package_maps_payload_and_rejects_metadata_bytes_and_mode_mutations() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-npm-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        let source = root.join("payload");
        fs::create_dir(&source).map_err(|e| e.to_string())?;
        for (path, bytes) in [
            ("ripr", b"native".as_slice()),
            ("LICENSE-MIT", b"mit"),
            ("LICENSE-APACHE", b"apache"),
            (
                "README-server.txt",
                super::super::release_server_readme("0.11.0").as_bytes(),
            ),
        ] {
            fs::write(source.join(path), bytes).map_err(|e| e.to_string())?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(source.join("ripr"), fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
        let files = payload::payload_file_identities(&source, "ripr")?;
        let identity = payload::FinalNativePayloadIdentity {
            schema_version: 1,
            kind: "ripr_final_native_payload".into(),
            product: "ripr".into(),
            version: "0.11.0".into(),
            target: TARGET.into(),
            executable: "ripr".into(),
            source: PayloadSourceIdentity {
                commit_sha: "a".repeat(40),
                tree_sha: "b".repeat(40),
            },
            cargo_lock_sha256: "c".repeat(64),
            selected_features: vec![],
            toolchain: PayloadToolchainIdentity {
                rustc_verbose_version: "rust".into(),
                cargo_verbose_version: "cargo".into(),
            },
            build_environment: PayloadBuildEnvironmentIdentity {
                os: "linux".into(),
                arch: "x64".into(),
                ci_provider: "test".into(),
                runner_image: None,
            },
            payload_aggregate_sha256: payload::aggregate_payload_digest(&files)?,
            files,
            native_runtime_evidence: NativeRuntimeEvidenceIdentity {
                state: "test".into(),
                source: "test".into(),
            },
        };
        let contract = parse_distribution_contract(
            "policy",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../policy/distribution.toml"
            )),
        )?;
        let original = root.join("original.tgz");
        verify_notices(&identity, &source)?;
        let mut wrong_license = identity.clone();
        let license = wrong_license
            .files
            .iter_mut()
            .find(|item| item.path == "LICENSE-MIT")
            .ok_or("license missing")?;
        license.sha256 = "0".repeat(64);
        let Err(_) = verify_notices(&wrong_license, &source) else {
            return Err("tampered license digest accepted".to_string());
        };
        let mut wrong_notice = identity.clone();
        let notice = wrong_notice
            .files
            .iter_mut()
            .find(|item| item.path == "README-server.txt")
            .ok_or("notice missing")?;
        notice.sha256 = "0".repeat(64);
        let Err(_) = verify_notices(&wrong_notice, &source) else {
            return Err("tampered notice digest accepted".to_string());
        };
        pack(&source, &identity, &contract, &original)?;
        verify_package(&original, &identity, &contract)?;
        let repeated = root.join("repeat.tgz");
        pack(&source, &identity, &contract, &repeated)?;
        assert_eq!(sha256_file(&original)?, sha256_file(&repeated)?);
        let (verified, snapshot) = verify_package(&repeated, &identity, &contract)?;
        ensure_archive_unchanged(&repeated, &verified)?;
        fs::write(&repeated, b"replacement").map_err(|e| e.to_string())?;
        let Err(_) = ensure_archive_unchanged(&repeated, &verified) else {
            return Err("archive changed after verification accepted".to_string());
        };
        let staged_receipt = root.join("staged.json");
        fs::write(&staged_receipt, b"receipt").map_err(|e| e.to_string())?;
        let promoted_package = root.join("promoted.tgz");
        let promoted_receipt = root.join("promoted.json");
        promote_pair(
            &snapshot,
            &verified,
            &staged_receipt,
            &promoted_package,
            &promoted_receipt,
        )?;
        assert_eq!(sha256_file(&promoted_package)?, verified);
        for mutation in [
            "binary", "mode", "license", "version", "platform", "missing",
        ] {
            let changed = root.join(format!("{mutation}.tgz"));
            mutate(&original, &changed, mutation)?;
            let Err(_) = verify_package(&changed, &identity, &contract) else {
                return Err(format!("accepted {mutation}"));
            };
        }
        let mut wrong_target = identity.clone();
        wrong_target.target = "aarch64-unknown-linux-gnu".into();
        let Err(_) = manifest(&wrong_target, &contract) else {
            return Err("manifest for an uncontracted target accepted".to_string());
        };
        let mut wrong_policy = contract.clone();
        let linux = wrong_policy
            .target
            .iter_mut()
            .find(|item| item.rust_target == TARGET)
            .ok_or("Linux target missing")?;
        linux.npm_cpu = "arm64".into();
        let Err(_) = manifest(&identity, &wrong_policy) else {
            return Err("manifest with a mismatched npm cpu accepted".to_string());
        };
        let mut wrong_hash = identity.clone();
        wrong_hash.files[0].sha256 = "0".repeat(64);
        let Err(_) = verify_package(&original, &wrong_hash, &contract) else {
            return Err("package with a tampered payload digest accepted".to_string());
        };
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(source.join("ripr"), fs::Permissions::from_mode(0o644))
            .map_err(|e| e.to_string())?;
        let Err(_) = pack(&source, &identity, &contract, &root.join("no-mode.tgz")) else {
            return Err("payload without the execute bit accepted".to_string());
        };
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn mutate(input: &Path, output: &Path, mutation: &str) -> Result<(), String> {
        let mut source = tar::Archive::new(GzDecoder::new(
            File::open(input).map_err(|e| e.to_string())?,
        ));
        let writer = GzBuilder::new().mtime(0).write(
            File::create(output).map_err(|e| e.to_string())?,
            Compression::default(),
        );
        let mut out = tar::Builder::new(writer);
        for entry in source.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            let path = entry
                .path()
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .into_owned();
            let mut mode = entry.header().mode().map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            if mutation == "missing" && path == "package/LICENSE-MIT" {
                continue;
            }
            if mutation == "binary" && path == "package/bin/ripr" {
                bytes = b"wrong!".to_vec();
            }
            if mutation == "mode" && path == "package/bin/ripr" {
                mode = 0o644;
            }
            if mutation == "license" && path == "package/LICENSE-MIT" {
                bytes = b"bad".to_vec();
            }
            if path == "package/package.json" && (mutation == "version" || mutation == "platform") {
                let mut value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                if mutation == "version" {
                    value["version"] = json!("0.0.0");
                } else {
                    value["os"] = json!(["darwin"]);
                }
                bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
            }
            append(&mut out, &path, &bytes, mode)?;
        }
        out.into_inner()
            .map_err(|e| e.to_string())?
            .finish()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
