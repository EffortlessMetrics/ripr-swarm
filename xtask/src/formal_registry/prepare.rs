use super::*;
use formal::{Criticality, Family, Phase, require, strings, text};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, PathBuf};

const FILE_LIMIT: u64 = 8 * 1_048_576;
const SOURCE_LIMIT: u64 = 256 * 1_048_576;
const COMMON_FILES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "xtask/Cargo.toml",
    "crates/ripr/src/process_owner.rs",
    "crates/ripr-contract/src/analysis/cancellation.rs",
];
const COMMON_DIRS: &[&str] = &[".cargo", "xtask/src", "tools/repo-policy"];
const CURRENTNESS: &[&str] = &[
    "source_closure",
    "registry_generation",
    "claim_generation",
    "provider_profile",
    "bounds",
    "assumptions",
    "native_inventory",
    "raw_output",
    "producer_generation",
];

fn path(value: &str) -> AuditResult<()> {
    native::relative_path(value)?;
    require(
        !value.contains(':') && !value.chars().any(char::is_control) && !value.contains(['*', '?']),
        "registry paths must be literal repository-relative paths",
    )
}

fn normalize(registry: &mut Registry) {
    registry
        .claims
        .sort_by(|a, b| a.claim.claim_id.cmp(&b.claim.claim_id));
    registry
        .compatible_relocations
        .sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
    for entry in &mut registry.claims {
        let claim = &mut entry.claim;
        for values in [
            &mut claim.non_claims,
            &mut claim.owners,
            &mut claim.source_closure,
            &mut claim.trust_dependencies,
            &mut claim.subjects,
            &mut claim.required_covers,
            &mut claim.negative_controls,
            &mut claim.currentness_inputs,
            &mut entry.authority_paths,
            &mut entry.correspondence_review.verification_only_paths,
            &mut entry.correspondence_review.limitations,
        ] {
            values.sort();
        }
        entry.cover_requirements.sort_by(|a, b| a.id.cmp(&b.id));
        entry.negative_controls.sort_by(|a, b| a.id.cmp(&b.id));
        for control in &mut entry.negative_controls {
            control.expected_failures.sort();
        }
        entry
            .correspondence_review
            .production
            .sort_by(|a, b| (&a.file, &a.symbol).cmp(&(&b.file, &b.symbol)));
    }
}

