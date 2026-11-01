mod npm_native;
mod payload;
mod producer;
mod qualification;

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::GzBuilder;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::Builder;

use crate::{command_success_owned, run_output, run_owned};

pub(crate) fn release_server_archive(args: &[String]) -> Result<(), String> {
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
    let version = normalize_product_version(&version)?;
    let asset_name = format!("ripr-server-v{version}-{target}.{archive}");
    let package_dir = Path::new("package");
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
    write_release_server_receipt(
        &version,
        &target,
        &executable,
        &archive,
        package_dir,
        &asset_path,
    )?;
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
    // Placement-independent subject identity: the manifest describes product
    // bytes, never a publication channel. A channel-suffixed version would
    // fork subject names between RC and stable, so it fails closed here;
    // placement travels in the installed catalog, not the manifest.
    let version = normalize_product_version(&version)?;
    let dist_dir = Path::new("dist");
    // Published as `SHA256SUMS` (the near-universal ecosystem convention) so
    // consumers can run `sha256sum -c SHA256SUMS` against the release assets.
    // The content format is unchanged (`<sha256>  <file_name>` per line).
    let sha256sums_path = dist_dir.join("SHA256SUMS");
    // Also remove any legacy `checksums.txt` left in a reused `dist/` from a
    // pre-rename run so the stale sidecar cannot linger beside — or be hashed
    // into — the new `SHA256SUMS`.
    let legacy_checksums_path = dist_dir.join("checksums.txt");
    let manifest_path = dist_dir.join(format!("ripr-server-manifest-v{version}.json"));
    let assembly_receipt_path =
        dist_dir.join(format!("ripr-server-assembly-v{version}.receipt.json"));
    if legacy_checksums_path.exists() {
        fs::remove_file(&legacy_checksums_path).map_err(|err| {
            format!(
                "failed to remove {} before validation: {err}",
                legacy_checksums_path.display()
            )
        })?;
    }
    let outputs = render_release_server_assembly(dist_dir, &version, &repository)?;
    write_release_server_outputs_transactional(&[
        (&manifest_path, outputs.manifest.as_str()),
        (&sha256sums_path, outputs.checksums.as_str()),
        (&assembly_receipt_path, outputs.assembly_receipt.as_str()),
    ])?;
    eprintln!("wrote {}", manifest_path.display());
    eprintln!("wrote {}", sha256sums_path.display());
    eprintln!("wrote {}", assembly_receipt_path.display());
    Ok(())
}

/// Deterministic outputs of the one validated server assembler.
pub(crate) struct ReleaseServerAssemblyOutputs {
    pub(crate) manifest: String,
    pub(crate) checksums: String,
    pub(crate) assembly_receipt: String,
}

