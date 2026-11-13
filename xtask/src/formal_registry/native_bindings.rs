//! Passive native input preparation for the canonical registry. Bindings are
//! untrusted declarations keyed to existing decisions, not another registry.
//! This module neither runs a verifier nor grants native origin or assurance.

use super::{LoadedClaim, ProviderRoute, RegistryResult, ValidatedRegistry, load, require, text};
use crate::formal_evidence::native;
use crate::formal_source_inputs;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const FILE_LIMIT: u64 = 8 * 1_048_576;
const SOURCE_LIMIT: u64 = 256 * 1_048_576;
const MAX_FILES: usize = 20_000;
const COMMON_FILES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "xtask/Cargo.toml",
    "crates/ripr/src/process_owner.rs",
    "crates/ripr-contract/src/analysis/cancellation.rs",
];
const COMMON_DIRECTORIES: &[&str] = &[".cargo", "xtask/src", "tools/repo-policy"];

pub const USAGE: &str = "formal-claim-registry prepare-native --root DIR [--bindings FILE]\nRequires formal-native-bindings. Exit 0 means passive preparation only; missing bindings remain explicit. No verifier runs and no assurance or execution authority is granted.";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BindingInput {
    pub schema: String,
    pub registry_generation: String,
    pub bindings: Vec<ClaimBinding>,
}

