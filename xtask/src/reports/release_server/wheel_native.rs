//! No-publish producer that wraps one target's verified native release payload
//! into a platform-tagged `ripr-rs` Python wheel (#5378, #4490). The native
//! executable is never rebuilt: the wheel carries the payload bytes whose
//! digests the final payload identity already binds.

use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::npm_native::{self, promote_pair, verify_notices, verify_qualification};
use super::{hex_lower, payload, producer, sha256_file};
use crate::policy::distribution::{
    DistributionContract, load_distribution_contract, pep440_version,
};

const PYPROJECT_PATH: &str = "packaging/python/pyproject.toml";
const README_PATH: &str = "packaging/python/README.md";
const LICENSE_FILES: [&str; 2] = ["LICENSE-APACHE", "LICENSE-MIT"];
const SCRIPT_MODE: u32 = 0o100_755;
const FILE_MODE: u32 = 0o100_644;

/// Python packaging sources the wheel metadata is generated from.
struct PythonSources {
    pyproject: String,
    readme: String,
}

fn load_python_sources() -> Result<PythonSources, String> {
    Ok(PythonSources {
        pyproject: fs::read_to_string(PYPROJECT_PATH)
            .map_err(|e| format!("read {PYPROJECT_PATH}: {e}"))?,
        readme: fs::read_to_string(README_PATH).map_err(|e| format!("read {README_PATH}: {e}"))?,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Entry {
    name: String,
    bytes: Vec<u8>,
    mode: u32,
}

fn urlsafe_b64_nopad(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk.first().copied().unwrap_or(0));
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        for index in 0..=chunk.len() {
            let shift = 18 - 6 * index as u32;
            out.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
        }
    }
    out
}

fn record_line(name: &str, bytes: &[u8]) -> String {
    format!(
        "{name},sha256={},{}",
        urlsafe_b64_nopad(&Sha256::digest(bytes)),
        bytes.len()
    )
}

/// RECORD text covering `entries` (everything that precedes RECORD) followed
/// by RECORD's own hashless line.
fn record_bytes(entries: &[Entry], record_name: &str) -> Vec<u8> {
    let mut text = String::new();
    for entry in entries {
        text.push_str(&record_line(&entry.name, &entry.bytes));
        text.push('\n');
    }
    text.push_str(&format!("{record_name},,\n"));
    text.into_bytes()
}

fn single_line(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.contains(['\n', '\r']) {
        return Err(format!(
            "{PYPROJECT_PATH}: {field} must be a non-empty single line"
        ));
    }
    Ok(())
}

fn string_field<'a>(table: &'a toml::Table, key: &str) -> Result<&'a str, String> {
    let value = table
        .get(key)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("{PYPROJECT_PATH}: {key} must be a string"))?;
    single_line(key, value)?;
    Ok(value)
}

fn string_list(table: &toml::Table, key: &str) -> Result<Vec<String>, String> {
    let items = table
        .get(key)
        .and_then(toml::Value::as_array)
        .ok_or_else(|| format!("{PYPROJECT_PATH}: project.{key} must be an array"))?;
    items
        .iter()
        .map(|item| {
            let text = item
                .as_str()
                .ok_or_else(|| format!("{PYPROJECT_PATH}: project.{key} must hold strings"))?;
            single_line(key, text)?;
            Ok(text.to_owned())
        })
        .collect()
}

