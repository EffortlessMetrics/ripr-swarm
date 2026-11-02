//! Pure projection of existing Rust/Cargo authorities into `ProjectFactsV1`
//! (#6423). Cargo metadata and the workspace source-role model remain the
//! only semantic owners; this module adds no second metadata probe.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::analysis::workspace::{
    SourceRole, SourceRoleContext, classify_with, context_for_files, normalize_path,
};
use crate::config::RiprConfig;
use crate::domain::{
    CanonicalSourceRole, FactCurrentnessV1, PROJECT_FACTS_SCHEMA_VERSION, ProjectFactsV1,
    SourceUnitFactV1, TestEnvironmentFactV1,
};

const INPUT_WORKSPACE_MANIFESTS: &str = "workspace_manifests";
const INPUT_LOCKFILE: &str = "lockfile";
const INPUT_TOOLCHAIN: &str = "toolchain";
const INPUT_CARGO_CONFIG: &str = "cargo_config";

/// Project the analyzed workspace's Rust/Cargo facts into the shared contract.
pub(crate) fn project_rust_cargo_facts(
    workspace_root: &Path,
    config: &RiprConfig,
) -> Result<ProjectFactsV1, String> {
    let excluded = excluded_workspace_prefixes(workspace_root)?;
    let files = discover_rust_sources(workspace_root, &excluded)?;
    let mut context = context_for_files(workspace_root, files.iter().map(PathBuf::as_path));
    apply_config_overlays(config, &mut context);
    let repository_identity = repository_layout_identity(workspace_root)?;
    let config_input_keys = workspace_config_input_keys(workspace_root);
    let package_dirs = package_manifest_dirs(workspace_root, &files)?;
    let mut package_ids: Vec<String> = package_dirs.values().cloned().collect();
    package_ids.sort();
    package_ids.dedup();
    let mut source_units = Vec::new();
    for file in &files {
        let normalized = normalize_path(file);
        let role = classify_with(Path::new(&normalized), &context);
        let package_id = package_dirs
            .get(&normalized)
            .cloned()
            .unwrap_or_else(|| ".".to_string());
        let canonical = map_source_role(role);
        let mut unit_keys = BTreeSet::from([INPUT_WORKSPACE_MANIFESTS.to_string()]);
        if workspace_root.join("Cargo.lock").is_file() {
            unit_keys.insert(INPUT_LOCKFILE.to_string());
        }
        if has_toolchain_file(workspace_root) {
            unit_keys.insert(INPUT_TOOLCHAIN.to_string());
        }
        if config_input_keys.contains(INPUT_CARGO_CONFIG) {
            unit_keys.insert(INPUT_CARGO_CONFIG.to_string());
        }
        source_units.push(SourceUnitFactV1 {
            path: normalized,
            package_id,
            role: canonical.role,
            native_role: canonical.native_role,
            basis: canonical.basis,
            currentness: FactCurrentnessV1 {
                input_keys: unit_keys,
            },
            limitations: Vec::new(),
        });
    }
    let test_environments = build_test_environments(workspace_root, &package_dirs)?;
    ProjectFactsV1::seal(ProjectFactsV1 {
        schema_version: PROJECT_FACTS_SCHEMA_VERSION.to_string(),
        language: "rust".to_string(),
        repository_identity,
        repository_root_cwd: ".".to_string(),
        package_ids,
        source_units,
        test_environments,
        config_input_keys: config_input_keys.clone(),
        currentness: FactCurrentnessV1 {
            input_keys: config_input_keys,
        },
        limitations: Vec::new(),
        semantic_digest: String::new(),
    })
}

struct MappedRole {
    role: CanonicalSourceRole,
    native_role: Option<String>,
    basis: String,
}

fn map_source_role(role: SourceRole) -> MappedRole {
    match role {
        SourceRole::ProductionSubject => MappedRole {
            role: CanonicalSourceRole::Production,
            native_role: None,
            basis: "rust_source_role:production_subject".to_string(),
        },
        SourceRole::ProductionLikeTestInfrastructure => MappedRole {
            role: CanonicalSourceRole::TestHelper,
            native_role: Some("production_like_target".to_string()),
            basis: "rust_source_role:production_like".to_string(),
        },
        SourceRole::TestEvidence => MappedRole {
            role: CanonicalSourceRole::Test,
            native_role: None,
            basis: "rust_source_role:test_evidence".to_string(),
        },
        SourceRole::BenchEvidence => MappedRole {
            role: CanonicalSourceRole::Test,
            native_role: Some("bench_evidence".to_string()),
            basis: "rust_source_role:bench_evidence".to_string(),
        },
        SourceRole::ExampleEvidence => MappedRole {
            role: CanonicalSourceRole::Example,
            native_role: None,
            basis: "rust_source_role:example_evidence".to_string(),
        },
        SourceRole::FixtureOrReceiptEvidence => MappedRole {
            role: CanonicalSourceRole::Fixture,
            native_role: None,
            basis: "rust_source_role:fixture_or_receipt".to_string(),
        },
        #[cfg(test)]
        SourceRole::UnknownRole => MappedRole {
            role: CanonicalSourceRole::Unknown,
            native_role: None,
            basis: "rust_source_role:unknown".to_string(),
        },
    }
}