pub fn validate_registry(registry: &Registry) -> AuditResult<()> {
    require(
        registry.schema == "ripr.critical-claim-registry.v1" && registry.generation > 0,
        "unsupported registry schema or nonpositive generation",
    )?;
    formal::audit(
        &registry
            .claims
            .iter()
            .map(|r| r.claim.clone())
            .collect::<Vec<_>>(),
        None,
    )?;
    for entry in &registry.claims {
        let claim = &entry.claim;
        require(
            !matches!(claim.phase, Phase::OrdinaryPrCritical | Phase::MergeReady),
            "new registry entries cannot authorize historical PR phases",
        )?;
        strings(&entry.authority_paths, "authority_paths", true)?;
        strings(
            &entry.correspondence_review.verification_only_paths,
            "verification_only_paths",
            true,
        )?;
        strings(
            &entry.correspondence_review.limitations,
            "correspondence limitations",
            true,
        )?;
        require(
            entry.correspondence_review.rule == "normal_production_bodies_reviewed_no_replacements",
            "unsupported production correspondence rule",
        )?;
        require(
            !entry.correspondence_review.production.is_empty(),
            "production correspondence is missing",
        )?;
        for owner in &entry.correspondence_review.production {
            path(&owner.file)?;
            text(&owner.symbol, "production symbol")?;
            text(&owner.anchor, "production source anchor")?;
            require(
                claim
                    .owners
                    .contains(&format!("{}::{}", owner.file, owner.symbol))
                    && claim.source_closure.contains(&owner.file),
                "production owner is outside the declared closure",
            )?;
        }
        path(&claim.owner_spec)?;
        require(
            claim.owner_spec.starts_with("docs/specs/RIPR-SPEC-")
                && claim.owner_spec.ends_with(".md"),
            "owner_spec must name a repository specification",
        )?;
        for required in CURRENTNESS {
            require(
                claim.currentness_inputs.iter().any(|s| s == required),
                &format!("missing currentness law: {required}"),
            )?;
        }
        require(
            claim.transition_policy == "generation_on_decision_change_no_automatic_retirement",
            "unsupported transition law",
        )?;
        for p in entry
            .authority_paths
            .iter()
            .chain(&claim.source_closure)
            .chain(&entry.correspondence_review.verification_only_paths)
        {
            path(p)?;
        }
        for p in &entry.correspondence_review.verification_only_paths {
            require(
                entry.authority_paths.contains(p),
                "verification input omitted from authority closure",
            )?;
        }
        let cover_ids: BTreeSet<_> = entry.cover_requirements.iter().map(|c| &c.id).collect();
        require(
            cover_ids.len() == entry.cover_requirements.len()
                && cover_ids == claim.required_covers.iter().collect(),
            "required cover mapping is incomplete or duplicated",
        )?;
        if claim.criticality == Criticality::Critical {
            require(
                !cover_ids.is_empty(),
                "critical claim needs an explicit reachability obligation",
            )?;
        }
        let mut native_covers = BTreeSet::new();
        for cover in &entry.cover_requirements {
            path(&cover.file)?;
            text(&cover.anchor, "cover source anchor")?;
            require(
                entry.authority_paths.contains(&cover.file),
                "cover omitted from authority closure",
            )?;
            if let Some(native) = &cover.native {
                path(&native.file)?;
                text(&native.description, "native cover description")?;
                text(&native.function, "native cover function")?;
                require(
                    claim.provider == "kani"
                        && entry.authority_paths.contains(&native.file)
                        && native.line > 0
                        && native.column > 0
                        && native_covers.insert(native),
                    "invalid, omitted or duplicated native cover mapping",
                )?;
            }
        }
        let controls: BTreeSet<_> = entry.negative_controls.iter().map(|c| &c.id).collect();
        require(
            controls.len() == entry.negative_controls.len()
                && controls == claim.negative_controls.iter().collect(),
            "negative-control mapping is incomplete or duplicated",
        )?;
        for control in &entry.negative_controls {
            path(&control.file)?;
            text(&control.before, "negative before")?;
            text(&control.after, "negative after")?;
            text(&control.effect, "negative effect")?;
            require(
                control.before != control.after
                    && control.expected_failed_claim == claim.claim_id
                    && claim.subjects.contains(&control.expected_subject)
                    && claim.source_closure.contains(&control.file),
                "negative control lacks an exact distinct mutation and failed subject",
            )?;
            if claim.provider == "kani" {
                require(
                    entry
                        .correspondence_review
                        .production
                        .iter()
                        .any(|owner| owner.file == control.file)
                        && !entry.authority_paths.contains(&control.file),
                    "native control must edit a reviewed production file separate from proof-authority files",
                )?;
            }
            let mut properties = BTreeSet::new();
            for expected in &control.expected_failures {
                let subject = entry
                    .subject
                    .as_ref()
                    .ok_or("native assertion has no registered harness")?;
                let presentation = &expected.presentation;
                let prefix = format!("{}.assertion.", subject.symbol);
                let suffix = expected.property.strip_prefix(&prefix).unwrap_or("");
                text(&presentation.description, "native assertion description")?;
                path(&presentation.file)?;
                require(
                    claim.provider == "kani"
                        && !suffix.is_empty()
                        && suffix.bytes().all(|byte| byte.is_ascii_digit())
                        && properties.insert(&expected.property)
                        && presentation.file == subject.file
                        && presentation.function == subject.symbol
                        && presentation.line > 0
                        && presentation.column > 0,
                    "native negative control requires distinct assertions in its exact registered harness",
                )?;
            }
        }
        match claim.provider.as_str() {
            "kani" => {
                let profile = entry
                    .provider_profile
                    .as_ref()
                    .ok_or("Kani profile missing")?;
                require(
                    profile.version == "0.68.0"
                        && profile.solver_version == "6.11.0"
                        && profile.configuration_policy == "kani-0.68-defaults-v1"
                        && profile.default_unwind == 32
                        && profile.target == "x86_64-unknown-linux-gnu"
                        && profile.compiler_trust == "kani-bundled-observed",
                    "unknown provider/configuration profile",
                )?;
                require(
                    matches!(claim.bounds.get("default_unwind"), Some(formal::BoundValue::Number(value)) if value.as_u64() == Some(profile.default_unwind)),
                    "registered default unwind differs from the admitted native profile",
                )?;
                require(
                    entry.closure_rule == ClosureRule::KaniWorkspaceSourcesV1
                        && claim.result_satisfaction
                            == "all_registered_subjects_verified_with_complete_required_covers",
                    "unsupported Kani source closure or satisfaction law",
                )?;
                let subject = entry.subject.as_ref().ok_or("Kani subject missing")?;
                path(&subject.file)?;
                text(&subject.anchor, "harness source anchor")?;
                let package = entry.package.as_deref().ok_or("Kani package missing")?;
                require(
                    matches!(package, "ripr" | "ripr-contract"),
                    "unadmitted Kani package",
                )?;
                require(
                    claim.subjects == [format!("{package}:{}::{}", subject.file, subject.symbol)]
                        && entry.authority_paths.contains(&subject.file)
                        && claim.source_closure.contains(&subject.file),
                    "Kani subject/authority mapping mismatch",
                )?;
            }
            "manual" | "tlc" => {
                require(
                    entry.provider_profile.is_none()
                        && entry.subject.is_none()
                        && entry.package.is_none()
                        && entry.closure_rule == ClosureRule::ExplicitManualV1,
                    "non-Kani obligation cannot borrow a Kani route",
                )?;
                require(
                    (claim.provider == "manual" && claim.family == Family::ManualOrExternal)
                        || (claim.provider == "tlc" && claim.family == Family::StateMachineModel),
                    "provider/family mismatch",
                )?;
                text(
                    entry
                        .alternative_evidence_owner
                        .as_deref()
                        .ok_or("alternative evidence owner missing")?,
                    "alternative evidence owner",
                )?;
            }
            _ => return Err("unknown critical-claim provider".to_string()),
        }
    }
    let mut relocations = BTreeSet::new();
    for relocation in &registry.compatible_relocations {
        require(
            relocations.insert(&relocation.claim_id),
            "duplicate compatible relocation",
        )?;
        path(&relocation.from_file)?;
        path(&relocation.to_file)?;
        formal::digest(&relocation.source_digest, "relocation digest")?;
        require(
            relocation.from_file != relocation.to_file,
            "relocation must move a file",
        )?;
        require(
            registry.claims.iter().any(|entry| {
                entry.claim.claim_id == relocation.claim_id
                    && entry.claim.subjects.contains(&relocation.to_subject)
                    && entry.authority_paths.contains(&relocation.to_file)
            }),
            "relocation has no exact registered successor",
        )?;
    }
    Ok(())
}