/// Shared assembler authority. Performs no writes or public subject changes.
pub(crate) fn render_release_server_assembly(
    dist_dir: &Path,
    version: &str,
    repository: &str,
) -> Result<ReleaseServerAssemblyOutputs, String> {
    let manifest_path = dist_dir.join(format!("ripr-server-manifest-v{version}.json"));
    let receipt_set = validate_release_server_receipts(dist_dir, version)?;
    let receipt_targets = &receipt_set.targets;
    let discovered_assets = release_server_assets(dist_dir, version)?;
    validate_configured_release_server_targets(&discovered_assets)?;
    let asset_targets = discovered_assets
        .iter()
        .map(|asset| asset.target.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if *receipt_targets != asset_targets {
        return Err(format!(
            "release server receipt targets do not match archive targets: receipts {:?}, archives {:?}",
            receipt_targets, asset_targets
        ));
    }
    validate_release_server_staging_inventory(
        dist_dir,
        version,
        &discovered_assets,
        receipt_targets,
    )?;
    let mut assets = serde_json::Map::new();
    let build_identity = &receipt_set.baseline;
    let target_set_digest = release_server_target_set_digest();
    let mut target_set_names = release_server_target_set().to_vec();
    target_set_names.sort_unstable();
    let distribution_generation = release_distribution_generation(
        version,
        &build_identity.candidate_sha,
        &build_identity.candidate_tree,
        &target_set_digest,
    );
    for asset in discovered_assets {
        let receipt = receipt_set
            .receipts
            .get(&asset.target)
            .ok_or_else(|| format!("missing validated receipt for target `{}`", asset.target))?;
        let sha_path = dist_dir.join(format!("{}.sha256", asset.file_name));
        let sha = read_trimmed(&sha_path)?;
        let actual_sha = sha256_file(&dist_dir.join(&asset.file_name))?;
        if sha != actual_sha {
            return Err(format!(
                "archive checksum mismatch for `{}`: sidecar `{sha}`, actual `{actual_sha}`",
                asset.file_name
            ));
        }
        // Placement-independent subject identity only. No scheme, host,
        // repository, tag, port, query, fragment, or absolute URL may enter
        // manifest bytes: retrieval URLs compose later from the accepted
        // installed catalog placement plus this relative subject.
        let subject = asset.file_name.clone();
        assets.insert(
            asset.target,
            serde_json::json!({
                "subject": subject,
                "archive_format": receipt.archive_format,
                "archive_size": receipt.archive.size,
                "sha256": sha,
                "executable": {
                    "path": receipt.executable.path,
                    "size": receipt.executable.size,
                    "sha256": receipt.executable.sha256,
                },
                "receipt": {
                    "path": receipt.path,
                    "sha256": receipt.sha256,
                    "schema_version": receipt.schema_version,
                    "target": receipt.target,
                },
            }),
        );
    }

    let manifest = serde_json::json!({
        "schema_version": "2",
        "product_version": version,
        "distribution_generation": distribution_generation.clone(),
        "source_repository": repository,
        "target_set": {
            "targets": target_set_names,
            "digest": target_set_digest,
        },
        "producer": {
            "tool": "xtask release-server-manifest",
            "schema": "server-manifest/2",
        },
        "build_identity": {
            "repository": build_identity.repository,
            "candidate_sha": build_identity.candidate_sha,
            "candidate_tree": build_identity.candidate_tree,
            "toolchain": build_identity.toolchain,
            "toolchain_file_sha256": build_identity.toolchain_file_sha256,
            "cargo_lock_sha256": build_identity.cargo_lock_sha256,
            "profile": build_identity.profile,
            "features": build_identity.features,
            "locked": build_identity.locked,
        },
        "assets": assets,
    });
    let manifest_text = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("failed to render release server manifest: {err}"))?;
    let manifest_text = format!("{manifest_text}\n");
    let mut checksum_lines = Vec::new();
    for path in sorted_dist_files(dist_dir)? {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.ends_with(".sha256")
            || file_name == "SHA256SUMS"
            || file_name == "checksums.txt"
            || file_name.ends_with(".receipt.json")
            || file_name
                == manifest_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
        {
            continue;
        }
        checksum_lines.push(format!("{}  {file_name}", sha256_file(&path)?));
    }
    checksum_lines.push(format!(
        "{}  {}",
        sha256_bytes(manifest_text.as_bytes()),
        manifest_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
    ));
    let checksum_text = format!("{}\n", checksum_lines.join("\n"));
    let assembly_receipt = serde_json::json!({
        "schema_version": "0.2",
        "assembler": "xtask",
        "version": version,
        "placement_independent": true,
        "distribution_generation": distribution_generation,
        "build_identity": {
            "repository": build_identity.repository,
            "candidate_sha": build_identity.candidate_sha,
            "candidate_tree": build_identity.candidate_tree,
            "toolchain": build_identity.toolchain,
            "toolchain_file_sha256": build_identity.toolchain_file_sha256,
            "cargo_lock_sha256": build_identity.cargo_lock_sha256,
            "profile": build_identity.profile,
            "features": build_identity.features,
            "locked": build_identity.locked,
        },
        "inputs": {
            "receipt_count": receipt_set.receipts.len(),
            "receipts": receipt_set.receipts.iter().map(|(target, receipt)| serde_json::json!({
                "target": target,
                "path": receipt.path,
                "sha256": receipt.sha256,
                "schema_version": receipt.schema_version,
            })).collect::<Vec<_>>(),
        },
        "accepted_subject_count": receipt_set.targets.len(),
        "manifest": {
            "path": manifest_path.file_name().and_then(|name| name.to_str()).unwrap_or_default(),
            "size": manifest_text.len(),
            "sha256": sha256_bytes(manifest_text.as_bytes()),
        },
        "sha256sums": {
            "path": "SHA256SUMS",
            "size": checksum_text.len(),
            "sha256": sha256_bytes(checksum_text.as_bytes()),
        },
        "publication_mutation_attempted": false,
        "disposition": "assembled",
        "non_claims": ["no publication or ref mutation", "final attestation remains downstream"],
    });
    let assembly_text = serde_json::to_string_pretty(&assembly_receipt)
        .map_err(|err| format!("failed to render assembly receipt: {err}"))?;
    let assembly_text = format!("{assembly_text}\n");
    Ok(ReleaseServerAssemblyOutputs {
        manifest: manifest_text,
        checksums: checksum_text,
        assembly_receipt: assembly_text,
    })
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
    for path in release_server_public_asset_paths(Path::new("dist"), &version)? {
        upload_args.push(path.to_string_lossy().to_string());
    }
    upload_args.push("--clobber".to_string());
    run_owned("gh", &upload_args)
}

