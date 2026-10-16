//! Bounded native output-set custody and publication. No replay artifact is used.

use crate::app::{CheckInput, OutputFormat};
use crate::config::RiprConfig;
use crate::core_error::CoreError;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 16_384;
const MAX_INPUT_BYTES: u64 = 128 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
pub(super) const MEMBERS: [(OutputFormat, &str); 3] = [
    (OutputFormat::BadgeJson, "badge.json"),
    (OutputFormat::Json, "detail.json"),
    (OutputFormat::Human, "human.txt"),
];

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Observation {
    digest: String,
    modified: Option<std::time::SystemTime>,
}

/// Observed persistent drift guard, not an immutable filesystem or replay cache.
/// The regular-file inventory also catches added/deleted presentation inputs.
pub(super) struct Custody {
    root: PathBuf,
    requested_root: PathBuf,
    destination: PathBuf,
    requested_destination: PathBuf,
    boundary: crate::analysis::input_boundary::Boundary,
    extras: Vec<PathBuf>,
    files: BTreeMap<PathBuf, Option<Observation>>,
    today: String,
    cargo: String,
    budget: Option<std::ffi::OsString>,
}

impl Custody {
    pub(super) fn capture(input: &CheckInput, destination: &Path) -> Result<Self, String> {
        let root = input.root.canonicalize().map_err(|e| e.to_string())?;
        if root.ancestors().take(65).count() > 64 {
            return Err("--output-dir input ancestry exceeds depth 64".into());
        }
        let requested_destination = destination.to_path_buf();
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let destination = parent
            .canonicalize()
            .map_err(|e| format!("--output-dir parent: {e}"))?
            .join(
                destination
                    .file_name()
                    .ok_or("--output-dir requires a new directory name")?,
            );
        if std::fs::symlink_metadata(&destination).is_ok() {
            return Err(
                "--output-dir must name a new directory; existing output sets are never replaced"
                    .into(),
            );
        }
        let boundary = crate::analysis::input_boundary::Boundary::new(&input.root)?;
        let mut extras = Vec::new();
        for ancestor in root.ancestors() {
            for name in ["ripr.toml", "Cargo.toml", "Cargo.lock"] {
                extras.push(ancestor.join(name));
            }
        }
        extras.extend(input.diff_file.iter().cloned());
        extras.extend(input.perl_facts_path.iter().cloned());
        let mut custody = Self {
            root,
            requested_root: input.root.clone(),
            destination,
            requested_destination,
            boundary,
            extras,
            files: BTreeMap::new(),
            today: crate::output::suppressions::current_iso_date(),
            cargo: crate::analysis::cargo_input_identity(&input.root)
                .ok_or("--output-dir cannot capture Cargo input identity")?,
            budget: std::env::var_os("RIPR_CHECK_FINDINGS_BYTES"),
        };
        custody.files = custody.observe()?;
        Ok(custody)
    }

    pub(super) fn bind_config(&mut self, config: &RiprConfig) -> Result<(), String> {
        self.verify()?;
        if let Some(path) = config.source_path() {
            let mut remaining = MAX_INPUT_BYTES;
            let observed = observe_file(path, &mut remaining)?;
            if observed.as_ref().map(|value| &value.digest)
                != config
                    .source_text()
                    .map(|text| digest(text.as_bytes()))
                    .as_ref()
            {
                return Err("--output-dir loaded configuration drifted before binding".into());
            }
            self.extras.push(path.to_path_buf());
            if let Some(before) = self.files.insert(path.to_path_buf(), observed.clone())
                && before != observed
            {
                return Err("--output-dir configuration drifted during loading".into());
            }
        }
        if config.perl().producer().is_some() {
            return Err("--output-dir does not support configured Perl producers; pass explicit --perl-facts instead".into());
        }
        let suppression = self.root.join(config.suppressions().path());
        self.extras.push(suppression);
        let with_auxiliary = self.observe()?;
        if self
            .files
            .iter()
            .any(|(path, before)| with_auxiliary.get(path) != Some(before))
        {
            return Err("--output-dir input drift while binding presentation inputs".into());
        }
        self.files = with_auxiliary;
        Ok(())
    }