pub(super) fn read_bounded(path: &Path, limit: u64) -> AuditResult<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    require(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "input must be a regular, non-symlink file",
    )?;
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    require(
        bytes.len() as u64 <= limit,
        "input exceeds bounded read limit",
    )?;
    Ok(bytes)
}

fn resolve(root: &Path, name: &str) -> AuditResult<PathBuf> {
    path(name)?;
    let mut current = root.to_path_buf();
    for component in Path::new(name).components() {
        require(
            matches!(component, Component::Normal(_)),
            "non-normal repository path",
        )?;
        current.push(component);
        require(
            !fs::symlink_metadata(&current)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink(),
            "source symlink is outside the admitted closure",
        )?;
    }
    Ok(current)
}

fn gather(root: &Path, name: &str, files: &mut BTreeSet<String>) -> AuditResult<()> {
    let resolved = resolve(root, name)?;
    if resolved.is_dir() {
        for entry in fs::read_dir(resolved).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let child = entry
                .file_name()
                .into_string()
                .map_err(|path| format!("non-UTF8 source path: {path:?}"))?;
            gather(root, &format!("{name}/{child}"), files)?;
        }
    } else {
        files.insert(name.to_string());
        require(files.len() <= 20_000, "source closure exceeds 20,000 files")?;
    }
    Ok(())
}