pub(crate) fn release_server_public_asset_paths(
    dist_dir: &Path,
    version: &str,
) -> Result<Vec<PathBuf>, String> {
    let assets = release_server_assets(dist_dir, version)?;
    let mut paths = Vec::with_capacity(assets.len() * 2 + 2);
    for asset in assets {
        paths.push(dist_dir.join(&asset.file_name));
        paths.push(dist_dir.join(format!("{}.sha256", asset.file_name)));
    }
    paths.push(dist_dir.join(format!("ripr-server-manifest-v{version}.json")));
    paths.push(dist_dir.join("SHA256SUMS"));
    for path in &paths {
        let metadata = fs::symlink_metadata(path)
            .map_err(|err| format!("release asset {} is unavailable: {err}", path.display()))?;
        if !metadata.file_type().is_file() {
            return Err(format!(
                "release asset is not a regular file: {}",
                path.display()
            ));
        }
    }
    Ok(paths)
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
    for entry in fs::read_dir(dist_dir)
        .map_err(|err| format!("failed to read {}: {err}", dist_dir.display()))?
    {
        let path = entry
            .map_err(|err| format!("failed to read entry under {}: {err}", dist_dir.display()))?
            .path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|err| format!("failed to inspect {}: {err}", path.display()))?;
        if !metadata.file_type().is_file() {
            return Err(format!(
                "non-regular release server staging entry `{}`",
                path.display()
            ));
        }
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
        if assets
            .iter()
            .any(|asset: &ReleaseServerAsset| asset.target == target)
        {
            return Err(format!(
                "duplicate release server target `{target}` in `{file_name}`"
            ));
        }
        assets.push(ReleaseServerAsset {
            target: target.to_string(),
            file_name: file_name.to_string(),
        });
    }
    assets.sort_by(|left, right| left.target.cmp(&right.target));
    Ok(assets)
}