fn metadata_text(
    contract: &DistributionContract,
    sources: &PythonSources,
    python_version: &str,
) -> Result<String, String> {
    let root: toml::Table =
        toml::from_str(&sources.pyproject).map_err(|e| format!("parse {PYPROJECT_PATH}: {e}"))?;
    let project = root
        .get("project")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("{PYPROJECT_PATH}: missing [project]"))?;
    let name = string_field(project, "name")?;
    if name != contract.python.distribution {
        return Err(format!(
            "{PYPROJECT_PATH}: project.name `{name}` does not match contract distribution `{}`",
            contract.python.distribution
        ));
    }
    if string_list(project, "dynamic")? != ["version"] || project.contains_key("version") {
        return Err(format!(
            "{PYPROJECT_PATH}: version must be dynamic and supplied by the producer"
        ));
    }
    let license_files = string_list(project, "license-files")?;
    if license_files != LICENSE_FILES {
        return Err(format!(
            "{PYPROJECT_PATH}: license-files must be exactly {LICENSE_FILES:?}"
        ));
    }
    let readme = project
        .get("readme")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("{PYPROJECT_PATH}: project.readme must be a table"))?;
    if readme.get("file").and_then(toml::Value::as_str) != Some("README.md") {
        return Err(format!(
            "{PYPROJECT_PATH}: project.readme.file must be README.md"
        ));
    }
    let content_type = string_field(readme, "content-type")?;
    let author = project
        .get("authors")
        .and_then(toml::Value::as_array)
        .and_then(|authors| authors.first())
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("{PYPROJECT_PATH}: project.authors[0] must be a table"))?;
    let author = string_field(author, "name")?;
    let mut text = String::from("Metadata-Version: 2.4\n");
    text.push_str(&format!("Name: {name}\nVersion: {python_version}\n"));
    text.push_str(&format!(
        "Summary: {}\n",
        string_field(project, "description")?
    ));
    text.push_str(&format!(
        "Keywords: {}\n",
        string_list(project, "keywords")?.join(",")
    ));
    text.push_str(&format!("Author: {author}\n"));
    text.push_str(&format!(
        "License-Expression: {}\n",
        string_field(project, "license")?
    ));
    for license in &license_files {
        text.push_str(&format!("License-File: {license}\n"));
    }
    for classifier in string_list(project, "classifiers")? {
        text.push_str(&format!("Classifier: {classifier}\n"));
    }
    text.push_str(&format!(
        "Requires-Python: {}\n",
        string_field(project, "requires-python")?
    ));
    let urls = project
        .get("urls")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("{PYPROJECT_PATH}: project.urls must be a table"))?;
    for (label, url) in urls {
        let url = url
            .as_str()
            .ok_or_else(|| format!("{PYPROJECT_PATH}: project.urls.{label} must be a string"))?;
        single_line(label, label)?;
        single_line(label, url)?;
        text.push_str(&format!("Project-URL: {label}, {url}\n"));
    }
    text.push_str(&format!("Description-Content-Type: {content_type}\n\n"));
    text.push_str(&sources.readme);
    Ok(text)
}

fn wheel_text(platform_tag: &str) -> String {
    format!(
        "Wheel-Version: 1.0\nGenerator: ripr-xtask\nRoot-Is-Purelib: false\nTag: py3-none-{platform_tag}\n"
    )
}

struct WheelPlan {
    archive: String,
    platform_tag: String,
    python_version: String,
    base: String,
    filename: String,
}

impl WheelPlan {
    fn dist_info(&self, leaf: &str) -> String {
        format!("{}.dist-info/{leaf}", self.base)
    }

    fn script_name(&self, executable: &str) -> String {
        format!("{}.data/scripts/{executable}", self.base)
    }

    fn record_name(&self) -> String {
        self.dist_info("RECORD")
    }

    /// The exact entry names, in the exact archive order.
    fn names(&self, executable: &str) -> Vec<String> {
        vec![
            self.script_name(executable),
            self.dist_info("METADATA"),
            self.dist_info("WHEEL"),
            self.dist_info("licenses/LICENSE-APACHE"),
            self.dist_info("licenses/LICENSE-MIT"),
            self.record_name(),
        ]
    }
}