    fn observe(&self) -> Result<BTreeMap<PathBuf, Option<Observation>>, String> {
        let mut paths = self.extras.clone();
        let mut pending = vec![(self.root.clone(), 0)];
        let mut visited = 0;
        while let Some((dir, depth)) = pending.pop() {
            if depth > 64 {
                return Err("--output-dir input directory depth exceeds 64".into());
            }
            for entry in
                std::fs::read_dir(&dir).map_err(|e| format!("inspect {}: {e}", dir.display()))?
            {
                let entry = entry.map_err(|e| e.to_string())?;
                visited += 1;
                if visited > MAX_ENTRIES {
                    return Err("--output-dir input inventory exceeds 16384 entries".into());
                }
                let path = entry.path();
                if path == self.destination {
                    continue;
                }
                let kind = entry.file_type().map_err(|e| e.to_string())?;
                // Match the native module-escape inventory boundary, not the
                // narrower default source-discovery directory exclusions.
                if entry.file_name() == ".git"
                    || (entry.file_name() == "target" && dir.join("Cargo.toml").is_file())
                {
                    continue;
                }
                if kind.is_dir() {
                    paths.push(path.clone());
                    pending.push((path, depth + 1));
                } else {
                    paths.push(path);
                }
            }
        }
        paths.sort();
        paths.dedup();
        if paths.len() > MAX_ENTRIES {
            return Err(
                "--output-dir input inventory including auxiliary paths exceeds 16384 entries"
                    .into(),
            );
        }
        let mut bytes_left = MAX_INPUT_BYTES;
        paths
            .into_iter()
            .map(|path| {
                let observed = observe_file(&path, &mut bytes_left)?;
                Ok((path, observed))
            })
            .collect()
    }

    pub(super) fn destination(&self) -> &Path {
        &self.destination
    }

    pub(super) fn verify(&self) -> Result<(), String> {
        self.boundary.verify()?;
        self.verify_date(&crate::output::suppressions::current_iso_date())
    }

    fn verify_date(&self, today: &str) -> Result<(), String> {
        let parent = self
            .requested_destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if parent.canonicalize().ok().as_deref() != self.destination.parent() {
            return Err("--output-dir destination parent changed during the operation".into());
        }
        if self.requested_root.canonicalize().ok().as_ref() != Some(&self.root)
            || today != self.today
            || self.budget != std::env::var_os("RIPR_CHECK_FINDINGS_BYTES")
            || Some(&self.cargo) != crate::analysis::cargo_input_identity(&self.root).as_ref()
            || self.files != self.observe()?
        {
            return Err("--output-dir input drift: diff, config, source, suppression, date or rendering environment changed; retry on stable inputs".into());
        }
        Ok(())
    }

    pub(super) fn identity(&self, input: &CheckInput, config: &RiprConfig) -> serde_json::Value {
        let inputs: Vec<_> = self.files.iter().map(|(path, observation)| {
            serde_json::json!({
                "path": crate::analysis::stable_path_text(path),
                "kind": match observation { None => "absent", Some(value) if value.digest == "directory" => "directory", Some(_) => "file" },
                "sha256": observation.as_ref().filter(|value| value.digest != "directory").map(|value| &value.digest),
            })
        }).collect();
        serde_json::json!({
            "producer": crate::build_identity::cache_identity(), "available_languages": available_languages(),
            "effective_input": format!("{input:?}"), "effective_config": format!("{config:?}"),
            "utc_date": self.today, "cargo_input_identity": self.cargo,
            "findings_budget": self.budget.as_ref().map(|value| value.to_string_lossy()),
            "observed_inputs": inputs,
        })
    }
}

fn available_languages() -> Vec<&'static str> {
    [
        crate::domain::LanguageId::Rust,
        crate::domain::LanguageId::Python,
        crate::domain::LanguageId::TypeScript,
        crate::domain::LanguageId::JavaScript,
        crate::domain::LanguageId::Perl,
    ]
    .into_iter()
    .filter(|language| language.is_available())
    .map(|language| language.as_str())
    .collect()
}