pub(crate) fn validate_configured_release_server_targets(
    assets: &[ReleaseServerAsset],
) -> Result<(), String> {
    let configured_targets = release_server_target_set();
    let observed = assets
        .iter()
        .map(|asset| asset.target.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for expected in configured_targets {
        if !observed.contains(expected) {
            return Err(format!(
                "missing configured release server target `{expected}`"
            ));
        }
    }
    for target in observed {
        if !configured_targets.contains(&target) {
            return Err(format!(
                "unknown configured release server target `{target}`"
            ));
        }
    }
    Ok(())
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

/// Product-version admission for server subject producers (archives and the
/// server manifest). Subjects are product identity: a release-channel suffix
/// would fork archive and manifest names between RC and stable, so it fails
/// closed here. Placement travels in the installed distribution catalog.
pub(crate) fn normalize_product_version(version: &str) -> Result<String, String> {
    let version = normalize_release_version(version);
    if version.is_empty() {
        return Err("release product version must not be empty".to_string());
    }
    if version.contains('-') {
        return Err(format!(
            "release product version `{version}` must not carry a release-channel suffix; subjects are product identity and placement travels in the installed catalog"
        ));
    }
    if !version.chars().all(|character| {
        character.is_ascii_alphanumeric()
            || character == '.'
            || character == '+'
            || character == '_'
    }) {
        return Err(format!(
            "release product version `{version}` contains unsupported characters"
        ));
    }
    Ok(version)
}

/// Canonical configured server target set. One definition feeds validation,
/// the manifest `target_set` identity, and the distribution generation.
pub(crate) fn release_server_target_set() -> [&'static str; 5] {
    [
        "x86_64-pc-windows-msvc",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
    ]
}

pub(crate) fn release_server_target_set_digest() -> String {
    let mut targets = release_server_target_set().to_vec();
    targets.sort_unstable();
    sha256_bytes(targets.join("\n").as_bytes())
}

/// Content-derived distribution generation: identical candidate bytes always
/// yield the same generation across RC and stable placements, and any new
/// candidate yields a new generation. Byte equality is still enforced by
/// per-artifact digests, never by this label alone.
pub(crate) fn release_distribution_generation(
    product_version: &str,
    candidate_sha: &str,
    candidate_tree: &str,
    target_set_digest: &str,
) -> String {
    sha256_bytes(
        format!(
            "server-manifest/2|{product_version}|{candidate_sha}|{candidate_tree}|{target_set_digest}"
        )
        .as_bytes(),
    )
}

fn copy_release_file(file_name: &str, package_dir: &Path) -> Result<(), String> {
    fs::copy(file_name, package_dir.join(file_name)).map_err(|err| {
        format!(
            "failed to copy {file_name} into {}: {err}",
            package_dir.display()
        )
    })?;
    Ok(())
}

pub(crate) fn release_server_readme(version: &str) -> String {
    format!(
        "ripr server {version}\n\nThis archive contains the ripr executable used by the VS Code/Open VSX\nextension. It is distributed under MIT OR Apache-2.0."
    )
}

pub(crate) fn create_tar_gz_archive(package_dir: &Path, asset_path: &Path) -> Result<(), String> {
    let output = fs::File::create(asset_path)
        .map_err(|err| format!("failed to create {}: {err}", asset_path.display()))?;
    let encoder = GzBuilder::new()
        .mtime(0)
        .write(output, Compression::default());
    let mut builder = Builder::new(encoder);
    for path in sorted_package_files(package_dir)? {
        let name = path
            .file_name()
            .ok_or_else(|| format!("invalid package path {}", path.display()))?;
        let mut header = tar::Header::new_gnu();
        let metadata = fs::metadata(&path)
            .map_err(|err| format!("failed to stat {}: {err}", path.display()))?;
        header.set_size(metadata.len());
        header.set_mode(package_mode(name.to_string_lossy().as_ref()));
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header
            .set_username("")
            .map_err(|err| format!("set tar username: {err}"))?;
        header
            .set_groupname("")
            .map_err(|err| format!("set tar groupname: {err}"))?;
        header.set_cksum();
        let mut input = fs::File::open(&path)
            .map_err(|err| format!("failed to open {} for tar: {err}", path.display()))?;
        builder
            .append_data(&mut header, name, &mut input)
            .map_err(|err| format!("failed to write tar entry {}: {err}", path.display()))?;
    }
    let encoder = builder
        .into_inner()
        .map_err(|err| format!("failed to finalize tar {}: {err}", asset_path.display()))?;
    encoder
        .finish()
        .map_err(|err| format!("failed to finalize gzip {}: {err}", asset_path.display()))?;
    Ok(())
}

pub(crate) fn create_zip_archive(package_dir: &Path, asset_path: &Path) -> Result<(), String> {
    let file = fs::File::create(asset_path)
        .map_err(|err| format!("failed to create {}: {err}", asset_path.display()))?;
    let mut writer = zip::ZipWriter::new(file);
    // Retain Unix permission metadata on every host, including Windows, so
    // archive readback preserves the executable role's mode.
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .system(zip::System::Unix);

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
        let entry_options = options.unix_permissions(package_mode(&file_name));
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

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ReleaseServerBuildReceipt {
    schema_version: String,
    repository: String,
    candidate_sha: String,
    candidate_tree: String,
    toolchain: ReleaseServerToolchain,
    toolchain_file_sha256: String,
    cargo_lock_sha256: String,
    profile: String,
    features: Vec<String>,
    locked: bool,
    build_command: String,
    version: String,
    target: String,
    archive_format: String,
    executable: ReleaseServerFile,
    archive: ReleaseServerFile,
    members: Vec<ReleaseServerMember>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ReleaseServerToolchain {
    rustc: String,
    cargo: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ReleaseServerFile {
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize, Clone)]
struct ReleaseServerMember {
    path: String,
    kind: String,
    role: String,
    size: u64,
    sha256: String,
    mode: u32,
}

fn write_release_server_receipt(
    version: &str,
    target: &str,
    executable: &str,
    archive: &str,
    package_dir: &Path,
    archive_path: &Path,
) -> Result<(), String> {
    let executable_path = package_dir.join(executable);
    let executable_file = release_server_file(executable, &executable_path)?;
    let archive_file = release_server_file(
        archive_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("invalid archive path {}", archive_path.display()))?,
        archive_path,
    )?;
    let members = archive_member_inventory(archive_path, archive, executable)?;
    let identity = release_server_build_identity()?;
    let receipt = ReleaseServerBuildReceipt {
        schema_version: "0.2".to_string(),
        repository: identity.repository,
        candidate_sha: identity.candidate_sha,
        candidate_tree: identity.candidate_tree,
        toolchain: identity.toolchain,
        toolchain_file_sha256: identity.toolchain_file_sha256,
        cargo_lock_sha256: identity.cargo_lock_sha256,
        profile: "release".to_string(),
        features: Vec::new(),
        locked: true,
        build_command: format!("cargo build -p ripr --release --locked --target {target}"),
        version: version.to_string(),
        target: target.to_string(),
        archive_format: archive.to_string(),
        executable: executable_file,
        archive: archive_file,
        members,
    };
    let receipt_path =
        Path::new("dist").join(format!("ripr-server-v{version}-{target}.receipt.json"));
    if let Some(parent) = receipt_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&receipt)
        .map_err(|err| format!("failed to render release server receipt: {err}"))?;
    fs::write(&receipt_path, format!("{text}\n"))
        .map_err(|err| format!("failed to write {}: {err}", receipt_path.display()))?;
    eprintln!("wrote {}", receipt_path.display());
    Ok(())
}

struct ReleaseServerBuildIdentity {
    repository: String,
    candidate_sha: String,
    candidate_tree: String,
    toolchain: ReleaseServerToolchain,
    toolchain_file_sha256: String,
    cargo_lock_sha256: String,
}

fn release_server_build_identity() -> Result<ReleaseServerBuildIdentity, String> {
    let candidate_sha = std::env::var("CANDIDATE_SHA")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("GITHUB_SHA")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| run_output("git", &["rev-parse", "HEAD"]).ok())
        .unwrap_or_else(|| "unavailable".to_string());
    let candidate_tree = run_output("git", &["rev-parse", "HEAD^{tree}"])
        .unwrap_or_else(|_| "unavailable".to_string())
        .trim()
        .to_string();
    let rustc = run_output("rustc", &["-vV"])
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|_| "unavailable".to_string());
    let cargo = run_output("cargo", &["--version"])
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|_| "unavailable".to_string());
    let toolchain_file = Path::new("rust-toolchain.toml");
    let lockfile = Path::new("Cargo.lock");
    let toolchain_file_sha256 = if toolchain_file.is_file() {
        sha256_file(toolchain_file)?
    } else {
        "unavailable".to_string()
    };
    let cargo_lock_sha256 = if lockfile.is_file() {
        sha256_file(lockfile)?
    } else {
        "unavailable".to_string()
    };
    Ok(ReleaseServerBuildIdentity {
        repository: std::env::var("GITHUB_REPOSITORY").unwrap_or_else(|_| "local".to_string()),
        candidate_sha: candidate_sha.trim().to_string(),
        candidate_tree,
        toolchain: ReleaseServerToolchain { rustc, cargo },
        toolchain_file_sha256,
        cargo_lock_sha256,
    })
}

