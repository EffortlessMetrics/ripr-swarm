//! Actual Cargo producer, archive inventory, install and executable custody.
use super::{
    AdmittedSource,
    input::{CapturedFile, capture_retained_executable_with, revalidate_retained_file},
    safe_artifact_path,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(test)]
mod inventory_tests;
mod producer;
use producer::{InstallProducer, InstallToolchain};

/// Archive custody starts at an observed successful Cargo producer, never a
/// caller's archive/version string. Ordinary entries retain raw Git attribution.
pub(crate) struct AttributedArchive {
    source: AdmittedSource,
    path: PathBuf,
    bytes: Vec<u8>,
    inventory: BTreeMap<String, ArchiveFile>,
    staged: bool,
}

struct ArchiveFile {
    bytes: Vec<u8>,
    mode: u32,
}

impl AttributedArchive {
    pub(crate) fn produce(source: AdmittedSource, owned_root: &Path) -> Result<Self, String> {
        source.revalidate()?;
        let target = owned_root.join("package-target");
        // The published `ripr` is packaged from a publish stage (#14198), so
        // the qualified archive is too (#15204): the stage is a clone of the
        // admitted commit with inline test modules stripped, and the archive
        // inventory below accepts only committed blobs or their exact strip.
        let stage = publish_stage_for(&source, owned_root)?;
        let workspace = stage.as_deref().unwrap_or_else(|| source.root());
        let manifest = workspace.join("Cargo.toml");
        let config = owned_cargo_config(owned_root, "package", None)?;
        let mut args = vec![
            "--config".to_string(),
            config.to_string_lossy().into_owned(),
            "package".to_string(),
            "--jobs".to_string(),
            "2".to_string(),
            "--locked".to_string(),
            "--manifest-path".to_string(),
            manifest.to_string_lossy().into_owned(),
            "--target-dir".to_string(),
            target.to_string_lossy().into_owned(),
        ];
        // Path dependencies such as `ripr-contract` are not on a registry
        // before the release, so they are packaged in the same invocation and
        // Cargo verifies the candidate against them.
        for package in packaged_packages(&source)? {
            args.push("--package".to_string());
            args.push(package);
        }
        qualified_command(
            Path::new("cargo"),
            &args,
            workspace,
            Duration::from_mins(15),
            "qualified cargo package",
        )?;
        source.revalidate()?;
        if let Some(stage) = &stage {
            crate::publish_stage::verify_stage(stage)?;
        }
        let path = target.join("package").join(format!(
            "{}-{}.crate",
            source.package_name(),
            source.version()
        ));
        let bytes =
            std::fs::read(&path).map_err(|error| format!("read produced archive: {error}"))?;
        let inventory = archive_inventory(&bytes, &ArchiveRules::of(&source)?)?;
        let archive = Self {
            source,
            path,
            bytes,
            inventory,
            staged: stage.is_some(),
        };
        archive.revalidate()?;
        Ok(archive)
    }

    pub(crate) fn revalidate(&self) -> Result<(), String> {
        self.source.revalidate()?;
        revalidate_retained_file(
            &self.path,
            &self.bytes,
            "produced archive bytes changed after attribution",
        )
    }

