use super::*;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

struct UniqueJson(serde_json::Value);

impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;

            fn expecting(
                &self,
                formatter: &mut std::fmt::Formatter<'_>,
            ) -> std::fmt::Result {
                formatter.write_str("JSON without duplicate object keys")
            }

            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }

            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }

            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }

            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(value)
                    .map(|number| UniqueJson(number.into()))
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }

            fn visit_string<E: serde::de::Error>(
                self,
                value: String,
            ) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }

            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::Null))
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueJson(value)) = sequence.next_element()? {
                    values.push(value);
                }
                Ok(UniqueJson(values.into()))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some((key, UniqueJson(value))) =
                    map.next_entry::<String, UniqueJson>()?
                {
                    if values.insert(key.clone(), value).is_some() {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON key: {key}"
                        )));
                    }
                }
                Ok(UniqueJson(values.into()))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

pub(super) fn parse_unique<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> RegistryResult<T> {
    let UniqueJson(value) = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

fn read_bounded(path: &Path, remaining: &mut u64) -> RegistryResult<Vec<u8>> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("inspect {}: {error}", path.display()))?;
    require(
        metadata.file_type().is_file(),
        format!("{} must be a regular non-symlink file", path.display()),
    )?;
    let limit = MAX_INPUT_BYTES.min(*remaining);
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| format!("open {}: {error}", path.display()))?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    let length = u64::try_from(bytes.len()).map_err(|error| error.to_string())?;
    require(
        length <= limit,
        "input exceeds 1 MiB per file or 16 MiB total budget",
    )?;
    *remaining -= length;
    Ok(bytes)
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn registry_root(manifest_path: &Path) -> RegistryResult<PathBuf> {
    let policy = manifest_path
        .parent()
        .ok_or_else(|| "registry manifest has no policy directory".to_string())?;
    require(
        policy.file_name().and_then(|name| name.to_str()) == Some("policy"),
        "registry manifest must live directly under policy/",
    )?;
    policy
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "registry manifest has no repository root".to_string())
}

fn require_regular_path(root: &Path, relative: &str, field: &str) -> RegistryResult<()> {
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("{field} {}: {error}", path.display()))?;
    require(
        metadata.file_type().is_file(),
        format!("{field} {} must be a regular non-symlink file", path.display()),
    )
}

fn discover_claim_documents(root: &Path) -> RegistryResult<BTreeSet<String>> {
    let directory = root.join("policy/formal-claims.d");
    let mut documents = BTreeSet::new();
    for entry in fs::read_dir(&directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        require(
            file_type.is_file() && entry.path().extension().and_then(|value| value.to_str()) == Some("json"),
            format!(
                "{}: claim registry directory accepts only regular .json files",
                entry.path().display()
            ),
        )?;
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_path_buf();
        documents.insert(normalized_path(&relative));
    }
    Ok(documents)
}

fn validate_manifest(manifest: &Manifest) -> RegistryResult<()> {
    require(
        manifest.schema == "ripr.formal-claim-registry.v1",
        "unsupported registry schema",
    )?;
    text(&manifest.repository, "repository")?;
    require(
        manifest.registry_generation > 0,
        "registry_generation must be positive",
    )?;
    require(
        !manifest.claims.is_empty() && manifest.claims.len() <= MAX_CLAIMS,
        "registry must contain between 1 and 256 active claims",
    )?;
    unique_paths(
        &manifest.global_authority_paths,
        "global_authority_paths",
        true,
    )?;

    let mut documents = BTreeSet::new();
    let mut claims = BTreeSet::new();
    for entry in &manifest.claims {
        repository_path(&entry.document, "claim document")?;
        require(
            entry.document.starts_with("policy/formal-claims.d/")
                && entry.document.ends_with(".json"),
            "claim document must be a .json file under policy/formal-claims.d/",
        )?;
        stable_id(&entry.claim_id, "manifest claim_id")?;
        require(
            entry.claim_generation > 0,
            format!("{}: manifest claim_generation must be positive", entry.claim_id),
        )?;
        unique_paths(&entry.authority_paths, "authority_paths", true)?;
        digest_field(
            &entry.expected_decision_digest,
            "expected_decision_digest",
        )?;
        require(
            documents.insert(entry.document.as_str()),
            format!("duplicate claim document {}", entry.document),
        )?;
        require(
            claims.insert(entry.claim_id.as_str()),
            format!("duplicate active claim ID {}", entry.claim_id),
        )?;
    }

    let active: BTreeSet<&str> = manifest
        .claims
        .iter()
        .map(|entry| entry.claim_id.as_str())
        .collect();
    let mut retired = BTreeSet::new();
    for retirement in &manifest.retirements {
        stable_id(&retirement.claim_id, "retirement claim_id")?;
        require(
            retirement.final_generation > 0,
            format!("{}: retirement final_generation must be positive", retirement.claim_id),
        )?;
        text(&retirement.reason, "retirement reason")?;
        require(
            retired.insert(retirement.claim_id.as_str()),
            format!("duplicate retirement {}", retirement.claim_id),
        )?;
        require(
            !active.contains(retirement.claim_id.as_str()),
            format!("{} is both active and retired", retirement.claim_id),
        )?;
        if let Some(replacement) = &retirement.replacement_claim_id {
            stable_id(replacement, "replacement_claim_id")?;
            require(
                replacement != &retirement.claim_id && active.contains(replacement.as_str()),
                format!(
                    "{}: replacement must name another active claim",
                    retirement.claim_id
                ),
            )?;
        }
    }
    Ok(())
}

