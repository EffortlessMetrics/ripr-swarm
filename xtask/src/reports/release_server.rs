mod npm_native;
mod packages;
mod payload;
mod producer;
mod qualification;
mod wheel_native;

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{command_success_owned, run_owned};

pub(crate) fn release_server_archive(args: &[String]) -> Result<(), String> {
    if let Some(artifacts) = optional_release_arg(args, "packages-from") {
        let out_dir = required_release_arg(args, "out-dir", "PACKAGES_OUT_DIR")?;
        return packages::produce_family(Path::new(&artifacts), Path::new(&out_dir));
    }
    if let Some(payload_dir) = optional_release_arg(args, "npm-payload-dir") {
        let identity = required_release_arg(args, "payload-identity", "PAYLOAD_IDENTITY")?;
        let producer = required_release_arg(args, "producer-receipt", "PRODUCER_RECEIPT")?;
        let build_identity = required_release_arg(args, "build-identity", "BUILD_IDENTITY")?;
        let archive = required_release_arg(args, "qualified-archive", "QUALIFIED_ARCHIVE")?;
        let out_dir = required_release_arg(args, "out-dir", "NPM_NATIVE_OUT_DIR")?;
        return npm_native::produce(
            Path::new(&payload_dir),
            Path::new(&identity),
            Path::new(&producer),
            Path::new(&build_identity),
            Path::new(&archive),
            Path::new(&out_dir),
        );
    }
    if let Some(payload_dir) = optional_release_arg(args, "wheel-payload-dir") {
        let identity = required_release_arg(args, "payload-identity", "PAYLOAD_IDENTITY")?;
        let producer = required_release_arg(args, "producer-receipt", "PRODUCER_RECEIPT")?;
        let build_identity = required_release_arg(args, "build-identity", "BUILD_IDENTITY")?;
        let archive = required_release_arg(args, "qualified-archive", "QUALIFIED_ARCHIVE")?;
        let out_dir = required_release_arg(args, "out-dir", "WHEEL_NATIVE_OUT_DIR")?;
        return wheel_native::produce(
            Path::new(&payload_dir),
            Path::new(&identity),
            Path::new(&producer),
            Path::new(&build_identity),
            Path::new(&archive),
            Path::new(&out_dir),
        );
    }
    if let Some(rows_dir) = optional_release_arg(args, "terminal-receipt-dir") {
        let out_dir = required_release_arg(args, "out-dir", "TERMINAL_RECEIPT_OUT_DIR")?;
        let candidate_sha = required_release_arg(args, "candidate-sha", "CANDIDATE_SHA")?;
        let version = required_release_arg(args, "version", "QUALIFICATION_VERSION")?;
        let verify_result = required_release_arg(args, "verify-result", "VERIFY_RESULT")?;
        let build_result = required_release_arg(args, "build-result", "BUILD_RESULT")?;
        let manifest_result = required_release_arg(args, "manifest-result", "MANIFEST_RESULT")?;
        let receipt = qualification::write_terminal_receipt(
            Path::new(&rows_dir),
            Path::new(&out_dir),
            &candidate_sha,
            &version,
            &verify_result,
            &build_result,
            &manifest_result,
        )?;
        eprintln!(
            "wrote terminal qualification receipt: status={} selected={} executed={} failed={} cancelled={} not_run={}",
            receipt.status,
            receipt.execution.selected,
            receipt.execution.executed,
            receipt.execution.failed,
            receipt.execution.cancelled,
            receipt.execution.not_run
        );
        return Ok(());
    }

    if let Some(payload_dir) = optional_release_arg(args, "verify-payload-dir") {
        let identity_path = required_release_arg(args, "payload-identity", "PAYLOAD_IDENTITY")?;
        let identity = payload::verify_final_native_payload(
            Path::new(&payload_dir),
            Path::new(&identity_path),
        )?;
        eprintln!(
            "verified final native payload {} {} ({})",
            identity.version, identity.target, identity.payload_aggregate_sha256
        );
        return Ok(());
    }

    let version = required_release_arg(args, "version", "RAW_VERSION")?;
    let target = required_release_arg(args, "target", "TARGET")?;
    let executable = required_release_arg(args, "executable", "EXECUTABLE")?;
    let archive = required_release_arg(args, "archive", "ARCHIVE")?;
    if !matches!(archive.as_str(), "zip" | "tar.gz") {
        return Err(format!(
            "unsupported release server archive format `{archive}`"
        ));
    }
    let version = normalize_release_version(&version);
    let asset_name = server_asset_name(&version, &target, &archive);
    let dist_dir = Path::new("dist");
    fs::create_dir_all(dist_dir)
        .map_err(|err| format!("failed to create {}: {err}", dist_dir.display()))?;

    let producer = producer::build_release_executable(&version, &target, &executable)?;
    let staged = payload::stage_final_native_payload(&version, &target, &executable, &archive)?;
    producer::bind_to_payload_identity(&producer, &staged.identity)?;
    let payload_root = staged
        .identity_json
        .parent()
        .ok_or_else(|| "payload identity path has no parent directory".to_string())?;
    let producer_receipt = producer::write_receipt(payload_root, &producer)?;
    let producer_receipt_sha256 = sha256_file(&producer_receipt)?;
    let identity_sha256 = payload::payload_identity_sha256(&staged)?;

    let asset_path = dist_dir.join(&asset_name);
    if asset_path.exists() {
        fs::remove_file(&asset_path)
            .map_err(|err| format!("failed to remove {}: {err}", asset_path.display()))?;
    }
    match archive.as_str() {
        "zip" => create_zip_archive(&staged.payload_dir, &asset_path)?,
        "tar.gz" => create_tar_gz_archive(&staged.payload_dir, &asset_path)?,
        other => {
            return Err(format!(
                "unsupported release server archive format `{other}`"
            ));
        }
    }

    let sha = sha256_file(&asset_path)?;
    fs::write(
        dist_dir.join(format!("{asset_name}.sha256")),
        format!("{sha}\n"),
    )
    .map_err(|err| {
        format!(
            "failed to write {}: {err}",
            dist_dir.join(format!("{asset_name}.sha256")).display()
        )
    })?;
    eprintln!("wrote {}", asset_path.display());
    eprintln!("wrote {}", staged.identity_json.display());
    eprintln!("wrote {}", staged.identity_markdown.display());
    eprintln!("wrote {}", producer_receipt.display());
    eprintln!(
        "final native payload aggregate SHA-256: {}",
        staged.identity.payload_aggregate_sha256
    );
    eprintln!("final native payload identity SHA-256: {identity_sha256}");
    eprintln!("producer build receipt SHA-256: {producer_receipt_sha256}");
    Ok(())
}