    #[cfg(test)]
    pub(crate) fn archive_path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn install(self, owned_root: &Path) -> Result<InstalledCandidate, String> {
        self.revalidate()?;
        // Cargo 1.95 stops workspace ancestor discovery at target/package.
        // Keep attributed manifests unchanged while isolating this owned extraction.
        let package_boundary = owned_root.join("target/package");
        std::fs::create_dir_all(&package_boundary)
            .map_err(|error| format!("create owned Cargo package boundary: {error}"))?;
        let extracted = package_boundary.join("extracted");
        std::fs::create_dir(&extracted)
            .map_err(|error| format!("create owned extraction root: {error}"))?;
        for (path, file) in &self.inventory {
            let output = extracted.join(path);
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create owned package directory: {error}"))?;
            }
            std::fs::write(&output, &file.bytes)
                .map_err(|error| format!("write attributed package entry: {error}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&output, std::fs::Permissions::from_mode(file.mode))
                    .map_err(|error| format!("restore attributed package permissions: {error}"))?;
            }
            #[cfg(not(unix))]
            let _mode = file.mode;
        }
        let install_root = owned_root.join("installed");
        let target_root = owned_root.join("install-target");
        let toolchain = InstallToolchain::observe(&extracted)?;
        let config = owned_cargo_config(owned_root, "install", Some(&toolchain))?;
        let args = vec![
            "--config".to_string(),
            config.to_string_lossy().into_owned(),
            "install".to_string(),
            "--message-format=json".to_string(),
            "--jobs".to_string(),
            "2".to_string(),
            "--path".to_string(),
            extracted.to_string_lossy().into_owned(),
            "--locked".to_string(),
            "--offline".to_string(),
            "--root".to_string(),
            install_root.to_string_lossy().into_owned(),
            "--target-dir".to_string(),
            target_root.to_string_lossy().into_owned(),
        ];
        let output = qualified_command_with_environment(
            Path::new("cargo"),
            &args,
            &extracted,
            Duration::from_mins(15),
            "qualified cargo install",
            &toolchain.environment(),
        )?;
        toolchain.revalidate(&extracted)?;
        self.revalidate()?;
        revalidate_extracted_files(&extracted, &self.inventory)?;
        let binary = install_root.join("bin").join(format!(
            "{}{}",
            self.source.package_name(),
            std::env::consts::EXE_SUFFIX
        ));
        let executable_bytes = std::fs::read(&binary)
            .map_err(|error| format!("read installed candidate executable: {error}"))?;
        if executable_bytes.is_empty() {
            return Err("installed candidate executable is empty".to_string());
        }
        let producer = InstallProducer::from_output(
            &output.stdout,
            &extracted,
            &target_root,
            self.source.package_name(),
            toolchain,
            &executable_bytes,
        )?;
        let installed = InstalledCandidate {
            archive: self,
            installed_binary: binary.clone(),
            binary,
            executable_bytes,
            producer,
            capture_root: owned_root
                .canonicalize()
                .map_err(|error| format!("resolve owned capture root: {error}"))?,
            captured: None,
        };
        installed.revalidate()?;
        Ok(installed)
    }
}

fn revalidate_extracted_files(
    root: &Path,
    inventory: &BTreeMap<String, ArchiveFile>,
) -> Result<(), String> {
    for (path, file) in inventory {
        revalidate_retained_file(
            &root.join(path),
            &file.bytes,
            "attributed extraction bytes changed during installation",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OwnedExtraction(PathBuf);

    impl Drop for OwnedExtraction {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn extracted_install_inputs_refuse_growth_at_each_retained_length() -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-extraction-custody-{}-{stamp}",
            std::process::id()
        ));
        if let Some(parent) = root.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let fixture = OwnedExtraction(root);
        let mut inventory = BTreeMap::new();
        for (path, bytes) in [("empty", b"".as_slice()), ("ordinary", b"1234".as_slice())] {
            std::fs::write(fixture.0.join(path), bytes).map_err(|error| error.to_string())?;
            inventory.insert(
                path.to_string(),
                ArchiveFile {
                    bytes: bytes.to_vec(),
                    mode: 0o644,
                },
            );
        }
        revalidate_extracted_files(&fixture.0, &inventory)?;
        std::fs::write(fixture.0.join("ordinary"), b"12345").map_err(|error| error.to_string())?;
        match revalidate_extracted_files(&fixture.0, &inventory) {
            Err(error)
                if error.contains("attributed extraction bytes changed during installation")
                    && error.contains("exceeds its 4-byte budget") => {}
            result => return Err(format!("wrong extracted-input growth refusal: {result:?}")),
        }
        std::fs::remove_dir_all(&fixture.0)
            .map_err(|error| format!("extracted-input fixture cleanup: {error}"))
    }
}

pub(crate) struct InstalledCandidate {
    archive: AttributedArchive,
    binary: PathBuf,
    executable_bytes: Vec<u8>,
    installed_binary: PathBuf,
    producer: InstallProducer,
    capture_root: PathBuf,
    captured: Option<CapturedFile>,
}