impl BindingInput {
    pub fn parse(bytes: &[u8]) -> RegistryResult<Self> {
        require(
            bytes.len() <= super::MAX_INPUT_BYTES as usize,
            "native bindings exceed 1 MiB",
        )?;
        load::parse_unique(bytes)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimBinding {
    pub claim_id: String,
    pub claim_generation: u64,
    pub decision_digest: String,
    pub profile: native::DeclaredKaniProfile,
    pub subjects: Vec<SubjectBinding>,
    pub covers: Vec<CoverBinding>,
    /// Human source correspondence declaration. It is retained for review,
    /// never accepted as authentication of a compiler input or result.
    pub correspondence: String,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubjectBinding {
    pub canonical_subject: String,
    pub package: String,
    pub file: String,
    pub qualified_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CoverBinding {
    pub obligation: String,
    pub canonical_subject: String,
    pub file: String,
    pub anchor: String,
    /// Absent native presentation is an explicit unestablished obligation.
    /// A supplied presentation is still only a declaration, not a native run.
    pub presentation: Option<NativePresentation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct NativePresentation {
    pub property: String,
    pub description: String,
    pub file: String,
    pub function: String,
    pub line: u64,
    pub column: u64,
}

impl NativePresentation {
    pub fn location(&self) -> String {
        format!(
            "{}:{}:{} in function {}",
            self.file, self.line, self.column, self.function
        )
    }
}

#[derive(Clone, Debug, Serialize)]
struct PreparedSubject {
    canonical_subject: String,
    native: native::Subject,
    effective_unwind: u64,
}

#[derive(Clone, Debug, Serialize)]
struct PreparedClaim {
    claim_id: String,
    claim_generation: u64,
    decision_digest: String,
    canonical_subjects: Vec<String>,
    required_covers: Vec<String>,
    negative_controls: Vec<String>,
    binding_state: &'static str,
    missing_obligations: Vec<String>,
    declaration: Option<ClaimBinding>,
    source_manifest: native::SourceManifest,
    source_digest: Option<String>,
    explicit_inputs: Vec<String>,
    subjects: Vec<PreparedSubject>,
}

/// Only the canonical loader and local preparation can construct this value.
/// Its JSON is a report; deserializing that JSON cannot recover this handle.
#[derive(Clone, Debug)]
pub struct PreparedNativeInputs {
    registry: ValidatedRegistry,
    bindings_digest: String,
    claims: Vec<PreparedClaim>,
}

impl PreparedNativeInputs {
    pub fn registry(&self) -> &ValidatedRegistry {
        &self.registry
    }

    pub fn to_json(&self) -> RegistryResult<String> {
        let scoreboard: serde_json::Value = serde_json::from_str(&self.registry.scoreboard_json()?)
            .map_err(|error| error.to_string())?;
        super::json(&serde_json::json!({
            "schema": "ripr.prepared-native-inputs.v1",
            "registry_generation": self.registry.generation(),
            "registry_digest": self.registry.digest(),
            "registry_input_digests": self.registry.input_digests(),
            "bindings_digest": self.bindings_digest,
            "source_policy": "ripr-workspace-ordinary-sources-v1",
            "claims": self.claims,
            "canonical_scoreboard": scoreboard,
            "native_origin": "not_established",
            "native_assurance": "not_assessed",
            "execution_authority": "not_granted",
            "limitations": [
                "Local byte observations do not authenticate a producer or prove compiler consumption.",
                "Ordinary repository roots and visible literal inputs are captured; arbitrary macro expansion, dependency contents and build-script I/O are not discovered.",
                "Source correspondence and native presentations remain supplied declarations.",
                "Protected provenance, currentness and transition admission remain external prerequisites (#14500)."
            ]
        }))
    }
}

impl ValidatedRegistry {
    pub fn prepare_native(
        &self,
        input: Option<&BindingInput>,
    ) -> RegistryResult<PreparedNativeInputs> {
        let absent = BindingInput {
            schema: "ripr.native-bindings.v1".to_string(),
            registry_generation: self.generation().to_string(),
            bindings: Vec::new(),
        };
        let input = input.unwrap_or(&absent);
        require(
            input.schema == "ripr.native-bindings.v1",
            "unknown native bindings schema",
        )?;
        require(
            input.registry_generation == self.generation(),
            "native binding registry generation mismatch",
        )?;
        require(
            input.bindings.len() <= self.loaded.claims.len(),
            "native bindings exceed canonical membership",
        )?;
        let mut bindings = BTreeMap::new();
        for binding in &input.bindings {
            let loaded = self
                .loaded
                .claims
                .iter()
                .find(|loaded| loaded.claim.claim_id == binding.claim_id)
                .ok_or_else(|| format!("unknown canonical claim binding: {}", binding.claim_id))?;
            require(
                bindings
                    .insert(binding.claim_id.as_str(), binding)
                    .is_none(),
                "duplicate canonical claim binding",
            )?;
            validate_binding(loaded, binding)?;
        }
        self.require_registry_bytes()?;
        let mut cache = SourceCache::default();
        let mut claims = Vec::new();
        for loaded in &self.loaded.claims {
            let declaration = bindings.get(loaded.claim.claim_id.as_str()).copied();
            claims.push(self.prepare_claim(loaded, declaration, &mut cache)?);
        }
        self.require_registry_bytes()?;
        let bytes = serde_json::to_vec(input).map_err(|error| error.to_string())?;
        Ok(PreparedNativeInputs {
            registry: self.clone(),
            bindings_digest: native::sha256(&bytes),
            claims,
        })
    }

    fn require_registry_bytes(&self) -> RegistryResult<()> {
        // Reuse exact canonical membership admission as well as byte equality.
        // Hashing only known files would miss a newly added unlisted claim.
        let current = load::load_registry(&self.root().join("policy/formal-claims.json"))?;
        require(
            current.input_digests == self.loaded.input_digests
                && current.generation == self.loaded.generation,
            "canonical registry bytes changed after validation",
        )
    }

    fn prepare_claim(
        &self,
        loaded: &LoadedClaim,
        binding: Option<&ClaimBinding>,
        cache: &mut SourceCache,
    ) -> RegistryResult<PreparedClaim> {
        let claim = &loaded.claim;
        let mut prepared = PreparedClaim {
            claim_id: claim.claim_id.clone(),
            claim_generation: claim.claim_generation,
            decision_digest: loaded.decision_digest.clone(),
            canonical_subjects: super::sorted(&claim.subjects),
            required_covers: super::sorted(&claim.required_covers),
            negative_controls: super::sorted(&claim.negative_controls),
            binding_state: "missing",
            missing_obligations: Vec::new(),
            declaration: binding.cloned(),
            source_manifest: BTreeMap::new(),
            source_digest: None,
            explicit_inputs: Vec::new(),
            subjects: Vec::new(),
        };
        let Some(binding) = binding else {
            prepared.missing_obligations.push(format!(
                "native binding absent for canonical route {}",
                loaded.route.as_str()
            ));
            prepared.missing_obligations.extend(
                claim
                    .subjects
                    .iter()
                    .map(|subject| format!("native subject: {subject}")),
            );
            prepared.missing_obligations.extend(
                claim
                    .required_covers
                    .iter()
                    .map(|cover| format!("native cover presentation: {cover}")),
            );
            return Ok(prepared);
        };
        let packages: Vec<_> = binding
            .subjects
            .iter()
            .map(|subject| subject.package.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut paths: BTreeSet<_> = claim
            .source_closure
            .iter()
            .chain(&loaded.authority_paths)
            .chain(&self.loaded.manifest.global_authority_paths)
            .chain(self.input_digests().keys())
            .cloned()
            .collect();
        paths.extend(COMMON_FILES.iter().map(|path| (*path).to_string()));
        let mut directories: BTreeSet<String> = COMMON_DIRECTORIES
            .iter()
            .map(|path| (*path).to_string())
            .collect();
        let mut dependencies: BTreeSet<_> = packages.iter().cloned().collect();
        if dependencies.contains("ripr") {
            dependencies.insert("ripr-contract".to_string());
        }
        for package in dependencies {
            paths.insert(format!("crates/{package}/Cargo.toml"));
            directories.insert(format!("crates/{package}/src"));
        }
        let mut visited = 0usize;
        for directory in directories {
            gather(self.root(), &directory, &mut paths, &mut visited, 0)?;
        }
        prepared.explicit_inputs =
            formal_source_inputs::extend_source_inputs(self.root(), &mut paths, &packages)
                .map_err(|error| format!("native source input closure: {error}"))?
                .into_iter()
                .collect();
        require(
            paths.len() <= MAX_FILES,
            "native source closure exceeds 20,000 files",
        )?;
        for path in paths {
            let bytes = cache.read(self.root(), &path)?;
            prepared.source_manifest.insert(path, native::sha256(bytes));
        }
        let unwind = claim
            .bounds
            .get("unwind")
            .map(|value| {
                value.as_u64().filter(|number| *number > 0).ok_or_else(|| {
                    "canonical harness unwind must be a positive integer".to_string()
                })
            })
            .transpose()?;
        for subject in &binding.subjects {
            let bytes = cache.read(self.root(), &subject.file)?;
            let source = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
            let bare = subject
                .qualified_name
                .rsplit("::")
                .next()
                .ok_or("native subject name absent")?;
            formal_source_inputs::validate_harness_configuration(source, bare, unwind)?;
            prepared.subjects.push(PreparedSubject {
                canonical_subject: subject.canonical_subject.clone(),
                native: native::Subject {
                    package: subject.package.clone(),
                    file: subject.file.clone(),
                    name: bare.to_string(),
                    qualified_name: Some(subject.qualified_name.clone()),
                    source_digest: native::sha256(bytes),
                },
                effective_unwind: unwind.unwrap_or(binding.profile.default_unwind),
            });
        }
        for cover in &binding.covers {
            validate_cover_source(self.root(), cover, cache)?;
        }
        for required in &claim.required_covers {
            if !binding
                .covers
                .iter()
                .any(|cover| cover.obligation == *required && cover.presentation.is_some())
            {
                prepared
                    .missing_obligations
                    .push(format!("native cover presentation: {required}"));
            }
        }
        prepared.binding_state = if prepared.missing_obligations.is_empty() {
            "prepared"
        } else {
            "incomplete"
        };
        prepared.source_digest = Some(native::sha256(&native::canonical_manifest_bytes(
            &prepared.source_manifest,
        )?));
        Ok(prepared)
    }
}

fn validate_binding(loaded: &LoadedClaim, binding: &ClaimBinding) -> RegistryResult<()> {
    let claim = &loaded.claim;
    require(
        loaded.route == ProviderRoute::Native
            && claim.provider == "kani"
            && claim.family == "bounded_model_check",
        "canonical claim has no ordinary native Kani route",
    )?;
    require(
        binding.claim_generation == claim.claim_generation
            && binding.decision_digest == loaded.decision_digest,
        "native binding claim generation or decision digest mismatch",
    )?;
    binding.profile.validate()?;
    text(&binding.correspondence, "native correspondence declaration")?;
    require(
        !binding.limitations.is_empty(),
        "native binding limitations must be explicit",
    )?;
    super::unique_strings(&binding.limitations, "native binding limitations")?;
    let mut subjects = BTreeSet::new();
    for subject in &binding.subjects {
        require(
            subjects.insert(&subject.canonical_subject),
            "duplicate canonical subject binding",
        )?;
        require(
            matches!(subject.package.as_str(), "ripr" | "ripr-contract"),
            "unsupported native binding package",
        )?;
        require(
            claim.source_closure.contains(&subject.file)
                && subject
                    .file
                    .starts_with(&format!("crates/{}/src/", subject.package)),
            "native subject is outside canonical source closure or package",
        )?;
        let bare = subject.qualified_name.rsplit("::").next().unwrap_or("");
        require(
            !bare.is_empty() && subject.canonical_subject.rsplit("::").next() == Some(bare),
            "native locator differs from canonical subject name",
        )?;
        require(
            subject.qualified_name.split("::").all(|part| {
                !part.is_empty() && part.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
            }),
            "invalid qualified native subject",
        )?;
    }
    require(
        subjects == claim.subjects.iter().collect(),
        "native subject inventory differs from canonical subjects",
    )?;
    let mut covers = BTreeSet::new();
    let mut properties = BTreeSet::new();
    let mut presentations = BTreeSet::new();
    for cover in &binding.covers {
        require(
            claim.required_covers.contains(&cover.obligation),
            "unknown canonical cover obligation",
        )?;
        require(
            covers.insert(&cover.obligation),
            "duplicate canonical cover binding",
        )?;
        require(
            subjects.contains(&cover.canonical_subject),
            "cover has an unknown canonical subject",
        )?;
        require(
            claim.source_closure.contains(&cover.file),
            "cover anchor is outside canonical source closure",
        )?;
        text(&cover.anchor, "native cover anchor")?;
        if let Some(presentation) = &cover.presentation {
            require(
                presentation.property.contains(".cover."),
                "native cover property is not a cover identity",
            )?;
            for (value, field) in [
                (&presentation.property, "native property"),
                (&presentation.description, "native description"),
                (&presentation.function, "native function"),
            ] {
                text(value, field)?;
            }
            require(
                claim.source_closure.contains(&presentation.file),
                "native cover presentation is outside canonical source closure",
            )?;
            require(
                presentation.line > 0 && presentation.column > 0,
                "native cover line and column must be positive",
            )?;
            require(
                properties.insert((&cover.canonical_subject, &presentation.property)),
                "one native property cannot satisfy two canonical cover obligations",
            )?;
            require(
                presentations.insert((
                    &cover.canonical_subject,
                    &presentation.description,
                    presentation.location(),
                )),
                "duplicate native cover presentation cannot satisfy two obligations",
            )?;
        }
    }
    Ok(())
}

#[derive(Default)]
struct SourceCache {
    bytes: BTreeMap<String, Vec<u8>>,
    total: u64,
}

impl SourceCache {
    fn read(&mut self, root: &Path, path: &str) -> RegistryResult<&[u8]> {
        if !self.bytes.contains_key(path) {
            load::require_regular_path(root, path, "native source input")?;
            let bytes = native::read_file_bounded(&root.join(path), FILE_LIMIT)?;
            self.total = self
                .total
                .checked_add(bytes.len() as u64)
                .ok_or("native source size overflow")?;
            require(
                self.total <= SOURCE_LIMIT && self.bytes.len() < MAX_FILES,
                "native source exceeds 256 MiB or 20,000 files",
            )?;
            self.bytes.insert(path.to_string(), bytes);
        }
        self.bytes
            .get(path)
            .map(Vec::as_slice)
            .ok_or_else(|| "native source cache incomplete".to_string())
    }
}

fn gather(
    root: &Path,
    directory: &str,
    paths: &mut BTreeSet<String>,
    visited: &mut usize,
    depth: usize,
) -> RegistryResult<()> {
    load::require_directory_path(root, directory)?;
    require(depth < 128, "native source directory depth exceeds 128")?;
    for entry in fs::read_dir(root.join(directory)).map_err(|error| error.to_string())? {
        *visited += 1;
        require(
            *visited <= MAX_FILES,
            "native source traversal exceeds 20,000 entries",
        )?;
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "native source path is not UTF-8")?;
        let path = format!("{directory}/{name}");
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            gather(root, &path, paths, visited, depth + 1)?;
        } else {
            load::require_regular_path(root, &path, "native source input")?;
            paths.insert(path);
        }
    }
    Ok(())
}

fn validate_cover_source(
    root: &Path,
    cover: &CoverBinding,
    cache: &mut SourceCache,
) -> RegistryResult<()> {
    let source =
        std::str::from_utf8(cache.read(root, &cover.file)?).map_err(|error| error.to_string())?;
    require(
        source.matches(&cover.anchor).count() == 1,
        "native cover anchor is missing or ambiguous",
    )?;
    if let Some(presentation) = &cover.presentation {
        let source = std::str::from_utf8(cache.read(root, &presentation.file)?)
            .map_err(|error| error.to_string())?;
        let line = usize::try_from(presentation.line - 1).map_err(|error| error.to_string())?;
        let line = source
            .lines()
            .nth(line)
            .ok_or("native cover line is outside captured source")?;
        require(
            presentation.column <= line.len() as u64 + 1,
            "native cover column is outside captured source",
        )?;
    }
    Ok(())
}

pub(super) fn execute(args: &[String]) -> RegistryResult<String> {
    let mut root = None;
    let mut bindings = None;
    let mut flags = args.iter();
    while let Some(flag) = flags.next() {
        let value = flags
            .next()
            .ok_or_else(|| format!("{flag} requires a value; {USAGE}"))?;
        match flag.as_str() {
            "--root" => {
                require(root.is_none(), "duplicate --root")?;
                root = Some(value);
            }
            "--bindings" => {
                require(bindings.is_none(), "duplicate --bindings")?;
                bindings = Some(value);
            }
            _ => {
                return Err(format!(
                    "unknown native preparation argument {flag}; {USAGE}"
                ));
            }
        }
    }
    let root = root.ok_or_else(|| format!("--root is required; {USAGE}"))?;
    let view = ValidatedRegistry::load(Path::new(root))?;
    let bindings = bindings
        .map(|path| {
            BindingInput::parse(&native::read_file_bounded(
                Path::new(path),
                super::MAX_INPUT_BYTES,
            )?)
        })
        .transpose()?;
    view.prepare_native(bindings.as_ref())?.to_json()
}

#[cfg(test)]
mod tests;