pub(crate) fn release_server_manifest(args: &[String]) -> Result<(), String> {
    let version = required_release_arg(args, "version", "RAW_VERSION")?;
    let repository = required_release_arg(args, "repository", "REPOSITORY")?;
    let version = normalize_release_version(&version);
    let dist_dir = Path::new("dist");
    // Published as `SHA256SUMS` (the near-universal ecosystem convention) so
    // consumers can run `sha256sum -c SHA256SUMS` against the release assets.
    // The content format is unchanged (`<sha256>  <file_name>` per line).
    let sha256sums_path = dist_dir.join("SHA256SUMS");
    // Also remove any legacy `checksums.txt` left in a reused `dist/` from a
    // pre-rename run so the stale sidecar cannot linger beside — or be hashed
    // into — the new `SHA256SUMS`.
    let legacy_checksums_path = dist_dir.join("checksums.txt");
    for path in [&sha256sums_path, &legacy_checksums_path] {
        if path.exists() {
            fs::remove_file(path)
                .map_err(|err| format!("failed to remove {}: {err}", path.display()))?;
        }
    }

    let mut assets = serde_json::Map::new();
    for asset in release_server_assets(dist_dir, &version)? {
        let sha_path = dist_dir.join(format!("{}.sha256", asset.file_name));
        let sha = read_trimmed(&sha_path)?;
        let url = format!(
            "https://github.com/{repository}/releases/download/v{version}/{}",
            asset.file_name
        );
        assets.insert(
            asset.target,
            serde_json::json!({ "url": url, "sha256": sha }),
        );
    }

    let manifest = serde_json::json!({
        "version": version,
        "assets": assets,
    });
    let manifest_path = dist_dir.join(format!("ripr-server-manifest-v{version}.json"));
    let manifest_text = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("failed to render release server manifest: {err}"))?;
    fs::write(&manifest_path, format!("{manifest_text}\n"))
        .map_err(|err| format!("failed to write {}: {err}", manifest_path.display()))?;

    let mut checksum_lines = Vec::new();
    for path in sorted_dist_files(dist_dir)? {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.ends_with(".sha256")
            || file_name == "SHA256SUMS"
            || file_name == "checksums.txt"
        {
            continue;
        }
        checksum_lines.push(format!("{}  {file_name}", sha256_file(&path)?));
    }
    fs::write(&sha256sums_path, format!("{}\n", checksum_lines.join("\n")))
        .map_err(|err| format!("failed to write {}: {err}", sha256sums_path.display()))?;
    eprintln!("wrote {}", manifest_path.display());
    eprintln!("wrote {}", sha256sums_path.display());

    let manifest_sha256 = sha256_file(&manifest_path)?;
    // The editor descriptor is keyed by the distribution generation
    // (MAJOR.MINOR.PATCH): the downloader looks up
    // `distributionGeneration(requestedVersion)`, which never carries a
    // prerelease or build-metadata suffix. A `--version 0.11.0-rc.1` run
    // would embed a generation no request can match, silently leaving the
    // packaged RC extension without its admitted fallback row (#3798), so
    // refuse the write before the descriptor can be committed.
    let generation_components = version.split('.').count();
    if version.contains('-')
        || version.contains('+')
        || generation_components != 3
        || version
            .split('.')
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(format!(
            "release-server-manifest must use the distribution generation (MAJOR.MINOR.PATCH) to write the editor descriptor; got `{version}`"
        ));
    }
    let descriptor_path = editor_distribution_descriptor_path();
    let descriptor_dir = descriptor_path
        .parent()
        .ok_or_else(|| "editor distribution descriptor path has no parent directory".to_string())?;
    fs::create_dir_all(descriptor_dir)
        .map_err(|err| format!("failed to create {}: {err}", descriptor_dir.display()))?;
    fs::write(
        &descriptor_path,
        editor_distribution_descriptor_source(&version, &manifest_sha256),
    )
    .map_err(|err| format!("failed to write {}: {err}", descriptor_path.display()))?;
    eprintln!("wrote {}", descriptor_path.display());
    Ok(())
}