fn anchor(root: &Path, file: &str, expected: &str) -> AuditResult<()> {
    let bytes = read_bounded(&resolve(root, file)?, FILE_LIMIT)?;
    let source = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    require(
        source.matches(expected).count() == 1,
        &format!("missing or ambiguous source anchor in {file}"),
    )
}

fn native_location(root: &Path, expected: &NativeCoverExpectation) -> AuditResult<()> {
    let bytes = read_bounded(&resolve(root, &expected.file)?, FILE_LIMIT)?;
    let source = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    let line = usize::try_from(expected.line - 1).map_err(|e| e.to_string())?;
    let line = source
        .lines()
        .nth(line)
        .ok_or("native property line is outside source")?;
    require(
        expected.column <= line.len() as u64 + 1,
        "native property column is outside source",
    )
}

fn closure_layout(
    entry: &Registration,
) -> AuditResult<(BTreeSet<String>, Vec<String>, Vec<String>)> {
    let mut files: BTreeSet<_> = entry
        .claim
        .source_closure
        .iter()
        .chain(&entry.authority_paths)
        .cloned()
        .collect();
    let mut directories = Vec::new();
    let mut optional = Vec::new();
    if entry.closure_rule == ClosureRule::KaniWorkspaceSourcesV1 {
        let packages = match entry.package.as_deref() {
            Some("ripr") => vec!["ripr", "ripr-contract"],
            Some("ripr-contract") => vec!["ripr-contract"],
            _ => return Err("package missing or unsupported".to_string()),
        };
        for package in packages {
            directories.push(format!("crates/{package}/src"));
            files.insert(format!("crates/{package}/Cargo.toml"));
            optional.push(format!("crates/{package}/build.rs"));
        }
        files.extend(COMMON_FILES.iter().map(|s| s.to_string()));
        directories.extend(COMMON_DIRS.iter().map(|s| s.to_string()));
    }
    Ok((files, directories, optional))
}

/// The same closed source-selection rule is applied to a native capture. This
/// detects an omitted helper even when a supplied prepared digest was recomputed.
pub(super) fn project_source(
    entry: &Registration,
    captured: &SourceManifest,
    explicit_inputs: &[String],
) -> AuditResult<SourceManifest> {
    let (mut files, directories, optional) = closure_layout(entry)?;
    for directory in directories {
        let members: Vec<_> = captured
            .keys()
            .filter(|p| p.starts_with(&format!("{directory}/")))
            .cloned()
            .collect();
        require(
            !members.is_empty(),
            &format!("source capture lacks required directory: {directory}"),
        )?;
        files.extend(members);
    }
    files.extend(optional.into_iter().filter(|p| captured.contains_key(p)));
    for input in explicit_inputs {
        path(input)?;
        files.insert(input.clone());
    }
    files
        .into_iter()
        .map(|path| {
            captured
                .get(&path)
                .cloned()
                .map(|digest| (path.clone(), digest))
                .ok_or_else(|| format!("source capture lacks required file: {path}"))
        })
        .collect()
}

struct ResolvedInputs {
    files: BTreeSet<String>,
    explicit_inputs: Vec<String>,
}