pub(super) fn load_registry(path: &Path) -> RegistryResult<LoadedRegistry> {
    let mut remaining = MAX_TOTAL_INPUT_BYTES;
    let manifest_bytes = read_bounded(path, &mut remaining)?;
    let manifest: Manifest = parse_unique(&manifest_bytes)
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
    validate_manifest(&manifest)?;
    let root = registry_root(path)?;
    for authority in &manifest.global_authority_paths {
        require_regular_path(&root, authority, "global authority path")?;
    }

    let listed: BTreeSet<String> = manifest
        .claims
        .iter()
        .map(|entry| entry.document.clone())
        .collect();
    let discovered = discover_claim_documents(&root)?;
    require(
        listed == discovered,
        format!(
            "registry membership differs from policy/formal-claims.d: listed={listed:?}; discovered={discovered:?}"
        ),
    )?;

    let mut audit_args = Vec::with_capacity(manifest.claims.len() * 2);
    for entry in &manifest.claims {
        audit_args.push("--claim".to_string());
        audit_args.push(root.join(&entry.document).to_string_lossy().into_owned());
    }
    // `formal_evidence` remains the one semantic validator for FormalClaimV1.
    formal_evidence::execute(&audit_args)
        .map_err(|error| format!("formal claim admission failed: {error}"))?;

    let mut claims = Vec::with_capacity(manifest.claims.len());
    let mut critical = 0usize;
    for entry in &manifest.claims {
        let claim_path = root.join(&entry.document);
        let bytes = read_bounded(&claim_path, &mut remaining)?;
        let claim: ClaimDocument = parse_unique(&bytes)
            .map_err(|error| format!("parse {}: {error}", claim_path.display()))?;
        require(
            claim.schema == "ripr.formal-claim.v1",
            format!("{}: unsupported claim schema", entry.claim_id),
        )?;
        require(
            claim.claim_id == entry.claim_id
                && claim.claim_generation == entry.claim_generation,
            format!(
                "{}: manifest identity/generation does not match its claim document",
                entry.claim_id
            ),
        )?;
        for authority in &entry.authority_paths {
            require_regular_path(&root, authority, "claim authority path")?;
        }
        for source in &claim.source_closure {
            require_regular_path(&root, source, "claim source closure")?;
        }
        let actual_digest = decision_digest(
            &claim,
            &entry.document,
            entry.route,
            &entry.authority_paths,
        )?;
        require(
            actual_digest == entry.expected_decision_digest,
            format!(
                "{}: decision digest mismatch; expected {}, observed {}",
                entry.claim_id, entry.expected_decision_digest, actual_digest
            ),
        )?;
        if entry.route == ProviderRoute::Native {
            require(
                claim.provider == "kani"
                    && claim.family == "bounded_model_check"
                    && !claim.subjects.is_empty(),
                format!(
                    "{}: native route requires a Kani bounded-model claim with subjects",
                    entry.claim_id
                ),
            )?;
        }
        if claim.criticality == "critical" {
            critical += 1;
        }
        claims.push(LoadedClaim {
            document: entry.document.clone(),
            route: entry.route,
            authority_paths: entry.authority_paths.clone(),
            decision_digest: actual_digest,
            claim,
        });
    }
    require(critical > 0, "registry has no active critical claims")?;
    claims.sort_by(|left, right| left.claim.claim_id.cmp(&right.claim.claim_id));

    let digest = registry_digest(&manifest, &claims)?;
    let generation = format!("{}:{digest}", manifest.registry_generation);
    Ok(LoadedRegistry {
        manifest,
        claims,
        registry_digest: digest,
        generation,
    })
}