fn release_server_file(path: &str, file: &Path) -> Result<ReleaseServerFile, String> {
    let metadata =
        fs::metadata(file).map_err(|err| format!("failed to stat {}: {err}", file.display()))?;
    Ok(ReleaseServerFile {
        path: path.to_string(),
        size: metadata.len(),
        sha256: sha256_file(file)?,
    })
}

fn archive_member_inventory(
    archive_path: &Path,
    archive_format: &str,
    executable: &str,
) -> Result<Vec<ReleaseServerMember>, String> {
    match archive_format {
        "zip" => {
            let file = fs::File::open(archive_path)
                .map_err(|err| format!("failed to open {}: {err}", archive_path.display()))?;
            let mut archive = zip::ZipArchive::new(file)
                .map_err(|err| format!("failed to read {}: {err}", archive_path.display()))?;
            let mut members = Vec::new();
            for index in 0..archive.len() {
                let mut entry = archive
                    .by_index(index)
                    .map_err(|err| format!("failed to read zip member {index}: {err}"))?;
                let name = entry
                    .name()
                    .map_err(|err| format!("failed to read zip member {index} name: {err}"))?
                    .trim_start_matches("./")
                    .to_string();
                if entry.is_dir() {
                    return Err(format!(
                        "release server archive contains directory `{name}`"
                    ));
                }
                let mode = entry.unix_mode().unwrap_or_else(|| package_mode(&name));
                members.push(archive_member(
                    &name,
                    entry.size(),
                    mode,
                    sha256_reader(&mut entry)
                        .map_err(|err| format!("failed to read archive member: {err}"))?,
                    executable,
                ));
            }
            validate_archive_members(&members, executable)?;
            Ok(members)
        }
        "tar.gz" => {
            let file = fs::File::open(archive_path)
                .map_err(|err| format!("failed to open {}: {err}", archive_path.display()))?;
            let decoder = GzDecoder::new(file);
            let mut archive = tar::Archive::new(decoder);
            let mut members = Vec::new();
            for entry in archive
                .entries()
                .map_err(|err| format!("failed to read tar members: {err}"))?
            {
                let mut entry = entry.map_err(|err| format!("failed to read tar member: {err}"))?;
                let kind = entry.header().entry_type();
                if !kind.is_file() {
                    return Err(format!(
                        "release server archive contains non-regular member `{}`",
                        entry
                            .path()
                            .map_err(|err| format!("read tar member path: {err}"))?
                            .display()
                    ));
                }
                let name = entry
                    .path()
                    .map_err(|err| format!("read tar member path: {err}"))?
                    .to_string_lossy()
                    .trim_start_matches("./")
                    .to_string();
                let size = entry.size();
                let mode = entry
                    .header()
                    .mode()
                    .map_err(|err| format!("read tar member mode: {err}"))?;
                let sha256 = sha256_reader(&mut entry)
                    .map_err(|err| format!("failed to read archive member: {err}"))?;
                members.push(archive_member(&name, size, mode, sha256, executable));
            }
            validate_archive_members(&members, executable)?;
            Ok(members)
        }
        other => Err(format!(
            "unsupported release server archive format `{other}`"
        )),
    }
}