pub fn prepare(root: &Path, registry_path: &Path) -> AuditResult<PreparedRegistry> {
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let registry_file = if registry_path.is_absolute() {
        registry_path.to_path_buf()
    } else {
        root.join(registry_path)
    };
    let relative = registry_file
        .strip_prefix(&root)
        .map_err(|error| format!("registry must be inside its source root: {error}"))?
        .to_str()
        .ok_or("registry path is not UTF-8")?
        .replace('\\', "/");
    let bytes = read_input(&resolve(&root, &relative)?)?;
    let mut registry: Registry = formal::parse_json(&bytes)?;
    normalize(&mut registry);
    validate_registry(&registry)?;
    let mut cache = SourceManifest::new();
    // Claims with the same package and exact initial path set have the same
    // conservative Rust include closure. Reuse only within this preparation;
    // later calls still rescan their current source bytes and configuration.
    let mut closures = BTreeMap::<(String, BTreeSet<String>), ResolvedInputs>::new();
    let mut total_bytes = 0u64;
    let mut claims = Vec::new();
    for registration in &registry.claims {
        let claim = &registration.claim;
        let (mut files, directories, optional) = closure_layout(registration)?;
        for directory in directories {
            gather(&root, &directory, &mut files)?;
        }
        files.extend(optional.into_iter().filter(|path| root.join(path).exists()));
        let explicit_inputs = if registration.closure_rule == ClosureRule::KaniWorkspaceSourcesV1 {
            let package = registration.package.as_ref().ok_or("package missing")?;
            let key = (package.clone(), files.clone());
            if let Some(captured) = closures.get(&key) {
                files = captured.files.clone();
                captured.explicit_inputs.clone()
            } else {
                let explicit_inputs =
                    native::extend_source_inputs(&root, &mut files, std::slice::from_ref(package))?
                        .into_iter()
                        .collect::<Vec<_>>();
                closures.insert(
                    key,
                    ResolvedInputs {
                        files: files.clone(),
                        explicit_inputs: explicit_inputs.clone(),
                    },
                );
                explicit_inputs
            }
        } else {
            Vec::new()
        };
        require(files.len() <= 20_000, "source closure exceeds 20,000 files")?;
        read_bounded(&resolve(&root, &claim.owner_spec)?, FILE_LIMIT)?;
        for owner in &registration.correspondence_review.production {
            anchor(&root, &owner.file, &owner.anchor)?;
        }
        if let Some(subject) = &registration.subject {
            anchor(&root, &subject.file, &subject.anchor)?;
            let bytes = read_bounded(&resolve(&root, &subject.file)?, FILE_LIMIT)?;
            let source = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
            let unwind = claim
                .bounds
                .get("unwind")
                .map(|value| match value {
                    formal::BoundValue::Number(number) => number
                        .as_u64()
                        .filter(|value| *value > 0)
                        .ok_or("harness unwind must be a positive integer"),
                    _ => Err("harness unwind must be a positive integer"),
                })
                .transpose()?;
            native::validate_harness_configuration(
                source,
                subject
                    .symbol
                    .rsplit("::")
                    .next()
                    .ok_or("harness name missing")?,
                unwind,
            )?;
        }
        for cover in &registration.cover_requirements {
            anchor(&root, &cover.file, &cover.anchor)?;
            if let Some(native) = &cover.native {
                native_location(&root, native)?;
            }
        }
        for control in &registration.negative_controls {
            anchor(&root, &control.file, &control.before)?;
            for expected in &control.expected_failures {
                native_location(&root, &expected.presentation)?;
            }
        }
        let mut source_manifest = SourceManifest::new();
        for name in files {
            if !cache.contains_key(&name) {
                let bytes = read_bounded(&resolve(&root, &name)?, FILE_LIMIT)?;
                total_bytes = total_bytes
                    .checked_add(bytes.len() as u64)
                    .ok_or("source size overflow")?;
                require(
                    total_bytes <= SOURCE_LIMIT,
                    "source closure exceeds 256 MiB",
                )?;
                cache.insert(name.clone(), native::sha256(&bytes));
            }
            source_manifest.insert(
                name.clone(),
                cache.get(&name).ok_or("source cache incomplete")?.clone(),
            );
        }
        let authority_manifest = registration
            .authority_paths
            .iter()
            .map(|path| {
                source_manifest
                    .get(path)
                    .cloned()
                    .map(|digest| (path.clone(), digest))
                    .ok_or("authority omitted from source")
            })
            .collect::<Result<SourceManifest, _>>()?;
        let subject = registration
            .subject
            .as_ref()
            .map(|locator| -> AuditResult<_> {
                Ok(native::Subject {
                    package: registration.package.clone().ok_or("package missing")?,
                    file: locator.file.clone(),
                    name: locator
                        .symbol
                        .rsplit("::")
                        .next()
                        .ok_or("harness name missing")?
                        .to_string(),
                    qualified_name: Some(locator.symbol.clone()),
                    source_digest: source_manifest
                        .get(&locator.file)
                        .ok_or("subject outside source")?
                        .clone(),
                })
            })
            .transpose()?;
        claims.push(PreparedClaim {
            registration: registration.clone(),
            claim: claim.clone(),
            semantic_family: registration.semantic_family.clone(),
            source_digest: native::sha256(&native::canonical_manifest_bytes(&source_manifest)?),
            source_manifest,
            explicit_inputs,
            authority_manifest,
            subject,
            assumptions_digest: formal::assumptions_digest(&claim.assumptions)?,
            bounds_digest: digest_value(&claim.bounds)?,
            provider_profile: registration.provider_profile.clone(),
            required_cover_count: claim.required_covers.len() as u64,
            correspondence_review: registration.correspondence_review.clone(),
        });
    }
    let prepared = PreparedRegistry {
        schema: "ripr.prepared-critical-claims.v1".to_string(),
        registry_generation: digest_value(&registry)?,
        registry_path: relative,
        registry_digest: native::sha256(&bytes),
        registry,
        claims,
    };
    validate_prepared(&prepared)?;
    Ok(prepared)
}