/// The editor module that embeds the admitted raw-byte manifest digest for a
/// distribution generation (#3798). Written relative to the repository root
/// by `release-bind-server-descriptor` when the extension is packaged (and by
/// `release-server-manifest` for local rehearsals); the committed module stays
/// empty, so only a packaged extension admits a generation's placements.
fn editor_distribution_descriptor_path() -> PathBuf {
    Path::new("editors")
        .join("vscode")
        .join("src")
        .join("serverDescriptor.ts")
}

pub(crate) fn editor_distribution_descriptor_source(
    generation: &str,
    manifest_sha256: &str,
) -> String {
    format!(
        r#"/**
 * Release-admitted distribution descriptors for managed server downloads
 * (#3798).
 *
 * GENERATED by `cargo xtask release-bind-server-descriptor` at package time
 * from the published manifest and its receipted digest; the committed module
 * stays empty. One raw-byte manifest SHA-256 per distribution generation admits
 * both the exact stable placement and the one predeclared exact RC placement,
 * because a generation's manifest bytes are placement-neutral and identical
 * on both placements.
 *
 * A generation without an admitted descriptor has no fallback row: stable
 * absence stays terminal and no RC manifest is fetched (#3798, control 7).
 * Never edit a digest by hand — regenerate the module from the built
 * distribution so the digest names real manifest bytes.
 */
export interface ServerDistributionDescriptor {{
  /** Distribution generation, e.g. `0.11.0`: no prerelease or build metadata. */
  readonly generation: string;
  /**
   * Raw-byte SHA-256 of the generation's
   * `ripr-server-manifest-v<generation>.json` as published to both placements.
   */
  readonly manifestSha256: string;
}}

export const SERVER_DISTRIBUTION_DESCRIPTORS: readonly ServerDistributionDescriptor[] = [
  {{ generation: "{generation}", manifestSha256: "{manifest_sha256}" }},
];
"#,
        generation = generation,
        manifest_sha256 = manifest_sha256
    )
}

/// Binds the published server manifest's raw-byte digest into the editor
/// descriptor at package time (#3798). The expected digest comes from the
/// release transaction receipt, never from the manifest being checked, so the
/// descriptor is no longer self-referential: the manifest bytes must match the
/// receipted digest before they are admitted, and the generation, asset URLs
/// and archive digests must be well formed.
pub(crate) fn release_bind_server_descriptor(args: &[String]) -> Result<(), String> {
    let version = normalize_release_version(&required_release_arg(args, "version", "RAW_VERSION")?);
    let manifest_path = PathBuf::from(required_release_arg(args, "manifest", "MANIFEST_PATH")?);
    let expected = required_release_arg(args, "manifest-sha256", "MANIFEST_SHA256")?;
    let bytes = fs::read(&manifest_path)
        .map_err(|err| format!("failed to read {}: {err}", manifest_path.display()))?;
    let source = bound_descriptor_source(&version, &bytes, &expected)?;
    let descriptor_path = editor_distribution_descriptor_path();
    fs::write(&descriptor_path, source)
        .map_err(|err| format!("failed to write {}: {err}", descriptor_path.display()))?;
    eprintln!("bound {} to manifest {expected}", descriptor_path.display());
    Ok(())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn bound_descriptor_source(
    generation: &str,
    manifest_bytes: &[u8],
    expected_sha256: &str,
) -> Result<String, String> {
    if !is_sha256_hex(expected_sha256) {
        return Err(format!(
            "--manifest-sha256 must be 64 lowercase hex characters; got `{expected_sha256}`"
        ));
    }
    let actual = format!("{:x}", Sha256::digest(manifest_bytes));
    if actual != expected_sha256 {
        return Err(format!(
            "manifest SHA-256 {actual} does not match the receipted digest {expected_sha256}"
        ));
    }
    if generation.split('.').count() != 3
        || generation
            .split('.')
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(format!(
            "descriptor generation must be MAJOR.MINOR.PATCH; got `{generation}`"
        ));
    }
    let manifest: serde_json::Value = serde_json::from_slice(manifest_bytes)
        .map_err(|err| format!("manifest is not valid JSON: {err}"))?;
    if manifest["version"].as_str() != Some(generation) {
        return Err(format!(
            "manifest version {} does not match generation {generation}",
            manifest["version"]
        ));
    }
    let assets = manifest["assets"]
        .as_object()
        .filter(|assets| !assets.is_empty())
        .ok_or_else(|| "manifest has no assets".to_string())?;
    for (target, asset) in assets {
        let url = asset["url"].as_str().unwrap_or_default();
        if !url.starts_with("https://") {
            return Err(format!("manifest asset {target} has a non-https url"));
        }
        if !is_sha256_hex(asset["sha256"].as_str().unwrap_or_default()) {
            return Err(format!("manifest asset {target} has no valid sha256"));
        }
    }
    Ok(editor_distribution_descriptor_source(
        generation,
        expected_sha256,
    ))
}

pub(crate) fn release_upload_assets(args: &[String]) -> Result<(), String> {
    let version = normalize_release_version(&required_release_arg(args, "version", "RAW_VERSION")?);
    let tag = format!("v{version}");
    if !command_success_owned(
        "gh",
        &["release".to_string(), "view".to_string(), tag.clone()],
    )? {
        run_owned(
            "gh",
            &[
                "release".to_string(),
                "create".to_string(),
                tag.clone(),
                "--title".to_string(),
                format!("ripr {version}"),
            ],
        )?;
    }

    let mut upload_args = vec!["release".to_string(), "upload".to_string(), tag];
    for path in sorted_dist_files(Path::new("dist"))? {
        upload_args.push(path.to_string_lossy().to_string());
    }
    upload_args.push("--clobber".to_string());
    run_owned("gh", &upload_args)
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ReleaseServerAsset {
    pub(crate) target: String,
    pub(crate) file_name: String,
}

pub(crate) fn release_server_assets(
    dist_dir: &Path,
    version: &str,
) -> Result<Vec<ReleaseServerAsset>, String> {
    let prefix = format!("ripr-server-v{version}-");
    let mut assets = Vec::new();
    for path in sorted_dist_files(dist_dir)? {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(target_with_suffix) = file_name.strip_prefix(&prefix) else {
            continue;
        };
        let target = target_with_suffix
            .strip_suffix(".tar.gz")
            .or_else(|| target_with_suffix.strip_suffix(".zip"));
        let Some(target) = target else {
            continue;
        };
        assets.push(ReleaseServerAsset {
            target: target.to_string(),
            file_name: file_name.to_string(),
        });
    }
    Ok(assets)
}

pub(crate) fn optional_release_arg(args: &[String], flag: &str) -> Option<String> {
    let flag_name = format!("--{flag}");
    for window in args.windows(2) {
        if window[0] == flag_name {
            return Some(window[1].clone());
        }
    }
    let inline_prefix = format!("{flag_name}=");
    args.iter()
        .find_map(|arg| arg.strip_prefix(&inline_prefix).map(str::to_string))
}

pub(crate) fn required_release_arg(
    args: &[String],
    flag: &str,
    env_name: &str,
) -> Result<String, String> {
    if let Some(value) = optional_release_arg(args, flag) {
        return Ok(value);
    }
    let flag_name = format!("--{flag}");
    std::env::var(env_name).map_err(|err| format!("missing {flag_name} or {env_name}: {err}"))
}

pub(crate) fn normalize_release_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_string()
}

pub(crate) fn release_server_readme(version: &str) -> String {
    format!(
        "ripr server {version}\n\nThis archive contains the ripr executable used by the VS Code/Open VSX\nextension. It is distributed under MIT OR Apache-2.0."
    )
}

/// Maximum compression for release server archives: download-size only.
/// The archive formats, file names and entry layout are unchanged, so
/// existing extractors (Expand-Archive, `tar -xzf`) need no change; only
/// the deflate effort inside the same gzip/zip containers rises.
pub(crate) const ARCHIVE_COMPRESSION_LEVEL: u32 = 9;

pub(crate) fn gzip_compression() -> flate2::Compression {
    flate2::Compression::new(ARCHIVE_COMPRESSION_LEVEL)
}

pub(crate) fn zip_file_options() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(i64::from(ARCHIVE_COMPRESSION_LEVEL)))
        .unix_permissions(0o644)
}