fn archive_member(
    name: &str,
    size: u64,
    mode: u32,
    sha256: String,
    executable: &str,
) -> ReleaseServerMember {
    let role = if name == executable {
        "executable"
    } else if name == "LICENSE-MIT" {
        "license_mit"
    } else if name == "LICENSE-APACHE" {
        "license_apache"
    } else if name == "README-server.txt" {
        "readme"
    } else {
        "reviewed_other"
    };
    ReleaseServerMember {
        path: name.to_string(),
        kind: "regular_file".to_string(),
        role: role.to_string(),
        size,
        sha256,
        mode,
    }
}

fn validate_archive_members(
    members: &[ReleaseServerMember],
    executable: &str,
) -> Result<(), String> {
    let executable_members = members
        .iter()
        .filter(|member| member.role == "executable")
        .collect::<Vec<_>>();
    if executable_members.len() != 1 {
        return Err(format!(
            "release server archive must contain exactly one executable role, found {}",
            executable_members.len()
        ));
    }
    if executable_members[0].path != executable || executable_members[0].mode & 0o111 == 0 {
        return Err(format!(
            "release server executable `{executable}` has an invalid path or mode"
        ));
    }
    for member in members {
        if member.role != "executable" && member.mode & 0o111 != 0 {
            return Err(format!(
                "release server non-executable member `{}` has executable mode {:o}",
                member.path, member.mode
            ));
        }
    }
    Ok(())
}

fn sorted_package_files(package_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(package_dir)
        .map_err(|err| format!("failed to read {}: {err}", package_dir.display()))?
    {
        let path = entry
            .map_err(|err| format!("failed to read package entry: {err}"))?
            .path();
        if !path.is_file() {
            return Err(format!(
                "release server package must be flat: {}",
                path.display()
            ));
        }
        paths.push(path);
    }
    paths.sort();
    Ok(paths)
}