pub fn validate_prepared(prepared: &PreparedRegistry) -> AuditResult<()> {
    require(
        prepared.schema == "ripr.prepared-critical-claims.v1",
        "unknown prepared registry schema",
    )?;
    validate_registry(&prepared.registry)?;
    formal::digest(&prepared.registry_digest, "raw registry digest")?;
    path(&prepared.registry_path)?;
    let mut normalized = prepared.registry.clone();
    normalize(&mut normalized);
    require(
        prepared.registry_generation == digest_value(&normalized)?
            && prepared.claims.len() == normalized.claims.len(),
        "prepared registry generation/inventory mismatch",
    )?;
    for (prepared, registration) in prepared.claims.iter().zip(&normalized.claims) {
        require(
            json_bytes(&prepared.registration)? == json_bytes(registration)?
                && json_bytes(&prepared.claim)? == json_bytes(&registration.claim)?,
            "prepared claim substitution",
        )?;
        require(
            prepared.source_digest
                == native::sha256(&native::canonical_manifest_bytes(
                    &prepared.source_manifest,
                )?)
                && prepared.assumptions_digest
                    == formal::assumptions_digest(&prepared.claim.assumptions)?
                && prepared.bounds_digest == digest_value(&prepared.claim.bounds)?
                && prepared.required_cover_count == prepared.claim.required_covers.len() as u64,
            "prepared source/assumption/bounds/cover digest mismatch",
        )?;
        let canonical_inputs: Vec<_> = prepared
            .explicit_inputs
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        require(
            canonical_inputs == prepared.explicit_inputs
                && (registration.closure_rule == ClosureRule::KaniWorkspaceSourcesV1
                    || prepared.explicit_inputs.is_empty()),
            "prepared explicit input closure is duplicated, unordered or outside its provider",
        )?;
        require(
            prepared.source_manifest
                == project_source(
                    registration,
                    &prepared.source_manifest,
                    &prepared.explicit_inputs,
                )?,
            "prepared source closure differs from the governed rule",
        )?;
        for (file, digest) in &prepared.source_manifest {
            path(file)?;
            formal::digest(digest, "source digest")?;
        }
        for file in prepared
            .claim
            .source_closure
            .iter()
            .chain(&registration.authority_paths)
        {
            require(
                prepared.source_manifest.contains_key(file),
                "prepared closure is missing a declared member",
            )?;
        }
        let authority: SourceManifest = registration
            .authority_paths
            .iter()
            .filter_map(|p| {
                prepared
                    .source_manifest
                    .get(p)
                    .map(|d| (p.clone(), d.clone()))
            })
            .collect();
        require(
            prepared.authority_manifest == authority
                && prepared.semantic_family == registration.semantic_family
                && prepared.provider_profile == registration.provider_profile
                && prepared.correspondence_review == registration.correspondence_review,
            "prepared authority/profile/correspondence substitution",
        )?;
        match (&prepared.subject, &registration.subject) {
            (Some(actual), Some(expected)) => require(
                actual.id() == prepared.claim.subjects[0]
                    && actual.file == expected.file
                    && actual.qualified_name.as_ref() == Some(&expected.symbol)
                    && prepared.source_manifest.get(&actual.file) == Some(&actual.source_digest),
                "prepared native subject substitution",
            )?,
            (None, None) => {}
            _ => return Err("prepared subject presence mismatch".to_string()),
        }
    }
    Ok(())
}