/// Shared authentic-chain semantics, with qualified custody enforced at each
/// producer invocation. The legacy caller retains its existing process mode.
#[derive(Clone, Copy)]
pub(crate) enum CandidateExecution<'a> {
    Legacy(&'a Path),
    Qualified(&'a InstalledCandidate),
}

impl CandidateExecution<'_> {
    pub(crate) fn fixture_git(
        self,
        root: &Path,
        args: &[&str],
        context: &str,
    ) -> Result<super::super::CommandResult, String> {
        match self {
            Self::Legacy(_) => super::super::run_fixture_git_command(root, args, context),
            Self::Qualified(candidate) => {
                candidate.revalidate()?;
                let args = args
                    .iter()
                    .map(|arg| (*arg).to_string())
                    .collect::<Vec<_>>();
                let output = qualified_command(
                    Path::new("git"),
                    &args,
                    root,
                    Duration::from_secs(30),
                    context,
                )?;
                candidate.revalidate()?;
                let status = output
                    .status
                    .ok_or_else(|| format!("{context} has no process status"))?;
                Ok(super::super::CommandResult {
                    status: status.code(),
                    success: status.success(),
                    stdout: String::from_utf8(output.stdout)
                        .map_err(|error| format!("{context} stdout UTF-8: {error}"))?,
                    stderr: String::from_utf8(output.stderr)
                        .map_err(|error| format!("{context} stderr UTF-8: {error}"))?,
                })
            }
        }
    }
    pub(crate) fn fixture_head(self, root: &Path) -> Result<String, String> {
        if matches!(self, Self::Legacy(_)) {
            return super::super::fixture_head(root);
        }
        let result = self.fixture_git(root, &["rev-parse", "HEAD"], "read fixture HEAD")?;
        let head = result.stdout.trim();
        if head.len() != 40 || !head.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("fixture HEAD is not a full commit SHA: {head}"));
        }
        Ok(head.to_string())
    }
    pub(crate) fn fixture_checkout(self, root: &Path, sha: &str) -> Result<(), String> {
        if matches!(self, Self::Legacy(_)) {
            return super::super::checkout_fixture_commit(root, sha);
        }
        self.fixture_git(
            root,
            &["checkout", "--quiet", "--detach", sha],
            "checkout fixture commit",
        )?;
        Ok(())
    }
    pub(crate) fn run(
        self,
        args: &[String],
        cwd: &Path,
        context: &str,
    ) -> Result<super::super::CommandResult, String> {
        match self {
            Self::Legacy(binary) => super::super::run_command_in_dir(binary, args, cwd, context),
            Self::Qualified(candidate) => {
                candidate.revalidate()?;
                let output = crate::run::capture_bytes_in_dir_with_timeout(
                    candidate.binary(),
                    args,
                    cwd,
                    &[],
                    &[],
                    Duration::from_mins(2),
                    context,
                )?;
                candidate.revalidate()?;
                if output.timed_out {
                    return Err(format!("{context} exceeded qualified process deadline"));
                }
                let status = output
                    .status
                    .ok_or_else(|| format!("{context} has no process status"))?;
                Ok(super::super::CommandResult {
                    status: status.code(),
                    success: status.success(),
                    stdout: String::from_utf8(output.stdout)
                        .map_err(|error| format!("{context} stdout UTF-8: {error}"))?,
                    stderr: String::from_utf8(output.stderr)
                        .map_err(|error| format!("{context} stderr UTF-8: {error}"))?,
                })
            }
        }
    }
}