fn package_mode(file_name: &str) -> u32 {
    if file_name == "ripr" || file_name == "ripr.exe" {
        0o755
    } else {
        0o644
    }
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

#[derive(Debug, Clone)]
struct ReleaseServerReceiptSummary {
    path: String,
    sha256: String,
    schema_version: String,
    target: String,
    archive_format: String,
    archive: ReleaseServerFile,
    executable: ReleaseServerFile,
}

pub(crate) struct ReleaseServerReceiptSet {
    targets: std::collections::BTreeSet<String>,
    baseline: ReleaseServerBuildReceipt,
    receipts: std::collections::BTreeMap<String, ReleaseServerReceiptSummary>,
}

pub(crate) fn validate_release_server_receipts(
    dist_dir: &Path,
    version: &str,
) -> Result<ReleaseServerReceiptSet, String> {
    let mut baseline: Option<ReleaseServerBuildReceipt> = None;
    let mut receipt_targets = std::collections::BTreeSet::new();
    let mut receipts = std::collections::BTreeMap::new();
    for path in sorted_dist_files(dist_dir)? {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !file_name.ends_with(".receipt.json") {
            continue;
        }
        if file_name == format!("ripr-server-assembly-v{version}.receipt.json") {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|err| {
            format!(
                "failed to read release server receipt {}: {err}",
                path.display()
            )
        })?;
        let receipt = serde_json::from_str::<ReleaseServerBuildReceipt>(&text).map_err(|err| {
            format!(
                "malformed release server receipt `{}`: {err}",
                path.display()
            )
        })?;
        if receipt.schema_version != "0.2" {
            return Err(format!(
                "unsupported release server receipt schema `{}` for `{file_name}`; expected `0.2`",
                receipt.schema_version
            ));
        }
        if receipt.version != version {
            return Err(format!(
                "release server receipt version `{}` does not match requested version `{version}`",
                receipt.version
            ));
        }
        let receipt_target = file_name
            .strip_prefix(&format!("ripr-server-v{version}-"))
            .and_then(|name| name.strip_suffix(".receipt.json"))
            .ok_or_else(|| format!("invalid release server receipt name `{file_name}`"))?;
        let expected_archive = format!(
            "ripr-server-v{version}-{receipt_target}.{}",
            receipt.archive_format
        );
        validate_release_server_relative_path(&receipt.archive.path, "archive", receipt_target)?;
        validate_release_server_relative_path(
            &receipt.executable.path,
            "executable",
            receipt_target,
        )?;
        if receipt.target != receipt_target || receipt.archive.path != expected_archive {
            return Err(format!(
                "release server receipt archive mapping mismatch for target `{receipt_target}`"
            ));
        }
        if !receipt_targets.insert(receipt_target.to_string()) {
            return Err(format!(
                "duplicate release server receipt target `{receipt_target}`"
            ));
        }
        let archive_path = dist_dir.join(&receipt.archive.path);
        let archive_metadata = fs::metadata(&archive_path).map_err(|err| {
            format!(
                "release server receipt archive `{}` is unavailable: {err}",
                receipt.archive.path
            )
        })?;
        if archive_metadata.len() != receipt.archive.size {
            return Err(format!(
                "release server receipt archive size mismatch for target `{receipt_target}`"
            ));
        }
        let archive_sha = sha256_file(&archive_path)?;
        if receipt.archive.sha256 != archive_sha {
            return Err(format!(
                "release server receipt archive digest mismatch for target `{receipt_target}`"
            ));
        }
        let actual_members = archive_member_inventory(
            &archive_path,
            &receipt.archive_format,
            &receipt.executable.path,
        )?;
        if receipt.members != actual_members {
            return Err(format!(
                "release server receipt member inventory mismatch for target `{receipt_target}`"
            ));
        }
        receipts.insert(
            receipt_target.to_string(),
            ReleaseServerReceiptSummary {
                path: file_name.to_string(),
                sha256: sha256_file(&path)?,
                schema_version: receipt.schema_version.clone(),
                target: receipt.target.clone(),
                archive_format: receipt.archive_format.clone(),
                archive: receipt.archive.clone(),
                executable: receipt.executable.clone(),
            },
        );
        if let Some(expected) = &baseline {
            if receipt.repository != expected.repository {
                return Err(format!(
                    "release server receipt repository mismatch for target `{}`",
                    receipt.target
                ));
            }
            if receipt.candidate_sha != expected.candidate_sha {
                return Err(format!(
                    "release server receipt candidate SHA mismatch for target `{}`",
                    receipt.target
                ));
            }
            if receipt.candidate_tree != expected.candidate_tree {
                return Err(format!(
                    "release server receipt candidate tree mismatch for target `{}`",
                    receipt.target
                ));
            }
            if rustc_toolchain_identity(&receipt.toolchain.rustc)?
                != rustc_toolchain_identity(&expected.toolchain.rustc)?
                || receipt.toolchain.cargo != expected.toolchain.cargo
            {
                return Err(format!(
                    "release server receipt toolchain mismatch for target `{}`",
                    receipt.target
                ));
            }
            if receipt.toolchain_file_sha256 != expected.toolchain_file_sha256 {
                return Err(format!(
                    "release server receipt toolchain-file identity mismatch for target `{}`",
                    receipt.target
                ));
            }
            if receipt.cargo_lock_sha256 != expected.cargo_lock_sha256 {
                return Err(format!(
                    "release server receipt Cargo.lock identity mismatch for target `{}`",
                    receipt.target
                ));
            }
            if receipt.profile != expected.profile
                || receipt.features != expected.features
                || receipt.locked != expected.locked
            {
                return Err(format!(
                    "release server receipt build contract mismatch for target `{}`",
                    receipt.target
                ));
            }
        } else {
            baseline = Some(receipt);
        }
    }
    let baseline = baseline.ok_or_else(|| "no release server receipts found".to_string())?;
    Ok(ReleaseServerReceiptSet {
        targets: receipt_targets,
        baseline,
        receipts,
    })
}

fn validate_release_server_relative_path(
    value: &str,
    field: &str,
    target: &str,
) -> Result<(), String> {
    let path = Path::new(value);
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(std::path::Component::Normal(_)), None)
            if !value.contains('/') && !value.contains('\\') =>
        {
            Ok(())
        }
        _ => Err(format!(
            "release server receipt {field} path is not a safe staging file for target `{target}`: `{value}`"
        )),
    }
}