fn plan(
    identity: &payload::FinalNativePayloadIdentity,
    contract: &DistributionContract,
) -> Result<WheelPlan, String> {
    let target = contract
        .target
        .iter()
        .find(|row| row.rust_target == identity.target)
        .ok_or_else(|| format!("missing {} distribution policy", identity.target))?;
    if target.executable != identity.executable
        || target
            .executable
            .strip_suffix(".exe")
            .unwrap_or(&target.executable)
            != contract.python.executable
    {
        return Err(format!(
            "payload executable `{}` does not match the {} wheel contract",
            identity.executable, identity.target
        ));
    }
    if !matches!(target.archive.as_str(), "tar.gz" | "zip") {
        return Err(format!(
            "unsupported archive `{}` in policy",
            target.archive
        ));
    }
    let tag = &target.wheel_platform_tag;
    if tag.is_empty() || !tag.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(format!("invalid wheel platform tag `{tag}`"));
    }
    let distribution = &contract.python.distribution;
    if distribution.is_empty()
        || !distribution
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(format!("invalid python distribution `{distribution}`"));
    }
    let python_version = pep440_version(&identity.version)?;
    let normalized = distribution.replace('-', "_");
    Ok(WheelPlan {
        archive: target.archive.clone(),
        platform_tag: tag.clone(),
        base: format!("{normalized}-{python_version}"),
        filename: format!("{normalized}-{python_version}-py3-none-{tag}.whl"),
        python_version,
    })
}

fn file_identity<'a>(
    identity: &'a payload::FinalNativePayloadIdentity,
    name: &str,
) -> Result<&'a payload::PayloadFileIdentity, String> {
    identity
        .files
        .iter()
        .find(|item| item.path == name)
        .ok_or_else(|| format!("payload identity missing {name}"))
}

fn check_payload_bytes(
    name: &str,
    bytes: &[u8],
    item: &payload::PayloadFileIdentity,
) -> Result<(), String> {
    let actual = hex_lower(&Sha256::digest(bytes));
    if actual != item.sha256 || bytes.len() as u64 != item.size_bytes {
        return Err(format!(
            "payload {name} does not match its identity: expected {}, got {actual}",
            item.sha256
        ));
    }
    Ok(())
}

fn read_payload_file(
    payload_dir: &Path,
    identity: &payload::FinalNativePayloadIdentity,
    name: &str,
) -> Result<Vec<u8>, String> {
    let item = file_identity(identity, name)?;
    let bytes = fs::read(payload_dir.join(name)).map_err(|e| format!("read {name}: {e}"))?;
    check_payload_bytes(name, &bytes, item)?;
    Ok(bytes)
}

fn build_entries(
    payload_dir: &Path,
    identity: &payload::FinalNativePayloadIdentity,
    contract: &DistributionContract,
    sources: &PythonSources,
) -> Result<(WheelPlan, Vec<Entry>), String> {
    let plan = plan(identity, contract)?;
    let metadata = metadata_text(contract, sources, &plan.python_version)?;
    let mut entries = vec![
        Entry {
            name: plan.script_name(&identity.executable),
            bytes: read_payload_file(payload_dir, identity, &identity.executable)?,
            mode: SCRIPT_MODE,
        },
        Entry {
            name: plan.dist_info("METADATA"),
            bytes: metadata.into_bytes(),
            mode: FILE_MODE,
        },
        Entry {
            name: plan.dist_info("WHEEL"),
            bytes: wheel_text(&plan.platform_tag).into_bytes(),
            mode: FILE_MODE,
        },
    ];
    for license in LICENSE_FILES {
        entries.push(Entry {
            name: plan.dist_info(&format!("licenses/{license}")),
            bytes: read_payload_file(payload_dir, identity, license)?,
            mode: FILE_MODE,
        });
    }
    let record = Entry {
        name: plan.record_name(),
        bytes: record_bytes(&entries, &plan.record_name()),
        mode: FILE_MODE,
    };
    entries.push(record);
    Ok((plan, entries))
}

fn write_wheel(entries: &[Entry], path: &Path) -> Result<(), String> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("create {}: {e}", path.display()))?;
    let mut writer = ZipWriter::new(file);
    for entry in entries {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(DateTime::default())
            .unix_permissions(entry.mode);
        writer
            .start_file(entry.name.as_str(), options)
            .map_err(|e| format!("start wheel entry {}: {e}", entry.name))?;
        writer
            .write_all(&entry.bytes)
            .map_err(|e| format!("write wheel entry {}: {e}", entry.name))?;
    }
    writer
        .finish()
        .map_err(|e| format!("finish wheel: {e}"))?
        .sync_all()
        .map_err(|e| format!("sync wheel: {e}"))
}

