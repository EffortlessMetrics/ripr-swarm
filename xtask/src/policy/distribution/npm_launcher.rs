use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use super::contract::{DistributionContract, TargetContract};

pub(super) const MANIFEST_PATH: &str = "packaging/npm/launcher/package.json";
pub(super) const BIN_PATH: &str = "packaging/npm/launcher/bin/ripr.cjs";
pub(super) const LIBRARY_PATH: &str = "packaging/npm/launcher/lib/launcher.cjs";
pub(super) const TEST_PATH: &str = "packaging/npm/launcher/test/launcher.test.cjs";
pub(super) const TEST_RUNNER_PATH: &str = "packaging/npm/launcher/test/run-tests.test.cjs";
pub(super) const TEST_RUNNER_TEXT: &str =
    include_str!("../../../../packaging/npm/launcher/test/run-tests.test.cjs");
pub(super) const LICENSE_APACHE_PATH: &str = "packaging/npm/launcher/LICENSE-APACHE";
pub(super) const LICENSE_MIT_PATH: &str = "packaging/npm/launcher/LICENSE-MIT";

const EXPECTED_FILES: &[&str] = &[
    "LICENSE-APACHE",
    "LICENSE-MIT",
    "README.md",
    "bin/ripr.cjs",
    "lib/launcher.cjs",
];
const FORBIDDEN_LIFECYCLE_SCRIPTS: &[&str] = &[
    "preinstall",
    "install",
    "postinstall",
    "preprepare",
    "prepare",
    "postprepare",
];

#[derive(Debug, Deserialize)]
struct LauncherManifest {
    name: String,
    version: String,
    license: String,
    #[serde(rename = "type")]
    module_type: String,
    bin: BTreeMap<String, String>,
    files: Vec<String>,
    engines: BTreeMap<String, String>,
    #[serde(default)]
    scripts: BTreeMap<String, String>,
    #[serde(rename = "publishConfig")]
    publish_config: PublishConfig,
    #[serde(rename = "optionalDependencies")]
    optional_dependencies: BTreeMap<String, String>,
    ripr: LauncherMetadata,
}