pub(crate) fn create_tar_gz_archive(package_dir: &Path, asset_path: &Path) -> Result<(), String> {
    // In-process tar+gzip instead of shelling out to `tar -czf`: there is
    // no portable way to pass a gzip level to external tar (GNU tar takes
    // it from the environment, libarchive/bsdtar from `--options`, and
    // Windows ships bsdtar), while flate2 level 9 is identical on every
    // release runner. The output stays a plain `.tar.gz`.
    let file = fs::File::create(asset_path)
        .map_err(|err| format!("failed to create {}: {err}", asset_path.display()))?;
    let encoder = flate2::write::GzEncoder::new(file, gzip_compression());
    let mut builder = tar::Builder::new(encoder);
    builder
        .append_dir_all(".", package_dir)
        .map_err(|err| format!("failed to write {}: {err}", asset_path.display()))?;
    let encoder = builder
        .into_inner()
        .map_err(|err| format!("failed to finish {}: {err}", asset_path.display()))?;
    encoder
        .finish()
        .map_err(|err| format!("failed to finish {}: {err}", asset_path.display()))?;
    Ok(())
}

pub(crate) fn create_zip_archive(package_dir: &Path, asset_path: &Path) -> Result<(), String> {
    let file = fs::File::create(asset_path)
        .map_err(|err| format!("failed to create {}: {err}", asset_path.display()))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip_file_options();

    let entries = fs::read_dir(package_dir)
        .map_err(|err| format!("failed to read {}: {err}", package_dir.display()))?;
    let mut sorted: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    sorted.sort();

    for path in sorted {
        let metadata = fs::metadata(&path)
            .map_err(|err| format!("failed to stat {}: {err}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!(
                "release server package directory must be flat; found non-file `{}`",
                path.display()
            ));
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("invalid file name in {}", path.display()))?
            .to_string();
        let entry_options = if file_name.ends_with(".exe") {
            options.unix_permissions(0o755)
        } else if metadata.permissions().readonly() {
            options
        } else if !file_name.contains('.') {
            // Best-effort executable bit for Unix-style binaries (ripr) without an extension.
            options.unix_permissions(0o755)
        } else {
            options
        };
        writer
            .start_file(&file_name, entry_options)
            .map_err(|err| format!("failed to start zip entry {file_name}: {err}"))?;
        let mut input = fs::File::open(&path)
            .map_err(|err| format!("failed to open {} for zip: {err}", path.display()))?;
        std::io::copy(&mut input, &mut writer)
            .map_err(|err| format!("failed to write zip entry {file_name}: {err}"))?;
    }
    writer
        .finish()
        .map_err(|err| format!("failed to finalize {}: {err}", asset_path.display()))?;
    Ok(())
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|err| format!("failed to open {} for hashing: {err}", path.display()))?;
    sha256_reader(&mut file)
        .map_err(|err| format!("failed to read {} for hashing: {err}", path.display()))
}