fn apply_config_overlays(config: &RiprConfig, context: &mut SourceRoleContext) {
    for target in &config.analysis.production_like_targets {
        context
            .production_like_targets
            .insert(PathBuf::from(normalize_path(target)));
    }
    for harness in &config.analysis.test_harnesses {
        context
            .harness_targets
            .insert(PathBuf::from(normalize_path(&harness.target)));
    }
}

fn discover_rust_sources(
    workspace_root: &Path,
    excluded: &BTreeSet<String>,
) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    walk_rust_files(workspace_root, workspace_root, excluded, &mut files)?;
    files.sort();
    Ok(files)
}

fn excluded_workspace_prefixes(workspace_root: &Path) -> Result<BTreeSet<String>, String> {
    let manifest = workspace_root.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok(BTreeSet::new());
    }
    let text = fs::read_to_string(&manifest)
        .map_err(|error| format!("read workspace manifest: {error}"))?;
    let value: toml::Value =
        toml::from_str(&text).map_err(|error| format!("parse workspace manifest: {error}"))?;
    let Some(workspace) = value.get("workspace") else {
        return Ok(BTreeSet::new());
    };
    let mut prefixes = BTreeSet::new();
    if let Some(array) = workspace.get("exclude").and_then(|entry| entry.as_array()) {
        for entry in array {
            if let Some(prefix) = entry.as_str() {
                prefixes.insert(normalize_path(Path::new(prefix)));
            }
        }
    }
    Ok(prefixes)
}

fn path_is_excluded(relative: &Path, excluded: &BTreeSet<String>) -> bool {
    let normalized = normalize_path(relative);
    excluded
        .iter()
        .any(|prefix| normalized == *prefix || normalized.starts_with(&format!("{prefix}/")))
}

fn walk_rust_files(
    workspace_root: &Path,
    directory: &Path,
    excluded: &BTreeSet<String>,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("project fact discovery failed for {directory:?}: {error}"))?;
    let mut collected: Vec<_> = entries.filter_map(Result::ok).collect();
    collected.sort_by_key(|entry| entry.file_name());
    for entry in collected {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            if name == "target" || name == ".git" {
                continue;
            }
            let relative = path
                .strip_prefix(workspace_root)
                .map_err(|_| format!("directory {path:?} escapes workspace root"))?;
            if path_is_excluded(relative, excluded) {
                continue;
            }
            walk_rust_files(workspace_root, &path, excluded, files)?;
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "rs") {
            let relative = path
                .strip_prefix(workspace_root)
                .map_err(|_| format!("path {path:?} escapes workspace root"))?;
            files.push(relative.to_path_buf());
        }
    }
    Ok(())
}

fn package_manifest_dirs(
    workspace_root: &Path,
    files: &[PathBuf],
) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();
    for file in files {
        let normalized = normalize_path(file);
        let anchored = workspace_root.join(file);
        let package_dir = owning_package_dir(workspace_root, &anchored)
            .map(|dir| normalize_package_dir(workspace_root, &dir))
            .unwrap_or_else(|| ".".to_string());
        map.insert(normalized, package_dir);
    }
    Ok(map)
}

fn normalize_package_dir(workspace_root: &Path, package_dir: &Path) -> String {
    let relative = package_dir
        .strip_prefix(workspace_root)
        .unwrap_or(package_dir);
    let text = normalize_path(relative);
    if text.is_empty() {
        ".".to_string()
    } else {
        text
    }
}

fn owning_package_dir(workspace_root: &Path, file: &Path) -> Option<PathBuf> {
    let mut directory = file.parent()?.to_path_buf();
    loop {
        if directory.join("Cargo.toml").is_file() {
            return Some(directory);
        }
        if directory == workspace_root || !directory.pop() {
            break;
        }
    }
    None
}