impl InstalledCandidate {
    pub(crate) fn fixture_bytes(&self, relative: &str) -> Result<Vec<u8>, String> {
        self.revalidate()?;
        let path = format!("fixtures/boundary_gap/input/{relative}");
        let bytes = self
            .archive
            .source
            .committed_file(&path)
            .ok_or_else(|| format!("admitted source lacks authentic fixture {path}"))?
            .to_vec();
        self.revalidate()?;
        Ok(bytes)
    }
    pub(crate) fn revalidate(&self) -> Result<(), String> {
        self.archive.revalidate()?;
        if let Some(captured) = &self.captured {
            return captured.revalidate(&self.executable_bytes);
        }
        self.producer.revalidate(&self.executable_bytes)?;
        revalidate_retained_file(
            &self.binary,
            &self.executable_bytes,
            "installed executable bytes changed after custody capture",
        )
    }
    pub(crate) fn binary(&self) -> &Path {
        &self.binary
    }
    /// Transition this admitted handle once. Reusing its qualified execution
    /// reference afterward never recopies mutable compiler/install output.
    pub(crate) fn freeze_into(&mut self, slot: &Path) -> Result<(), String> {
        self.freeze_with(slot, |file, bytes| file.write_all(bytes))
    }

    fn freeze_with(
        &mut self,
        slot: &Path,
        writer: impl FnOnce(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
    ) -> Result<(), String> {
        let captured = self.capture_with(slot, || Ok(()), writer)?;
        self.binary = slot.to_path_buf();
        self.captured = Some(captured);
        Ok(())
    }

    fn capture_with(
        &self,
        slot: &Path,
        before_create: impl FnOnce() -> Result<(), String>,
        writer: impl FnOnce(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
    ) -> Result<CapturedFile, String> {
        if self.captured.is_some() {
            return Err(
                "candidate executable already frozen; reuse its retained reference".to_string(),
            );
        }
        if !slot.is_absolute()
            || slot.parent() != Some(self.capture_root.as_path())
            || !slot
                .file_name()
                .is_some_and(|name| safe_artifact_path(Path::new(name)))
            || self.capture_root.canonicalize().ok().as_ref() != Some(&self.capture_root)
        {
            return Err(
                "capture destination must be an ordinary file in the owned producer root"
                    .to_string(),
            );
        }
        self.revalidate()?;
        let permissions = std::fs::metadata(&self.installed_binary)
            .map_err(|error| format!("read installed executable permissions: {error}"))?
            .permissions();
        capture_retained_executable_with(
            &self.executable_bytes,
            permissions,
            slot,
            before_create,
            |file, bytes| {
                writer(file, bytes)?;
                self.archive.revalidate().map_err(std::io::Error::other)
            },
        )
    }

    #[cfg(test)]
    pub(crate) fn freeze_into_with_writer_for_test(
        &mut self,
        slot: &Path,
        writer: impl FnOnce(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
    ) -> Result<(), String> {
        self.freeze_with(slot, writer)
    }

    #[cfg(test)]
    pub(crate) fn capture_unpublished_with_gate_for_test(
        &self,
        slot: &Path,
        before_create: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let captured =
            self.capture_with(slot, before_create, |file, bytes| file.write_all(bytes))?;
        captured.revalidate(&self.executable_bytes)
    }

    pub(crate) fn custody_json(&self) -> serde_json::Value {
        serde_json::json!({"source": self.archive.source.custody_json(),
            "archive_sha256": format!("{:x}", Sha256::digest(&self.archive.bytes)),
            "executable_sha256": format!("{:x}", Sha256::digest(&self.executable_bytes)),
            "archive": self.archive.path, "executable": self.binary,
            "archive_source": if self.archive.staged { "publish_stage" } else { "admitted_checkout" },
            "archive_entry_contract": "ordinary entries are committed blobs, or for the stripped source root exactly the inline-test strip of one; a dirty Cargo VCS record requires the stage marker naming the admitted commit",
            "installed_executable": self.installed_binary,
            "compiler_artifact": self.producer.artifact,
            "compiler_installed_bytes_equal_at_capture": true,
            "rustc_command": self.producer.toolchain.rustc_command,
            "rustc_verbose_version": self.producer.toolchain.rustc_verbose_version,
            "cargo_verbose_version": self.producer.toolchain.cargo_verbose_version,
            "capture_mode": if self.captured.is_some() { "exclusive_frozen_reference" } else { "installed_output" },
            "claim": "unlocked byte custody checks; not authenticated or atomic provenance"})
    }
}

fn owned_cargo_config(
    root: &Path,
    phase: &str,
    toolchain: Option<&InstallToolchain>,
) -> Result<PathBuf, String> {
    let temporary = root.join(format!("{phase}-temporary"));
    std::fs::create_dir(&temporary)
        .map_err(|error| format!("create owned Cargo temporary directory: {error}"))?;
    let temporary = temporary
        .canonicalize()
        .map_err(|error| format!("resolve owned Cargo temporary directory: {error}"))?;
    let value = temporary
        .to_str()
        .ok_or_else(|| "owned Cargo temporary path is not UTF-8".to_string())?;
    let mut environment = toml::map::Map::new();
    for name in ["TEMP", "TMP", "TMPDIR"] {
        let mut setting = toml::map::Map::new();
        setting.insert("value".to_string(), toml::Value::String(value.to_string()));
        setting.insert("force".to_string(), toml::Value::Boolean(true));
        setting.insert("relative".to_string(), toml::Value::Boolean(false));
        environment.insert(name.to_string(), toml::Value::Table(setting));
    }
    let mut config = toml::map::Map::new();
    config.insert("env".to_string(), toml::Value::Table(environment));
    if let Some(toolchain) = toolchain {
        toolchain.configure(&mut config)?;
    }
    let path = root.join(format!("{phase}-cargo-config.toml"));
    std::fs::write(
        &path,
        toml::to_string(&toml::Value::Table(config))
            .map_err(|error| format!("encode owned Cargo configuration: {error}"))?,
    )
    .map_err(|error| format!("write owned Cargo configuration: {error}"))?;
    Ok(path)
}

fn qualified_command(
    program: &Path,
    args: &[String],
    cwd: &Path,
    timeout: Duration,
    context: &str,
) -> Result<crate::run::TimedBytesOutput, String> {
    qualified_command_with_environment(program, args, cwd, timeout, context, &[])
}

fn qualified_command_with_environment(
    program: &Path,
    args: &[String],
    cwd: &Path,
    timeout: Duration,
    context: &str,
    environment: &[(&str, &str)],
) -> Result<crate::run::TimedBytesOutput, String> {
    let output = crate::run::capture_bytes_in_dir_with_timeout(
        program,
        args,
        cwd,
        environment,
        &[
            "CARGO_TARGET_DIR",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
        ],
        timeout,
        context,
    )?;
    if output.timed_out {
        return Err(format!("{context} exceeded owned process deadline"));
    }
    if !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "{context} failed with native status {:?}: {}",
            output.status.and_then(|status| status.code()),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output)
}

/// Stage the admitted commit for packaging when its stripped source root holds
/// inline test modules; otherwise package the admitted checkout itself.
fn publish_stage_for(
    source: &AdmittedSource,
    owned_root: &Path,
) -> Result<Option<PathBuf>, String> {
    let root = format!("{}/", crate::publish_stage::STRIPPED_SOURCE_ROOT);
    let mut strippable = false;
    for (path, blob) in source.committed_blobs() {
        if !(path.starts_with(&root) && path.ends_with(".rs")) {
            continue;
        }
        let text = std::str::from_utf8(blob)
            .map_err(|error| format!("committed {path} is not UTF-8: {error}"))?;
        let stripped = crate::publish_stage::strip_inline_test_modules(text)
            .map_err(|error| format!("committed {path}: {error}"))?;
        strippable |= stripped.modules > 0;
    }
    if !strippable {
        return Ok(None);
    }
    if source.package_prefix() != "crates/ripr/" {
        return Err(format!(
            "inline test modules under {root} belong to a package outside crates/ripr/"
        ));
    }
    let stage = owned_root.join("publish-stage");
    let report = crate::publish_stage::stage_workspace(source.root(), &stage)?;
    if report.commit != source.source_sha()? {
        return Err("publish stage commit differs from admitted source".to_string());
    }
    Ok(Some(stage))
}

/// The candidate package and its committed path dependencies, for one
/// multi-package `cargo package`.
fn packaged_packages(source: &AdmittedSource) -> Result<Vec<String>, String> {
    let manifest_path = format!("{}Cargo.toml", source.package_prefix());
    let manifest = source
        .committed_file(&manifest_path)
        .ok_or_else(|| "candidate source lacks committed package Cargo.toml".to_string())?;
    let manifest: toml::Value = toml::from_str(
        std::str::from_utf8(manifest)
            .map_err(|error| format!("package manifest UTF-8: {error}"))?,
    )
    .map_err(|error| format!("package manifest TOML: {error}"))?;
    let mut packages = std::collections::BTreeSet::new();
    for table in ["dependencies", "build-dependencies"] {
        let Some(dependencies) = manifest.get(table).and_then(toml::Value::as_table) else {
            continue;
        };
        for (key, value) in dependencies {
            if value.get("path").is_some() {
                let name = value
                    .get("package")
                    .and_then(toml::Value::as_str)
                    .unwrap_or(key);
                packages.insert(name.to_string());
            }
        }
    }
    packages.insert(source.package_name().to_string());
    Ok(packages.into_iter().collect())
}

/// What a produced archive may hold, taken only from the admitted source.
struct ArchiveRules<'a> {
    package_name: &'a str,
    version: &'a str,
    source_sha: &'a str,
    package_prefix: &'a str,
    blobs: &'a BTreeMap<String, Vec<u8>>,
}

impl<'a> ArchiveRules<'a> {
    fn of(source: &'a AdmittedSource) -> Result<Self, String> {
        Ok(Self {
            package_name: source.package_name(),
            version: source.version(),
            source_sha: source.source_sha()?,
            package_prefix: source.package_prefix(),
            blobs: source.committed_blobs(),
        })
    }