/// Hash a file while retaining its native I/O error for callers that own diagnostics.
pub(crate) fn sha256_file_io(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    sha256_reader(&mut file)
}

fn sha256_reader(reader: &mut impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

#[cfg(test)]
mod hash_reader_tests {
    use super::*;
    use std::io::{self, Cursor};

    struct FailOnce<R> {
        inner: R,
        error: Option<io::ErrorKind>,
    }

    impl<R: Read> Read for FailOnce<R> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if let Some(kind) = self.error.take() {
                return Err(io::Error::from(kind));
            }
            self.inner.read(buffer)
        }
    }

    #[test]
    fn retries_interruption_but_propagates_other_read_errors() -> io::Result<()> {
        let bytes = b"the full binary identity";
        let mut interrupted = FailOnce {
            inner: Cursor::new(bytes),
            error: Some(io::ErrorKind::Interrupted),
        };
        assert_eq!(
            sha256_reader(&mut interrupted)?,
            format!("{:x}", Sha256::digest(bytes))
        );
        let mut denied = FailOnce {
            inner: Cursor::new(bytes),
            error: Some(io::ErrorKind::PermissionDenied),
        };
        assert_eq!(
            sha256_reader(&mut denied).err().map(|error| error.kind()),
            Some(io::ErrorKind::PermissionDenied)
        );
        Ok(())
    }
}