/// Reads every entry, rejecting directories, non-default timestamps and
/// non-deflate compression.
fn read_wheel(bytes: &[u8]) -> Result<Vec<Entry>, String> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("read wheel archive: {e}"))?;
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| format!("read wheel entry {index}: {e}"))?;
        let name = file
            .name()
            .map_err(|e| format!("read wheel entry {index} name: {e}"))?
            .into_owned();
        if !file.is_file() {
            return Err(format!("wheel entry {name} is not a regular file"));
        }
        if file.last_modified() != Some(DateTime::default()) {
            return Err(format!("wheel entry {name} has a non-fixed timestamp"));
        }
        if file.compression() != CompressionMethod::Deflated {
            return Err(format!("wheel entry {name} is not deflate-compressed"));
        }
        let mode = file
            .unix_mode()
            .ok_or_else(|| format!("wheel entry {name} has no unix mode"))?;
        let mut content = Vec::new();
        file.read_to_end(&mut content)
            .map_err(|e| format!("read wheel entry {name}: {e}"))?;
        entries.push(Entry {
            name,
            bytes: content,
            mode,
        });
    }
    Ok(entries)
}

fn verify_wheel(
    bytes: &[u8],
    identity: &payload::FinalNativePayloadIdentity,
    contract: &DistributionContract,
    sources: &PythonSources,
) -> Result<(), String> {
    let plan = plan(identity, contract)?;
    let entries = read_wheel(bytes)?;
    let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    let expected_names = plan.names(&identity.executable);
    if names != expected_names {
        return Err(format!(
            "wheel inventory mismatch: expected {expected_names:?}, got {names:?}"
        ));
    }
    let script = &plan.script_name(&identity.executable);
    for entry in &entries {
        let expected_mode = if &entry.name == script {
            SCRIPT_MODE
        } else {
            FILE_MODE
        };
        if entry.mode != expected_mode {
            return Err(format!(
                "wheel entry {} has mode {:o}, expected {expected_mode:o}",
                entry.name, entry.mode
            ));
        }
    }
    let (record, covered) = entries.split_last().ok_or("wheel has no entries")?;
    if record.bytes != record_bytes(covered, &record.name) {
        return Err("wheel RECORD does not match the archive contents".to_string());
    }
    let find = |name: &str| -> Result<&Entry, String> {
        entries
            .iter()
            .find(|entry| entry.name == name)
            .ok_or_else(|| format!("wheel missing {name}"))
    };
    let executable = &identity.executable;
    check_payload_bytes(
        executable,
        &find(script)?.bytes,
        file_identity(identity, executable)?,
    )?;
    for license in LICENSE_FILES {
        check_payload_bytes(
            license,
            &find(&plan.dist_info(&format!("licenses/{license}")))?.bytes,
            file_identity(identity, license)?,
        )?;
    }
    if find(&plan.dist_info("WHEEL"))?.bytes != wheel_text(&plan.platform_tag).into_bytes() {
        return Err("wheel WHEEL metadata mismatch".to_string());
    }
    let metadata = metadata_text(contract, sources, &plan.python_version)?;
    if find(&plan.dist_info("METADATA"))?.bytes != metadata.into_bytes() {
        return Err("wheel METADATA mismatch".to_string());
    }
    Ok(())
}

/// Packs the wheel into `path`, re-opens it, verifies it and returns the
/// verified bytes with their digest.
fn pack_and_verify(
    payload_dir: &Path,
    identity: &payload::FinalNativePayloadIdentity,
    contract: &DistributionContract,
    sources: &PythonSources,
    path: &Path,
) -> Result<(String, Vec<u8>), String> {
    let (_, entries) = build_entries(payload_dir, identity, contract, sources)?;
    write_wheel(&entries, path)?;
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    verify_wheel(&bytes, identity, contract, sources)?;
    Ok((hex_lower(&Sha256::digest(&bytes)), bytes))
}