    fn committed(&self, path: &str) -> Option<&[u8]> {
        self.blobs.get(path).map(Vec::as_slice)
    }

    /// True when `body` is exactly the publish-stage strip of the committed
    /// blob at `committed_path` and that strip removed at least one module.
    fn is_committed_strip(&self, committed_path: &str, body: &[u8]) -> bool {
        let root = format!("{}/", crate::publish_stage::STRIPPED_SOURCE_ROOT);
        if !(committed_path.starts_with(&root) && committed_path.ends_with(".rs")) {
            return false;
        }
        let Some(text) = self
            .committed(committed_path)
            .and_then(|blob| std::str::from_utf8(blob).ok())
        else {
            return false;
        };
        crate::publish_stage::strip_inline_test_modules(text)
            .is_ok_and(|stripped| stripped.modules > 0 && stripped.text.as_bytes() == body)
    }
}

fn archive_inventory(
    bytes: &[u8],
    rules: &ArchiveRules<'_>,
) -> Result<BTreeMap<String, ArchiveFile>, String> {
    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    let prefix = format!("{}-{}", rules.package_name, rules.version);
    let expected_root = Path::new(&prefix);
    let mut inventory = BTreeMap::new();
    let mut vcs_dirty = false;
    let mut stripped_entries = 0_usize;
    for entry in archive
        .entries()
        .map_err(|error| format!("read produced archive entries: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("read produced archive entry: {error}"))?;
        let path = entry
            .path()
            .map_err(|error| format!("produced archive path: {error}"))?
            .into_owned();
        super::super::validate_package_entry(&path, entry.header().entry_type(), expected_root)?;
        if entry.header().entry_type().is_dir() {
            continue;
        }
        if !entry.header().entry_type().is_file() {
            return Err("unsupported produced archive entry type".to_string());
        }
        let relative = path
            .strip_prefix(expected_root)
            .map_err(|error| format!("archive root: {error}"))?;
        if !safe_artifact_path(relative) {
            return Err("unsupported archive relative path".to_string());
        }
        let relative = relative
            .to_str()
            .ok_or_else(|| "archive relative path UTF-8".to_string())?
            .replace('\\', "/");
        let mode = entry
            .header()
            .mode()
            .map_err(|error| format!("archive file mode: {error}"))?;
        if mode & !0o777 != 0 {
            return Err("unsupported archive file permission bits".to_string());
        }
        let mut body = Vec::new();
        entry
            .read_to_end(&mut body)
            .map_err(|error| format!("read produced entry bytes: {error}"))?;
        let committed_path = format!("{}{relative}", rules.package_prefix);
        match relative.as_str() {
            ".cargo_vcs_info.json" => {
                let vcs: serde_json::Value = serde_json::from_slice(&body)
                    .map_err(|error| format!("Cargo VCS info: {error}"))?;
                // Cargo may omit false. A present dirty field must be a
                // boolean; absence is accepted, not a provenance claim. A
                // dirty record is accepted below only with the stage marker.
                vcs_dirty = match vcs.pointer("/git/dirty") {
                    None => false,
                    Some(value) => value.as_bool().ok_or_else(|| {
                        "produced Cargo VCS identity differs from admitted source".to_string()
                    })?,
                };
                if vcs.pointer("/git/sha1").and_then(serde_json::Value::as_str)
                    != Some(rules.source_sha)
                    || vcs.get("path_in_vcs").and_then(serde_json::Value::as_str)
                        != Some(rules.package_prefix.trim_end_matches('/'))
                {
                    return Err(
                        "produced Cargo VCS identity differs from admitted source".to_string()
                    );
                }
            }
            crate::publish_stage::PUBLISH_STAGE_MARKER => {
                if body != format!("{}\n", rules.source_sha).as_bytes() {
                    return Err(
                        "publish-stage marker does not name the admitted commit".to_string()
                    );
                }
            }
            "Cargo.toml.orig" => {
                if rules.committed(&format!("{}Cargo.toml", rules.package_prefix))
                    != Some(body.as_slice())
                {
                    return Err(
                        "Cargo original manifest differs from committed package manifest"
                            .to_string(),
                    );
                }
            }
            "Cargo.toml" => {
                let manifest: toml::Value = toml::from_str(
                    std::str::from_utf8(&body)
                        .map_err(|error| format!("packaged manifest UTF-8: {error}"))?,
                )
                .map_err(|error| format!("packaged manifest TOML: {error}"))?;
                if manifest
                    .get("package")
                    .and_then(|value| value.get("name"))
                    .and_then(toml::Value::as_str)
                    != Some(rules.package_name)
                    || manifest
                        .get("package")
                        .and_then(|value| value.get("version"))
                        .and_then(toml::Value::as_str)
                        != Some(rules.version)
                {
                    return Err("Cargo normalized package identity mismatch".to_string());
                }
            }
            "Cargo.lock" => {
                let lock: toml::Value = toml::from_str(
                    std::str::from_utf8(&body)
                        .map_err(|error| format!("packaged lock UTF-8: {error}"))?,
                )
                .map_err(|error| format!("packaged lock TOML: {error}"))?;
                if lock
                    .get("version")
                    .and_then(toml::Value::as_integer)
                    .is_none()
                {
                    return Err("Cargo generated lock has no supported version".to_string());
                }
            }
            _ => {
                if rules.committed(&committed_path) != Some(body.as_slice()) {
                    if !rules.is_committed_strip(&committed_path, &body) {
                        return Err(format!(
                            "packaged ordinary entry is not the committed source blob: {relative}"
                        ));
                    }
                    stripped_entries += 1;
                }
            }
        }
        if inventory
            .insert(relative, ArchiveFile { bytes: body, mode })
            .is_some()
        {
            return Err("duplicate archive path".to_string());
        }
    }
    for required in [
        "Cargo.toml",
        "Cargo.toml.orig",
        "Cargo.lock",
        ".cargo_vcs_info.json",
    ] {
        if !inventory.contains_key(required) {
            return Err(format!("produced archive lacks {required}"));
        }
    }
    // A publish-stage archive is exactly: a dirty record, the marker naming
    // the admitted commit, and at least one stripped entry. Any one of the
    // three without the others is refused.
    let marked = inventory.contains_key(crate::publish_stage::PUBLISH_STAGE_MARKER);
    if vcs_dirty != marked || marked != (stripped_entries > 0) {
        return Err(format!(
            "produced archive is not a committed or publish-stage package: dirty={vcs_dirty}, marker={marked}, stripped entries={stripped_entries}"
        ));
    }
    Ok(inventory)
}