fn observe_file(path: &Path, bytes_left: &mut u64) -> Result<Option<Observation>, String> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("inspect {}: {e}", path.display())),
    };
    if meta.is_dir() {
        return Ok(Some(Observation {
            digest: "directory".into(),
            modified: None,
        }));
    }
    if !meta.is_file() {
        return Err(format!(
            "--output-dir requires regular input files: {}",
            path.display()
        ));
    }
    if meta.len() > *bytes_left {
        return Err("--output-dir input custody exceeds 128 MiB".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| format!("read {}: {e}", path.display()))?
        .take(*bytes_left + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > *bytes_left {
        return Err("--output-dir input custody exceeds 128 MiB".into());
    }
    *bytes_left -= bytes.len() as u64;
    let after = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.len() != after.len() || meta.modified().ok() != after.modified().ok() {
        return Err(format!(
            "--output-dir input changed while reading {}",
            path.display()
        ));
    }
    Ok(Some(Observation {
        digest: digest(&bytes),
        modified: meta.modified().ok(),
    }))
}

/// Exclusive directory + exclusive member writes + manifest last. A partial
/// write or interrupted process leaves no admissible complete-set manifest.
pub(super) fn publish(
    destination: &Path,
    documents: &[(&str, String)],
    identity: serde_json::Value,
    complete_set: bool,
    command_success: bool,
    error: Option<&str>,
    final_check: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if !crate::cli::commands_context::windows_stripped_components(destination).is_empty() {
        return Err(
            "--output-dir path contains components Windows strips; choose a literal path".into(),
        );
    }
    if documents.iter().map(|(_, text)| text.len()).sum::<usize>() > MAX_OUTPUT_BYTES {
        return Err("--output-dir rendered output exceeds 32 MiB".into());
    }
    std::fs::create_dir(destination)
        .map_err(|e| format!("create new --output-dir {}: {e}", destination.display()))?;
    let mut members = Vec::new();
    for (name, document) in documents {
        write_new(&destination.join(name), document.as_bytes())?;
        members.push(serde_json::json!({"file": name, "bytes": document.len(), "sha256": digest(document.as_bytes())}));
    }
    let manifest = serde_json::json!({
        "schema_version": "ripr-native-diff-output-set-v1", "complete_set": complete_set,
        "command_success": command_success, "error": error, "identity": identity, "members": members,
    });
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    let member_bytes = documents.iter().map(|(_, text)| text.len()).sum::<usize>();
    if bytes.len() > MAX_OUTPUT_BYTES.saturating_sub(member_bytes) {
        return Err(
            "--output-dir members plus manifest exceed 32 MiB; output set is incomplete".into(),
        );
    }
    let pending = destination.join("manifest.pending");
    write_new(&pending, &bytes)?;
    final_check()?;
    std::fs::rename(&pending, destination.join("manifest.json"))
        .map_err(|e| format!("publish manifest: {e}; output set is incomplete"))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| {
            format!(
                "create output {}: {e}; output set is incomplete",
                path.display()
            )
        })?;
    file.write_all(bytes)
        .and_then(|()| file.flush())
        .map_err(|e| {
            format!(
                "write output {}: {e}; output set is incomplete",
                path.display()
            )
        })
}