fn repository_layout_identity(workspace_root: &Path) -> Result<String, String> {
    let manifest = workspace_root.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok("layout:non-cargo".to_string());
    }
    let text = fs::read_to_string(&manifest)
        .map_err(|error| format!("read workspace manifest: {error}"))?;
    let value: toml::Value =
        toml::from_str(&text).map_err(|error| format!("parse workspace manifest: {error}"))?;
    if value.get("workspace").is_some() {
        let members = workspace_member_identities(&value);
        return Ok(format!("layout:workspace:{}", members.join(",")));
    }
    let name = value
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(|name| name.as_str())
        .unwrap_or("unnamed");
    Ok(format!("layout:package:{name}"))
}

fn workspace_member_identities(manifest: &toml::Value) -> Vec<String> {
    let Some(workspace) = manifest.get("workspace") else {
        return Vec::new();
    };
    let mut members = Vec::new();
    if let Some(array) = workspace.get("members").and_then(|value| value.as_array()) {
        for entry in array {
            if let Some(member) = entry.as_str() {
                members.push(member.to_string());
            }
        }
    }
    members.sort();
    members
}

fn workspace_config_input_keys(workspace_root: &Path) -> BTreeSet<String> {
    let mut keys = BTreeSet::from([INPUT_WORKSPACE_MANIFESTS.to_string()]);
    if workspace_root.join("Cargo.lock").is_file() {
        keys.insert(INPUT_LOCKFILE.to_string());
    }
    if has_toolchain_file(workspace_root) {
        keys.insert(INPUT_TOOLCHAIN.to_string());
    }
    if workspace_root.join(".cargo").is_dir() {
        keys.insert(INPUT_CARGO_CONFIG.to_string());
    }
    keys
}

fn has_toolchain_file(workspace_root: &Path) -> bool {
    workspace_root.join("rust-toolchain.toml").is_file()
        || workspace_root.join("rust-toolchain").is_file()
}