#[cfg(test)]
mod archive_compression_tests {
    use super::*;

    fn fixture_dir(label: &str) -> Result<PathBuf, String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-archive-compression-{label}-{}",
            std::process::id()
        ));
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        }
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        // Highly compressible payload so level choice changes bytes.
        fs::write(dir.join("ripr"), vec![b'x'; 64 * 1024]).map_err(|e| e.to_string())?;
        fs::write(
            dir.join("README-server.txt"),
            release_server_readme("0.11.0"),
        )
        .map_err(|e| e.to_string())?;
        Ok(dir)
    }

    #[test]
    fn compression_level_is_pinned_at_maximum_for_both_formats() {
        assert_eq!(ARCHIVE_COMPRESSION_LEVEL, 9);
        assert_eq!(gzip_compression(), flate2::Compression::new(9));
        let options = zip_file_options();
        assert_eq!(options.get_compression_level(), Some(9));
    }

    #[test]
    fn tar_gz_is_level_nine_gzip_and_round_trips() -> Result<(), String> {
        let dir = fixture_dir("tar")?;
        let asset = dir.with_extension("tar.gz");
        let result = (|| {
            create_tar_gz_archive(&dir, &asset)?;
            let bytes = fs::read(&asset).map_err(|e| e.to_string())?;
            // RFC 1952: magic + deflate, XFL=2 marks maximum compression.
            assert_eq!(&bytes[0..3], &[0x1f, 0x8b, 8]);
            assert_eq!(bytes[8], 2, "gzip XFL must mark level 9");
            let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(&bytes[..]));
            let mut names = Vec::new();
            for entry in archive.entries().map_err(|e| e.to_string())? {
                let mut entry = entry.map_err(|e| e.to_string())?;
                let name = entry
                    .path()
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace("./", "");
                if name.is_empty() || name.ends_with('/') {
                    continue;
                }
                let mut body = Vec::new();
                entry.read_to_end(&mut body).map_err(|e| e.to_string())?;
                assert_eq!(body, fs::read(dir.join(&name)).map_err(|e| e.to_string())?);
                names.push(name);
            }
            names.sort();
            assert_eq!(names, ["README-server.txt", "ripr"]);
            Ok(())
        })();
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_file(&asset);
        result
    }

    #[test]
    fn zip_stays_deflated_and_round_trips() -> Result<(), String> {
        let dir = fixture_dir("zip")?;
        let asset = dir.with_extension("zip");
        let result = (|| {
            create_zip_archive(&dir, &asset)?;
            let file = fs::File::open(&asset).map_err(|e| e.to_string())?;
            let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            assert_eq!(archive.len(), 2);
            for index in 0..archive.len() {
                let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
                assert_eq!(entry.compression(), zip::CompressionMethod::Deflated);
                let name = entry.name().map_err(|e| e.to_string())?.to_string();
                let mut body = Vec::new();
                entry.read_to_end(&mut body).map_err(|e| e.to_string())?;
                assert_eq!(body, fs::read(dir.join(&name)).map_err(|e| e.to_string())?);
                assert!(entry.size() > entry.compressed_size());
            }
            Ok(())
        })();
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_file(&asset);
        result
    }
}

pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn sorted_dist_files(dist_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dist_dir)
        .map_err(|err| format!("failed to read {}: {err}", dist_dir.display()))?
    {
        let path = entry
            .map_err(|err| format!("failed to read entry under {}: {err}", dist_dir.display()))?
            .path();
        if path.is_file() {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

pub(crate) fn read_trimmed(path: &Path) -> Result<String, String> {
    fs::read_to_string(path)
        .map(|text| text.trim().to_string())
        .map_err(|err| format!("failed to read {}: {err}", path.display()))
}

/// Release asset name for one server archive. `[package.metadata.binstall]` in
/// `crates/ripr/Cargo.toml` spells the same shape; `binstall_metadata` tests
/// fail when the two drift.
pub(crate) fn server_asset_name(version: &str, target: &str, archive: &str) -> String {
    format!("ripr-server-v{version}-{target}.{archive}")
}

#[cfg(test)]
mod binstall_metadata;