pub(super) fn refuse(
    destination: Option<&Path>,
    input: &CheckInput,
    format: OutputFormat,
    error: impl Into<CoreError>,
) -> String {
    let error = error.into();
    let Some(destination) = destination else {
        return super::check::refuse_check(input, format, error);
    };
    let mut documents = Vec::new();
    for (format, name) in MEMBERS {
        match super::check::machine_refusal_document(input, format, &error) {
            Some(Ok(document)) => documents.push((name, document)),
            Some(Err(render)) => return render,
            None => documents.push((name, String::new())),
        }
    }
    let message = error.to_string();
    match publish(
        destination,
        &documents,
        serde_json::json!({"requested_input": format!("{input:?}"), "producer": crate::build_identity::cache_identity(), "available_languages": available_languages(), "binding": "refused_incomplete"}),
        false,
        false,
        Some(&message),
        || Ok(()),
    ) {
        Ok(()) => message,
        Err(write) => format!("{message}; {write}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn custody_fixture(label: &str) -> Result<(Fixture, CheckInput, PathBuf), String> {
        let dir = super::super::tests::unique_command_test_dir(label);
        let root = dir.join("input");
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        for (name, text) in [
            ("lib.rs", "pub fn f() {}"),
            ("input.diff", "diff"),
            ("ripr.toml", "[languages]\nenabled = [\"rust\"]\n"),
            ("facts.json", "{}"),
        ] {
            std::fs::write(root.join(name), text).map_err(|e| e.to_string())?;
        }
        let input = CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("input.diff")),
            perl_facts_path: Some(root.join("facts.json")),
            ..CheckInput::default()
        };
        Ok((Fixture(dir.clone()), input, dir.join("outputs")))
    }

    #[test]
    fn fanout_detects_diff_config_suppression_source_facts_and_membership_drift()
    -> Result<(), String> {
        for name in [
            "input.diff",
            "ripr.toml",
            "lib.rs",
            "facts.json",
            ".ripr/suppressions.toml",
            "new_test.py",
        ] {
            let (_fixture, input, destination) = custody_fixture("fanout-drift")?;
            let mut custody = Custody::capture(&input, &destination)?;
            custody.bind_config(&RiprConfig::default())?;
            let path = input.root.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, "changed").map_err(|e| e.to_string())?;
            assert!(custody.verify().is_err(), "{name} drift must refuse");
        }
        let (_fixture, input, destination) = custody_fixture("fanout-date")?;
        let custody = Custody::capture(&input, &destination)?;
        assert!(custody.verify_date("0000-00-00").is_err());
        Ok(())
    }

    #[test]
    fn fanout_partial_member_manifest_and_late_drift_fail_without_completion() -> Result<(), String>
    {
        for failure in ["member", "manifest", "drift"] {
            let (_fixture, input, destination) = custody_fixture("fanout-write")?;
            let custody = Custody::capture(&input, &destination)?;
            let second = if failure == "member" {
                "missing/detail.json"
            } else {
                "detail.json"
            };
            let mut documents = vec![("badge.json", "first".into()), (second, "second".into())];
            if failure == "manifest" {
                documents.push(("manifest.pending", "collision".into()));
            }
            let result = publish(
                &destination,
                &documents,
                serde_json::Value::Null,
                true,
                true,
                None,
                || {
                    if failure == "drift" {
                        std::fs::write(input.root.join("lib.rs"), "changed")
                            .map_err(|e| e.to_string())?;
                    }
                    custody.verify()
                },
            );
            assert!(result.is_err(), "{failure} must fail");
            assert!(
                destination.join("badge.json").is_file(),
                "partial output control must be nonempty"
            );
            assert!(
                !destination.join("manifest.json").exists(),
                "{failure} must not publish completion"
            );
        }
        Ok(())
    }

    #[test]
    fn fanout_input_byte_cap_is_enforced_before_allocation() -> Result<(), String> {
        let (_fixture, input, destination) = custody_fixture("fanout-byte-cap")?;
        let file =
            std::fs::File::create(input.root.join("large.bin")).map_err(|e| e.to_string())?;
        file.set_len(MAX_INPUT_BYTES + 1)
            .map_err(|e| e.to_string())?;
        let result = Custody::capture(&input, &destination);
        assert!(matches!(result, Err(error) if error.contains("128 MiB")));
        Ok(())
    }

    #[test]
    fn fanout_output_caps_cover_members_and_manifest() -> Result<(), String> {
        let (_fixture, _input, destination) = custody_fixture("fanout-output-cap")?;
        let too_large = vec![("badge.json", "x".repeat(MAX_OUTPUT_BYTES + 1))];
        assert!(
            publish(
                &destination,
                &too_large,
                serde_json::Value::Null,
                true,
                true,
                None,
                || Ok(())
            )
            .is_err()
        );
        assert!(
            !destination.exists(),
            "oversized member must fail before publication"
        );
        let identity = serde_json::json!({"large": "\n".repeat(MAX_OUTPUT_BYTES / 2)});
        assert!(
            publish(
                &destination,
                &[("badge.json", "small".into())],
                identity,
                true,
                true,
                None,
                || Ok(())
            )
            .is_err()
        );
        assert!(
            destination.join("badge.json").exists(),
            "manifest-size partial output control"
        );
        assert!(!destination.join("manifest.json").exists());
        Ok(())
    }

    #[test]
    fn fanout_refuses_input_mutations_at_the_native_analysis_boundary() -> Result<(), String> {
        struct Mutator {
            path: PathBuf,
            result: std::cell::RefCell<Option<Result<(), String>>>,
        }
        impl crate::app::AnalysisProgressSink for Mutator {
            fn emit(&self, event: crate::app::AnalysisProgressEvent) {
                if event.stage == crate::app::AnalysisProgressStage::BuildingOutput {
                    let result = (|| {
                        if let Some(parent) = self.path.parent() {
                            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                        }
                        std::fs::write(&self.path, "changed during native analysis")
                            .map_err(|e| e.to_string())
                    })();
                    *self.result.borrow_mut() = Some(result);
                }
            }
        }
        for name in [
            "input.diff",
            "ripr.toml",
            ".ripr/suppressions.toml",
            "lib.rs",
        ] {
            let (_fixture, mut input, destination) = custody_fixture("fanout-native-drift")?;
            input.perl_facts_path = None;
            std::fs::write(input.root.join("Cargo.toml"), "[package]\nname = \"native-drift\"\nversion = \"0.0.0\"\n[lib]\npath = \"lib.rs\"\n[workspace]\n").map_err(|e| e.to_string())?;
            std::fs::write(
                input.root.join("lib.rs"),
                "pub fn positive(v: i32) -> bool { v > 0 }\n",
            )
            .map_err(|e| e.to_string())?;
            std::fs::write(input.root.join("input.diff"), "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-pub fn positive(v: i32) -> bool { v >= 0 }\n+pub fn positive(v: i32) -> bool { v > 0 }\n").map_err(|e| e.to_string())?;
            let mut custody = Custody::capture(&input, &destination)?;
            let config = crate::config::load_for_root(&input.root)?;
            custody.bind_config(&config)?;
            let before = crate::analysis::diff_pipeline_entry_count(&input.root)?;
            let sink = Mutator {
                path: input.root.join(name),
                result: std::cell::RefCell::new(None),
            };
            let output = crate::app::check_with_progress_core(
                input.clone(),
                &config,
                crate::app::AnalysisProgressScope::Diff,
                Some(&sink),
            )
            .map_err(|e| e.to_string())?;
            sink.result
                .into_inner()
                .ok_or("native mutation observer was not reached")??;
            assert!(
                !output.findings.is_empty(),
                "nonempty native analysis control"
            );
            assert_eq!(
                crate::analysis::diff_pipeline_entry_count(&input.root)? - before,
                1
            );
            assert!(custody.verify().is_err(), "{name} mutation must refuse");
            assert!(!destination.exists(), "drift must not admit an output set");
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn fanout_refuses_output_parent_identity_drift() -> Result<(), String> {
        let (fixture, input, destination) = custody_fixture("fanout-output-parent")?;
        let first = fixture.0.join("first");
        let second = fixture.0.join("second");
        let link = fixture.0.join("selected");
        std::fs::create_dir(&first).map_err(|e| e.to_string())?;
        std::fs::create_dir(&second).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(&first, &link).map_err(|e| e.to_string())?;
        let requested = link.join(destination.file_name().ok_or("missing destination name")?);
        let custody = Custody::capture(&input, &requested)?;
        assert!(custody.verify().is_ok());
        std::fs::remove_file(&link).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(&second, &link).map_err(|e| e.to_string())?;
        assert!(custody.verify().is_err());
        assert_eq!(custody.destination(), first.join("outputs"));
        assert!(!first.join("outputs").exists());
        assert!(!second.join("outputs").exists());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn fanout_input_identity_keeps_non_utf8_paths_without_panicking() -> Result<(), String> {
        use std::os::unix::ffi::OsStringExt;
        let (_fixture, input, destination) = custody_fixture("fanout-path-bytes")?;
        let name = std::ffi::OsString::from_vec(b"source-\xff.rs".to_vec());
        std::fs::write(input.root.join(name), "pub fn f() {}").map_err(|e| e.to_string())?;
        let custody = Custody::capture(&input, &destination)?;
        let identity = custody.identity(&input, &RiprConfig::default());
        let text = serde_json::to_string(&identity).map_err(|e| e.to_string())?;
        assert!(text.contains("observed_inputs"));
        assert!(custody.verify().is_ok());
        Ok(())
    }

    #[test]
    fn fanout_enters_diff_pipeline_once() -> Result<(), String> {
        let dir = super::super::tests::unique_command_test_dir("fanout");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let _guard = Fixture(dir.clone());
        let root = dir.join("input");
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"fanout-unit\"\nversion = \"0.0.0\"\n[lib]\npath = \"lib.rs\"\n[workspace]\n").map_err(|e| e.to_string())?;
        std::fs::write(root.join("lib.rs"), "pub fn value() -> bool { true }\n")
            .map_err(|e| e.to_string())?;
        std::fs::write(
            root.join("ripr.toml"),
            "[languages]\nenabled = [\"rust\"]\n",
        )
        .map_err(|e| e.to_string())?;
        let diff = dir.join("input.diff");
        std::fs::write(&diff, "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-pub fn value() -> bool { false }\n+pub fn value() -> bool { true }\n")
            .map_err(|e| e.to_string())?;
        let destination = dir.join("outputs");
        let before = crate::analysis::diff_pipeline_entry_count(&root)?;
        let result = super::super::check::check(&[
            "--root".into(),
            root.display().to_string(),
            "--diff".into(),
            diff.display().to_string(),
            "--output-dir".into(),
            destination.display().to_string(),
            "--quiet".into(),
        ]);
        assert!(result.is_ok(), "native output set must run: {result:?}");
        assert!(destination.join("manifest.json").is_file());
        assert_eq!(
            crate::analysis::diff_pipeline_entry_count(&root)? - before,
            1,
            "fan-out must enter the actual diff pipeline exactly once"
        );
        Ok(())
    }
}