fn build_test_environments(
    workspace_root: &Path,
    package_dirs: &BTreeMap<String, String>,
) -> Result<Vec<TestEnvironmentFactV1>, String> {
    let mut environments = BTreeMap::new();
    for package_dir in package_dirs.values() {
        let required_cwd = if package_dir == "." {
            ".".to_string()
        } else {
            package_dir.clone()
        };
        let environment_id = format!("cargo-test:{required_cwd}");
        environments.insert(
            environment_id.clone(),
            TestEnvironmentFactV1 {
                environment_id,
                framework: "libtest".to_string(),
                runner: "cargo".to_string(),
                package_root: package_dir.clone(),
                required_cwd,
                runtime_class: "rust_toolchain".to_string(),
                config_input_keys: workspace_config_input_keys(workspace_root),
                limitations: Vec::new(),
            },
        );
    }
    Ok(environments.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("ripr-project-facts-{label}-{nanos}"))
    }

    fn write(path: &Path, contents: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("mkdir {parent:?}: {error}"))?;
        }
        fs::write(path, contents).map_err(|error| format!("write {path:?}: {error}"))
    }

    #[test]
    fn single_package_projects_production_and_integration_test_rows() -> Result<(), String> {
        let dir = unique_dir("single");
        fs::create_dir_all(&dir).map_err(|error| format!("{error}"))?;
        write(
            &dir.join("Cargo.toml"),
            "[package]\nname = \"solo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        write(&dir.join("src/lib.rs"), "pub fn value() -> i32 { 1 }\n")?;
        write(
            &dir.join("tests/smoke.rs"),
            "#[test]\nfn smoke() { assert_eq!(solo::value(), 1); }\n",
        )?;
        let config = RiprConfig::default();
        let facts = project_rust_cargo_facts(&dir, &config)?;
        assert_eq!(facts.language, "rust");
        assert!(
            facts
                .source_units
                .iter()
                .any(|unit| unit.path == "src/lib.rs"
                    && unit.role == CanonicalSourceRole::Production)
        );
        assert!(facts.source_units.iter().any(|unit| {
            unit.path == "tests/smoke.rs" && unit.role == CanonicalSourceRole::Test
        }));
        assert_eq!(facts.repository_root_cwd, ".");
        assert!(
            facts
                .test_environments
                .iter()
                .any(|env| env.required_cwd == "." && env.runner == "cargo")
        );
        assert_ne!(facts.semantic_digest, "");
        Ok(())
    }

    #[test]
    fn workspace_member_test_cwd_stays_package_local() -> Result<(), String> {
        let dir = unique_dir("workspace");
        fs::create_dir_all(&dir).map_err(|error| format!("{error}"))?;
        write(
            &dir.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/a\"]\nresolver = \"2\"\n",
        )?;
        write(
            &dir.join("crates/a/Cargo.toml"),
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        write(
            &dir.join("crates/a/src/lib.rs"),
            "pub fn a() -> i32 { 1 }\n",
        )?;
        write(
            &dir.join("crates/a/tests/it.rs"),
            "#[test]\nfn it_works() { assert_eq!(a::a(), 1); }\n",
        )?;
        let facts = project_rust_cargo_facts(&dir, &RiprConfig::default())?;
        let member_env = facts
            .test_environments
            .iter()
            .find(|env| env.package_root == "crates/a")
            .ok_or("missing member test environment")?;
        assert_eq!(member_env.required_cwd, "crates/a");
        assert_ne!(member_env.required_cwd, facts.repository_root_cwd);
        Ok(())
    }

    #[test]
    fn example_and_build_script_roles_project_honestly() -> Result<(), String> {
        let dir = unique_dir("example-build");
        fs::create_dir_all(&dir).map_err(|error| format!("{error}"))?;
        write(
            &dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        write(&dir.join("src/lib.rs"), "pub fn demo() -> i32 { 0 }\n")?;
        write(
            &dir.join("examples/sample.rs"),
            "fn main() { let _ = demo::demo(); }\n",
        )?;
        write(
            &dir.join("build.rs"),
            "fn main() { println!(\"cargo:rerun-if-changed=build.rs\"); }\n",
        )?;
        let facts = project_rust_cargo_facts(&dir, &RiprConfig::default())?;
        assert!(facts.source_units.iter().any(|unit| {
            unit.path == "examples/sample.rs" && unit.role == CanonicalSourceRole::Example
        }));
        assert!(
            facts.source_units.iter().any(|unit| {
                unit.path == "build.rs" && unit.role == CanonicalSourceRole::Fixture
            })
        );
        Ok(())
    }

    #[test]
    fn lockfile_input_key_appears_only_when_lock_present() -> Result<(), String> {
        let dir = unique_dir("lock");
        fs::create_dir_all(&dir).map_err(|error| format!("{error}"))?;
        write(
            &dir.join("Cargo.toml"),
            "[package]\nname = \"locky\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        write(&dir.join("src/lib.rs"), "pub fn locky() -> i32 { 0 }\n")?;
        let without = project_rust_cargo_facts(&dir, &RiprConfig::default())?;
        assert!(!without.currentness.input_keys.contains(INPUT_LOCKFILE));
        write(&dir.join("Cargo.lock"), "# stub lock for projection test\n")?;
        let with = project_rust_cargo_facts(&dir, &RiprConfig::default())?;
        assert!(with.currentness.input_keys.contains(INPUT_LOCKFILE));
        Ok(())
    }

    #[test]
    fn excluded_workspace_member_stays_out_of_package_ids() -> Result<(), String> {
        let dir = unique_dir("excluded");
        fs::create_dir_all(&dir).map_err(|error| format!("{error}"))?;
        write(
            &dir.join("Cargo.toml"),
            "[workspace]\nmembers = [\"keep\"]\nexclude = [\"drop\"]\nresolver = \"2\"\n",
        )?;
        write(
            &dir.join("keep/Cargo.toml"),
            "[package]\nname = \"keep\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        write(&dir.join("keep/src/lib.rs"), "pub fn keep() -> i32 { 1 }\n")?;
        write(
            &dir.join("drop/Cargo.toml"),
            "[package]\nname = \"drop\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        write(
            &dir.join("drop/src/lib.rs"),
            "pub fn drop_value() -> i32 { 0 }\n",
        )?;
        let facts = project_rust_cargo_facts(&dir, &RiprConfig::default())?;
        assert!(facts.package_ids.iter().all(|id| id != "drop"));
        assert!(
            !facts
                .source_units
                .iter()
                .any(|unit| unit.path.starts_with("drop/"))
        );
        Ok(())
    }

    #[test]
    fn portable_digest_matches_across_relocated_checkout_roots() -> Result<(), String> {
        let left = unique_dir("reloc-left");
        let right = unique_dir("reloc-right");
        for dir in [&left, &right] {
            fs::create_dir_all(dir).map_err(|error| format!("{error}"))?;
            write(
                &dir.join("Cargo.toml"),
                "[package]\nname = \"reloc\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            )?;
            write(&dir.join("src/lib.rs"), "pub fn reloc() -> i32 { 7 }\n")?;
        }
        let left_facts = project_rust_cargo_facts(&left, &RiprConfig::default())?;
        let right_facts = project_rust_cargo_facts(&right, &RiprConfig::default())?;
        assert_eq!(
            left_facts.repository_identity,
            right_facts.repository_identity
        );
        assert_eq!(left_facts.semantic_digest, right_facts.semantic_digest);
        Ok(())
    }
}