fn ensure_unchanged(path: &Path, verified_hash: &str, what: &str) -> Result<(), String> {
    if sha256_file(path)? != verified_hash {
        return Err(format!("{what} changed after verification"));
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
    let contract = load_distribution_contract()?;
    let sources = load_python_sources()?;
    let wheel_plan = plan(&identity, &contract)?;
    let archive_name = format!(
        "ripr-server-v{}-{}.{}",
        identity.version, identity.target, wheel_plan.archive
    );
    if archive_path.file_name().and_then(|name| name.to_str()) != Some(archive_name.as_str()) {
        return Err(
            "qualified archive filename does not match the selected target/version".to_string(),
        );
    }
    let archive_hash = sha256_file(archive_path)?;
    let expected_qualification = npm_native::expected_qualification(
        &identity,
        &identity_hash,
        &producer,
        &producer_hash,
        &archive_name,
        &archive_hash,
    )?;
    verify_qualification(&build_bytes, &expected_qualification)?;
    fs::create_dir_all(out_dir).map_err(|e| format!("create output directory: {e}"))?;
    let staging = out_dir.join(format!(
        ".wheel-native-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    fs::create_dir(&staging).map_err(|e| format!("create wheel staging directory: {e}"))?;
    let result = (|| {
        let wheel = staging.join(&wheel_plan.filename);
        let (wheel_hash, wheel_bytes) =
            pack_and_verify(payload_dir, &identity, &contract, &sources, &wheel)?;
        if sha256_file(identity_path)? != identity_hash
            || payload::verify_final_native_payload(payload_dir, identity_path)? != identity
        {
            return Err("source payload or identity changed while packaging".to_string());
        }
        ensure_unchanged(&wheel, &wheel_hash, "wheel")?;
        ensure_unchanged(archive_path, &archive_hash, "qualified archive")?;
        let receipt = json!({"schema_version": 1, "kind": "ripr_native_python_wheel",
        "target": identity.target, "version": identity.version,
        "python_version": wheel_plan.python_version, "platform_tag": wheel_plan.platform_tag,
        "source_commit": identity.source.commit_sha, "source_tree": identity.source.tree_sha,
        "selected_features": identity.selected_features,
        "payload_aggregate_sha256": identity.payload_aggregate_sha256,
        "payload_identity_sha256": identity_hash, "producer_receipt_sha256": producer_hash,
        "build_identity_sha256": hex_lower(&Sha256::digest(&build_bytes)),
        "qualified_archive_sha256": archive_hash,
        "wheel": wheel_plan.filename, "wheel_sha256": wheel_hash, "files": identity.files});
        let receipt_name = format!("wheel-native-receipt-{}.json", identity.target);
        let receipt_path = staging.join(&receipt_name);
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
        drop(output);
        let output_wheel = out_dir.join(&wheel_plan.filename);
        let output_receipt = out_dir.join(&receipt_name);
        promote_pair(
            &wheel_bytes,
            &wheel_hash,
            &receipt_path,
            &output_wheel,
            &output_receipt,
        )?;
        eprintln!(
            "wrote {} and {}",
            output_wheel.display(),
            output_receipt.display()
        );
        Ok(())
    })();
    let cleanup =
        fs::remove_dir_all(&staging).map_err(|e| format!("remove wheel staging directory: {e}"));
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

    fn temp_root(label: &str) -> Result<std::path::PathBuf, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-wheel-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        Ok(root)
    }

    fn contract() -> Result<DistributionContract, String> {
        parse_distribution_contract(
            "policy",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../policy/distribution.toml"
            )),
        )
    }

    fn sources() -> PythonSources {
        PythonSources {
            pyproject: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../packaging/python/pyproject.toml"
            ))
            .to_owned(),
            readme: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../packaging/python/README.md"
            ))
            .to_owned(),
        }
    }

    struct Fixture {
        root: std::path::PathBuf,
        source: std::path::PathBuf,
        identity: payload::FinalNativePayloadIdentity,
        contract: DistributionContract,
        sources: PythonSources,
    }

    fn fixture(target: &str, executable: &str) -> Result<Fixture, String> {
        let root = temp_root("fixture")?;
        let source = root.join("payload");
        fs::create_dir(&source).map_err(|e| e.to_string())?;
        for (path, bytes) in [
            (executable, b"native-bytes".as_slice()),
            ("LICENSE-MIT", b"mit"),
            ("LICENSE-APACHE", b"apache"),
            (
                "README-server.txt",
                super::super::release_server_readme("0.11.0").as_bytes(),
            ),
        ] {
            fs::write(source.join(path), bytes).map_err(|e| e.to_string())?;
        }
        let files = payload::payload_file_identities(&source, executable)?;
        let identity = payload::FinalNativePayloadIdentity {
            schema_version: 1,
            kind: "ripr_final_native_payload".into(),
            product: "ripr".into(),
            version: "0.11.0".into(),
            target: target.into(),
            executable: executable.into(),
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
        Ok(Fixture {
            root,
            source,
            identity,
            contract: contract()?,
            sources: sources(),
        })
    }

    fn pack(fx: &Fixture, name: &str) -> Result<(std::path::PathBuf, Vec<u8>), String> {
        let path = fx.root.join(name);
        let (hash, bytes) =
            pack_and_verify(&fx.source, &fx.identity, &fx.contract, &fx.sources, &path)?;
        assert_eq!(hash, sha256_file(&path)?);
        Ok((path, bytes))
    }

    fn reject(fx: &Fixture, entries: &[Entry], label: &str) -> Result<(), String> {
        let path = fx.root.join(format!("{label}.whl"));
        write_wheel(entries, &path)?;
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        let Err(_) = verify_wheel(&bytes, &fx.identity, &fx.contract, &fx.sources) else {
            return Err(format!("accepted mutation {label}"));
        };
        Ok(())
    }

    /// Recomputes RECORD so a mutation is judged on its own merits.
    fn rerecord(entries: &mut [Entry]) -> Result<(), String> {
        let (record, covered) = entries.split_last_mut().ok_or("no entries")?;
        record.bytes = record_bytes(covered, &record.name);
        Ok(())
    }

    fn check_inventory(
        fx: &Fixture,
        executable: &str,
        tag: &str,
        wheel_name: &str,
    ) -> Result<(), String> {
        let (path, bytes) = pack(fx, wheel_name)?;
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some(wheel_name),
            "fixture wheel path"
        );
        let plan = plan(&fx.identity, &fx.contract)?;
        assert_eq!(plan.filename, wheel_name);
        assert_eq!(plan.platform_tag, tag);
        let entries = read_wheel(&bytes)?;
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                format!("ripr_rs-0.11.0.data/scripts/{executable}").as_str(),
                "ripr_rs-0.11.0.dist-info/METADATA",
                "ripr_rs-0.11.0.dist-info/WHEEL",
                "ripr_rs-0.11.0.dist-info/licenses/LICENSE-APACHE",
                "ripr_rs-0.11.0.dist-info/licenses/LICENSE-MIT",
                "ripr_rs-0.11.0.dist-info/RECORD",
            ]
        );
        assert_eq!(entries[0].bytes, b"native-bytes");
        assert_eq!(entries[0].mode, 0o100_755);
        assert_eq!(entries[3].bytes, b"apache");
        assert_eq!(entries[4].bytes, b"mit");
        let wheel = String::from_utf8(entries[2].bytes.clone()).map_err(|e| e.to_string())?;
        assert_eq!(
            wheel,
            format!(
                "Wheel-Version: 1.0\nGenerator: ripr-xtask\nRoot-Is-Purelib: false\nTag: py3-none-{tag}\n"
            )
        );
        let record = String::from_utf8(entries[5].bytes.clone()).map_err(|e| e.to_string())?;
        let lines: Vec<&str> = record.lines().collect();
        assert_eq!(lines.len(), 6);
        assert_eq!(lines[5], "ripr_rs-0.11.0.dist-info/RECORD,,");
        assert_eq!(lines[0], record_line(&entries[0].name, b"native-bytes"));
        let metadata = String::from_utf8(entries[1].bytes.clone()).map_err(|e| e.to_string())?;
        assert!(metadata.starts_with("Metadata-Version: 2.4\nName: ripr-rs\nVersion: 0.11.0\n"));
        assert!(metadata.contains("\nLicense-File: LICENSE-APACHE\nLicense-File: LICENSE-MIT\n"));
        assert!(metadata.contains("\nAuthor: Effortless Metrics\n"));
        assert!(
            metadata
                .contains("\nProject-URL: Repository, https://github.com/EffortlessMetrics/ripr\n")
        );
        assert!(metadata.contains("\nDescription-Content-Type: text/markdown\n\n"));
        assert!(metadata.ends_with(&fx.sources.readme));
        Ok(())
    }

    #[test]
    fn linux_wheel_has_exact_inventory_tag_and_record() -> Result<(), String> {
        let fx = fixture("x86_64-unknown-linux-gnu", "ripr")?;
        check_inventory(
            &fx,
            "ripr",
            "manylinux_2_34_x86_64",
            "ripr_rs-0.11.0-py3-none-manylinux_2_34_x86_64.whl",
        )?;
        fs::remove_dir_all(&fx.root).map_err(|e| e.to_string())
    }

    #[test]
    fn windows_wheel_carries_the_exe_script_and_win_tag() -> Result<(), String> {
        let fx = fixture("x86_64-pc-windows-msvc", "ripr.exe")?;
        check_inventory(
            &fx,
            "ripr.exe",
            "win_amd64",
            "ripr_rs-0.11.0-py3-none-win_amd64.whl",
        )?;
        fs::remove_dir_all(&fx.root).map_err(|e| e.to_string())
    }

    #[test]
    fn every_contract_target_yields_its_policy_tag_and_archive_name() -> Result<(), String> {
        let contract = contract()?;
        assert_eq!(contract.target.len(), 5);
        for row in &contract.target {
            let fx = fixture(&row.rust_target, &row.executable)?;
            let plan = plan(&fx.identity, &contract)?;
            assert_eq!(plan.platform_tag, row.wheel_platform_tag);
            assert_eq!(plan.archive, row.archive);
            assert!(
                plan.filename
                    .ends_with(&format!("-py3-none-{}.whl", row.wheel_platform_tag))
            );
            fs::remove_dir_all(&fx.root).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    #[test]
    fn repacking_is_byte_identical() -> Result<(), String> {
        let fx = fixture("x86_64-unknown-linux-gnu", "ripr")?;
        let (first, _) = pack(&fx, "one.whl")?;
        let (second, _) = pack(&fx, "two.whl")?;
        assert_eq!(
            fs::read(first).map_err(|e| e.to_string())?,
            fs::read(second).map_err(|e| e.to_string())?
        );
        fs::remove_dir_all(&fx.root).map_err(|e| e.to_string())
    }

    #[test]
    fn verification_rejects_tampered_wheels() -> Result<(), String> {
        let fx = fixture("x86_64-unknown-linux-gnu", "ripr")?;
        let (_, bytes) = pack(&fx, "base.whl")?;
        let base = read_wheel(&bytes)?;
        let last = base.len() - 1;

        let mut changed = base.clone();
        changed[0].bytes = b"other-bytes".to_vec();
        rerecord(&mut changed)?;
        reject(&fx, &changed, "script-bytes")?;

        let mut changed = base.clone();
        changed[3].bytes = b"other".to_vec();
        rerecord(&mut changed)?;
        reject(&fx, &changed, "license-bytes")?;

        let mut changed = base.clone();
        changed[last].bytes = String::from_utf8(changed[last].bytes.clone())
            .map_err(|e| e.to_string())?
            .replacen("sha256=", "sha256=A", 1)
            .into_bytes();
        reject(&fx, &changed, "record-hash")?;

        let mut changed = base.clone();
        let extra = Entry {
            name: "ripr_rs-0.11.0.dist-info/extra".into(),
            bytes: b"x".to_vec(),
            mode: FILE_MODE,
        };
        changed.insert(last, extra);
        rerecord(&mut changed)?;
        reject(&fx, &changed, "extra-entry")?;

        let mut changed = base.clone();
        changed.remove(3);
        rerecord(&mut changed)?;
        reject(&fx, &changed, "missing-entry")?;

        let mut changed = base.clone();
        changed.swap(3, 4);
        rerecord(&mut changed)?;
        reject(&fx, &changed, "entry-order")?;

        let mut changed = base.clone();
        changed[0].mode = FILE_MODE;
        reject(&fx, &changed, "script-mode")?;

        let mut changed = base.clone();
        changed[2].bytes = wheel_text("win_amd64").into_bytes();
        rerecord(&mut changed)?;
        reject(&fx, &changed, "wheel-tag")?;

        let mut changed = base.clone();
        changed[1].bytes = String::from_utf8(changed[1].bytes.clone())
            .map_err(|e| e.to_string())?
            .replacen("Version: 0.11.0", "Version: 0.0.1", 1)
            .into_bytes();
        rerecord(&mut changed)?;
        reject(&fx, &changed, "metadata")?;

        // The unmutated entries still verify, so each rejection above is
        // attributable to its mutation.
        verify_wheel(&bytes, &fx.identity, &fx.contract, &fx.sources)?;
        fs::remove_dir_all(&fx.root).map_err(|e| e.to_string())
    }

    #[test]
    fn packing_rejects_payload_bytes_that_disagree_with_the_identity() -> Result<(), String> {
        let fx = fixture("x86_64-unknown-linux-gnu", "ripr")?;
        fs::write(fx.source.join("ripr"), b"swapped").map_err(|e| e.to_string())?;
        let Err(_) = pack(&fx, "swapped.whl") else {
            return Err("payload differing from its identity accepted".to_string());
        };
        fs::write(fx.source.join("ripr"), b"native-bytes").map_err(|e| e.to_string())?;
        verify_notices(&fx.identity, &fx.source)?;
        pack(&fx, "restored.whl")?;
        fs::remove_dir_all(&fx.root).map_err(|e| e.to_string())
    }

    #[test]
    fn metadata_requires_the_contract_distribution_name() -> Result<(), String> {
        let fx = fixture("x86_64-unknown-linux-gnu", "ripr")?;
        let mut renamed = fx.contract.clone();
        renamed.python.distribution = "other-name".into();
        let Err(_) = metadata_text(&renamed, &fx.sources, "0.11.0") else {
            return Err("pyproject name differing from the contract accepted".to_string());
        };
        let mut wrong_executable = fx.identity.clone();
        wrong_executable.executable = "ripr.exe".into();
        let Err(_) = plan(&wrong_executable, &fx.contract) else {
            return Err("executable differing from the target row accepted".to_string());
        };
        fs::remove_dir_all(&fx.root).map_err(|e| e.to_string())
    }

    #[test]
    fn base64_matches_known_vectors() {
        for (input, expected) in [
            (b"".as_slice(), ""),
            (b"f", "Zg"),
            (b"fo", "Zm8"),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg"),
            (b"fooba", "Zm9vYmE"),
            (b"foobar", "Zm9vYmFy"),
            (&[0xfb, 0xff, 0xfe], "-__-"),
        ] {
            assert_eq!(urlsafe_b64_nopad(input), expected);
        }
        // Empty-input SHA-256, as RECORD files carry it.
        assert_eq!(
            urlsafe_b64_nopad(&Sha256::digest(b"")),
            "47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU"
        );
    }
}