fn rustc_toolchain_identity(value: &str) -> Result<(String, String), String> {
    let mut release = None;
    let mut commit_hash = None;
    for line in value.lines() {
        let Some((key, field)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "release" => release = Some(field.trim().to_string()),
            "commit-hash" => commit_hash = Some(field.trim().to_string()),
            _ => {}
        }
    }
    match (release, commit_hash) {
        (Some(release), Some(commit_hash)) => Ok((release, commit_hash)),
        _ => Err("rustc toolchain identity is missing release or commit-hash".to_string()),
    }
}

pub(crate) fn validate_release_server_staging_inventory(
    dist_dir: &Path,
    version: &str,
    assets: &[ReleaseServerAsset],
    receipt_targets: &std::collections::BTreeSet<String>,
) -> Result<(), String> {
    let mut allowed = std::collections::BTreeSet::new();
    for asset in assets {
        allowed.insert(asset.file_name.clone());
        allowed.insert(format!("{}.sha256", asset.file_name));
    }
    for target in receipt_targets {
        allowed.insert(format!("ripr-server-v{version}-{target}.receipt.json"));
    }
    allowed.insert("SHA256SUMS".to_string());
    allowed.insert(format!("ripr-server-manifest-v{version}.json"));
    allowed.insert(format!("ripr-server-assembly-v{version}.receipt.json"));
    for path in sorted_dist_files(dist_dir)? {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !allowed.contains(file_name) {
            return Err(format!(
                "unrecognized staged release server file `{file_name}`"
            ));
        }
    }
    Ok(())
}

pub(crate) fn write_release_server_outputs_transactional(
    outputs: &[(&Path, &str)],
) -> Result<(), String> {
    let transaction_id = format!("{}-{}", std::process::id(), outputs.len());
    let mut staged = Vec::with_capacity(outputs.len());
    for (index, (path, contents)) in outputs.iter().enumerate() {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("invalid release server output path {}", path.display()))?;
        let temporary = path.with_file_name(format!(".{file_name}.{transaction_id}.{index}.tmp"));
        if let Err(error) = fs::write(&temporary, contents) {
            for (staged_path, _) in &staged {
                let _ = fs::remove_file(staged_path);
            }
            let _ = fs::remove_file(&temporary);
            return Err(format!("failed to stage {}: {error}", path.display()));
        }
        staged.push((temporary, (*path).to_path_buf()));
    }

    let mut backups = Vec::new();
    let mut installed = Vec::new();
    let result = (|| {
        for (index, (_, destination)) in staged.iter().enumerate() {
            if destination.exists() {
                let backup = destination.with_file_name(format!(".{transaction_id}.{index}.bak"));
                fs::rename(destination, &backup).map_err(|error| {
                    format!("failed to preserve {}: {error}", destination.display())
                })?;
                backups.push((backup, destination.clone()));
            }
        }
        for (temporary, destination) in &staged {
            fs::rename(temporary, destination)
                .map_err(|error| format!("failed to install {}: {error}", destination.display()))?;
            installed.push(destination.clone());
        }
        Ok(())
    })();

    if let Err(error) = result {
        for destination in installed {
            let _ = fs::remove_file(destination);
        }
        for (backup, destination) in backups.iter().rev() {
            let _ = fs::rename(backup, destination);
        }
        for (temporary, _) in &staged {
            let _ = fs::remove_file(temporary);
        }
        return Err(error);
    }
    for (backup, _) in backups {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

pub(crate) fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
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