#[derive(Debug, Deserialize)]
struct PublishConfig {
    access: String,
    #[serde(default)]
    tag: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LauncherMetadata {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    product: String,
    executable: String,
    platforms: Vec<LauncherPlatform>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
struct LauncherPlatform {
    #[serde(rename = "nodePlatform")]
    node_platform: String,
    #[serde(rename = "nodeArch")]
    node_arch: String,
    libc: Option<String>,
    #[serde(rename = "rustTarget")]
    rust_target: String,
    package: String,
    executable: String,
}

pub(super) fn validate_launcher(
    path: &str,
    text: &str,
    workspace_version: &str,
    contract: &DistributionContract,
    violations: &mut Vec<String>,
) {
    let manifest: LauncherManifest = match serde_json::from_str(text) {
        Ok(manifest) => manifest,
        Err(err) => {
            violations.push(format!(
                "{path}: invalid npm launcher package manifest: {err}"
            ));
            return;
        }
    };

    check_equal(
        path,
        "name",
        &manifest.name,
        &contract.npm.launcher,
        violations,
    );
    check_equal(
        path,
        "version",
        &manifest.version,
        workspace_version,
        violations,
    );
    check_equal(
        path,
        "license",
        &manifest.license,
        "MIT OR Apache-2.0",
        violations,
    );
    check_equal(path, "type", &manifest.module_type, "commonjs", violations);
    check_equal(
        path,
        "bin.ripr",
        manifest
            .bin
            .get("ripr")
            .map(String::as_str)
            .unwrap_or_default(),
        "bin/ripr.cjs",
        violations,
    );
    if manifest.bin.len() != 1 {
        violations.push(format!(
            "{path}: bin must contain only the public `ripr` executable, got {:?}",
            manifest.bin.keys().collect::<Vec<_>>()
        ));
    }

    let actual_files = manifest
        .files
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let expected_files = EXPECTED_FILES.iter().copied().collect::<BTreeSet<_>>();
    if actual_files.len() != manifest.files.len() {
        violations.push(format!("{path}: files contains duplicate entries"));
    }
    if actual_files != expected_files {
        violations.push(format!(
            "{path}: files must be the explicit launcher package set {:?}, got {:?}",
            expected_files, actual_files
        ));
    }

    check_equal(
        path,
        "engines.node",
        manifest
            .engines
            .get("node")
            .map(String::as_str)
            .unwrap_or_default(),
        ">=20",
        violations,
    );
    check_equal(
        path,
        "publishConfig.access",
        &manifest.publish_config.access,
        "public",
        violations,
    );
    validate_publish_tag(
        path,
        &manifest.version,
        manifest.publish_config.tag.as_deref(),
        violations,
    );
    for script in FORBIDDEN_LIFECYCLE_SCRIPTS {
        if manifest.scripts.contains_key(*script) {
            violations.push(format!(
                "{path}: scripts.{script} is forbidden; installation cannot depend on npm lifecycle scripts"
            ));
        }
    }
    check_equal(
        path,
        "scripts.test",
        manifest
            .scripts
            .get("test")
            .map(String::as_str)
            .unwrap_or_default(),
        "node test/run-tests.test.cjs",
        violations,
    );

    if manifest.ripr.schema_version != 1 {
        violations.push(format!(
            "{path}: ripr.schemaVersion must be 1, got {}",
            manifest.ripr.schema_version
        ));
    }
    check_equal(
        path,
        "ripr.product",
        &manifest.ripr.product,
        &contract.product.name,
        violations,
    );
    check_equal(
        path,
        "ripr.executable",
        &manifest.ripr.executable,
        &contract.npm.executable,
        violations,
    );

    let expected_dependencies = contract
        .target
        .iter()
        .map(|target| (target.npm_package.as_str(), workspace_version))
        .collect::<BTreeMap<_, _>>();
    let actual_dependencies = manifest
        .optional_dependencies
        .iter()
        .map(|(name, version)| (name.as_str(), version.as_str()))
        .collect::<BTreeMap<_, _>>();
    if actual_dependencies != expected_dependencies {
        violations.push(format!(
            "{path}: optionalDependencies must equal the five exact-version native packages {:?}, got {:?}",
            expected_dependencies, actual_dependencies
        ));
    }

    let mut expected_platforms = contract
        .target
        .iter()
        .map(expected_platform)
        .collect::<Vec<_>>();
    expected_platforms.sort();
    let mut actual_platforms = manifest.ripr.platforms.clone();
    actual_platforms.sort();
    if actual_platforms.windows(2).any(|rows| rows[0] == rows[1]) {
        violations.push(format!("{path}: ripr.platforms contains duplicate rows"));
    }
    if actual_platforms != expected_platforms {
        violations.push(format!(
            "{path}: ripr.platforms must project policy/distribution.toml exactly; expected {expected_platforms:?}, got {actual_platforms:?}"
        ));
    }
}

/// A SemVer prerelease (version containing `-`) must publish under an explicit
/// non-`latest` dist-tag; a stable version must not be diverted off `latest`.
pub(super) fn validate_publish_tag(
    path: &str,
    version: &str,
    tag: Option<&str>,
    violations: &mut Vec<String>,
) {
    let is_prerelease = version.contains('-');
    match (is_prerelease, tag) {
        (true, None) => violations.push(format!(
            "{path}: prerelease version `{version}` must set publishConfig.tag to an explicit non-`latest` dist-tag so a plain `npm publish` cannot take `latest`"
        )),
        (true, Some(tag)) if tag.trim().is_empty() || tag.trim() == "latest" => {
            violations.push(format!(
                "{path}: prerelease version `{version}` must not publish with publishConfig.tag `{tag}`; use a non-`latest` dist-tag such as `next`"
            ));
        }
        (false, Some(tag)) if tag.trim() != "latest" => violations.push(format!(
            "{path}: stable version `{version}` must not set publishConfig.tag `{tag}`; stable releases take `latest` only through the reviewed operator step"
        )),
        _ => {}
    }
}

const COMMAND_SEPARATORS: &[&str] = &["&&", "||", ";", "|", "&"];

/// Joins shell `\`-continued lines so a command split across physical lines is
/// evaluated as one logical command. Returns each logical line with the
/// one-based number of its first physical line.
fn logical_lines(text: &str) -> Vec<(usize, String)> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut start = 0;
    for (index, raw) in text.lines().enumerate() {
        if current.is_empty() {
            start = index + 1;
        }
        let code = raw.trim_end_matches('\r');
        if let Some(stripped) = code.strip_suffix('\\') {
            current.push_str(stripped);
            current.push(' ');
            continue;
        }
        current.push_str(code);
        lines.push((start, std::mem::take(&mut current)));
    }
    if !current.is_empty() {
        lines.push((start, current));
    }
    lines
}

/// A dist-tag is acceptable only when it is a literal, valid, non-`latest`
/// word. Variables, expressions, command substitution and quoting tricks are
/// rejected because they can expand to `latest` at run time.
fn literal_non_latest_tag(token: &str) -> Result<(), String> {
    let value = token.trim_matches(|c| c == '"' || c == '\'');
    let valid = value
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !valid {
        return Err(format!(
            "tag `{token}` is not a literal dist-tag (variables and expressions can expand to `latest`)"
        ));
    }
    if value.eq_ignore_ascii_case("latest") {
        return Err("tag `latest` is forbidden".to_string());
    }
    Ok(())
}

/// Fails when a workflow publishes or tags npm without exactly one literal
/// non-`latest` tag. Backslash-continued lines are joined first. In-tree
/// workflows have no npm publish step; this keeps it that way unless the step
/// names its tag, and rejects `latest` outright.
pub(super) fn validate_workflow_npm_tags(path: &str, text: &str, violations: &mut Vec<String>) {
    for (number, line) in logical_lines(text) {
        let code = line.split(" #").next().unwrap_or_default();
        let tokens = code.split_whitespace().collect::<Vec<_>>();
        for (index, token) in tokens.iter().enumerate() {
            if *token != "npm" {
                continue;
            }
            let rest = &tokens[index + 1..];
            let end = rest
                .iter()
                .position(|t| COMMAND_SEPARATORS.contains(t))
                .unwrap_or(rest.len());
            let command = &rest[..end];
            let Some(subcommand) = command.iter().position(|t| !t.starts_with('-')) else {
                continue;
            };
            let args = &command[subcommand + 1..];
            match command[subcommand] {
                "publish" => check_publish_args(path, number, args, violations),
                "dist-tag" => check_dist_tag_args(path, number, args, violations),
                _ => {}
            }
        }
    }
}

fn check_publish_args(path: &str, number: usize, args: &[&str], violations: &mut Vec<String>) {
    let mut tags = Vec::new();
    let mut position = 0;
    while position < args.len() {
        let arg = args[position];
        if arg == "--tag" {
            tags.push(args.get(position + 1).copied().unwrap_or_default());
            position += 1;
        } else if let Some(value) = arg.strip_prefix("--tag=") {
            tags.push(value);
        }
        position += 1;
    }
    match tags.as_slice() {
        [] => violations.push(format!(
            "{path}:{number}: `npm publish` must pass an explicit non-`latest` --tag"
        )),
        [tag] => {
            if let Err(reason) = literal_non_latest_tag(tag) {
                violations.push(format!("{path}:{number}: `npm publish` {reason}"));
            }
        }
        _ => violations.push(format!(
            "{path}:{number}: `npm publish` must pass exactly one --tag; repeated or mixed --tag options are ambiguous"
        )),
    }
}

fn check_dist_tag_args(path: &str, number: usize, args: &[&str], violations: &mut Vec<String>) {
    if let ["add" | "set", _package, tag, ..] = args {
        if let Err(reason) = literal_non_latest_tag(tag) {
            violations.push(format!("{path}:{number}: `npm dist-tag` {reason}"));
        }
    }
}

pub(super) fn validate_license_copy(
    root_path: &str,
    root_text: &str,
    package_path: &str,
    package_text: &str,
    violations: &mut Vec<String>,
) {
    if package_text != root_text {
        violations.push(format!(
            "{package_path}: license copy must be byte-identical to {root_path}"
        ));
    }
}

pub(super) fn validate_launcher_sources(
    bin_path: &str,
    bin_text: &str,
    library_path: &str,
    library_text: &str,
    test_path: &str,
    test_text: &str,
    violations: &mut Vec<String>,
) {
    if !bin_text.starts_with("#!/usr/bin/env node\n") {
        violations.push(format!(
            "{bin_path}: launcher bin must use the Node env shebang"
        ));
    }
    for (needle, explanation) in [
        ("shell: false", "spawn must disable shell interpretation"),
        (
            "stdio: \"inherit\"",
            "native stdio must be inherited without wrapper output",
        ),
        (
            "createRequire",
            "native packages must resolve relative to the launcher",
        ),
        (
            "realpathSync",
            "native package/executable confinement must use real paths",
        ),
        (
            "optional dependencies enabled",
            "missing-payload recovery must be actionable",
        ),
        (
            "process.exitCode = signalExitCode",
            "a consumed child signal must still produce a nonzero launcher exit",
        ),
        (
            "SIGNALS_TO_OBSERVE",
            "terminal process-group signals must be observed without duplicate forwarding",
        ),
        (
            "const effectiveSignal = receivedSignal || signal",
            "the first observed termination signal must survive a graceful child exit",
        ),
    ] {
        if !library_text.contains(needle) {
            violations.push(format!("{library_path}: {explanation}; missing `{needle}`"));
        }
    }
    for forbidden in ["curl ", "wget ", "https.get", "execSync(", "shell: true"] {
        for (path, text) in [(bin_path, bin_text), (library_path, library_text)] {
            if text.contains(forbidden) {
                violations.push(format!(
                    "{path}: launcher cannot download, shell out, or enable shell interpolation; found `{forbidden}`"
                ));
            }
        }
    }
    validate_test_runner(
        TEST_RUNNER_PATH,
        TEST_RUNNER_TEXT,
        test_path,
        test_text,
        violations,
    );
}

const REQUIRED_EXECUTED_TESTS: &[&str] = &[
    "non-executable native payload gives exact recovery for each POSIX target",
    "public bin non-executable recovery survives a directory move and restores launch",
    "distinguishes native resolution failures from missing optional packages",
    "rejects non-object native manifests with a contextual typed error",
    "public bin identifies corrupt package metadata and recovers after restoration",
    "public bin retains missing-manifest and missing-exports reasons without claiming package absence",
    "rejects lifecycle scripts, version ranges, and dependency drift",
    "rejects missing, wrong-version, wrong-target, traversal, symlink, directory, and non-executable payloads",
    "forwards direct SIGTERM to native child exactly once and re-emits signal",
    "retains the first observed signal when a supervisor escalates SIGINT to SIGTERM",
    "observes terminal SIGINT and SIGHUP without forwarding duplicates to the native child",
    "source bin missing-package failure never falls back to PATH or writes stdout",
    "npm package contents are explicit and exclude tests and build residue",
];

pub(super) fn validate_test_runner(
    runner_path: &str,
    runner_text: &str,
    test_path: &str,
    test_text: &str,
    violations: &mut Vec<String>,
) {
    for (needle, explanation) in [
        (
            "--test-reporter=tap",
            "runner must consume a machine-readable test report",
        ),
        (
            "result.status !== 0",
            "runner must propagate the Node test process failure",
        ),
        (
            "missing.length > 0",
            "runner must fail when a required named control did not pass",
        ),
        (
            "SKIP|TODO",
            "runner must not count skipped or todo controls as passed",
        ),
    ] {
        if !runner_text.contains(needle) {
            violations.push(format!("{runner_path}: {explanation}; missing `{needle}`"));
        }
    }
    for required_test in REQUIRED_EXECUTED_TESTS {
        let quoted = format!("\"{required_test}\"");
        if !runner_text.contains(&quoted) {
            violations.push(format!(
                "{runner_path}: required executed-control admission is missing `{required_test}`"
            ));
        }
        if !test_text.contains(required_test) {
            violations.push(format!(
                "{test_path}: required launcher control is missing `{required_test}`"
            ));
        }
    }
}

fn expected_platform(target: &TargetContract) -> LauncherPlatform {
    LauncherPlatform {
        node_platform: target.npm_os.clone(),
        node_arch: target.npm_cpu.clone(),
        libc: target.npm_libc.clone(),
        rust_target: target.rust_target.clone(),
        package: target.npm_package.clone(),
        executable: format!("bin/{}", target.executable),
    }
}

fn check_equal(
    path: &str,
    field: &str,
    actual: &str,
    expected: &str,
    violations: &mut Vec<String>,
) {
    if actual != expected {
        violations.push(format!(
            "{path}: {field} must be `{expected}`, got `{actual}`"
        ));
    }
}
