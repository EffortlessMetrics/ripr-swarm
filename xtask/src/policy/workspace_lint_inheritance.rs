//! Require Cargo workspace members to opt in to the shared lint policy (#13222).
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize)]
struct WorkspaceMetadata {
    packages: Vec<WorkspacePackage>,
    workspace_members: Vec<String>,
}

#[derive(serde::Deserialize)]
struct WorkspacePackage {
    id: String,
    manifest_path: PathBuf,
}

pub(crate) fn workspace_lint_inheritance_violations() -> Result<Vec<String>, String> {
    // Cargo owns membership, including globs, excludes and implicit path members.
    // Metadata does not compile targets; locked/offline keeps discovery read-only.
    let args = [
        "metadata",
        "--format-version",
        "1",
        "--no-deps",
        "--locked",
        "--offline",
        "--manifest-path",
        "Cargo.toml",
    ]
    .map(str::to_string);
    let context = "check-lint-policy workspace membership";
    let output = crate::run::capture_output_without_timeout("cargo", &args, &[], context)?;
    let status = output
        .status
        .ok_or_else(|| format!("{context}: cargo metadata returned no exit status"))?;
    if !status.success() {
        let detail = [output.stdout.trim(), output.stderr.trim()]
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!(
            "{context}: cargo {} failed with {status}: {detail}",
            args.join(" ")
        ));
    }
    collect_inheritance_violations(&output.stdout, |path| {
        std::fs::read_to_string(path).map_err(|error| error.to_string())
    })
}

fn collect_inheritance_violations(
    metadata_text: &str,
    mut read_manifest: impl FnMut(&Path) -> Result<String, String>,
) -> Result<Vec<String>, String> {
    let metadata: WorkspaceMetadata = serde_json::from_str(metadata_text)
        .map_err(|error| format!("check-lint-policy: invalid Cargo workspace metadata: {error}"))?;
    if metadata.workspace_members.is_empty() {
        return Err("check-lint-policy: Cargo metadata has no workspace members".to_string());
    }

    let mut packages = BTreeMap::new();
    for package in metadata.packages {
        if package.id.trim().is_empty() {
            return Err("check-lint-policy: Cargo metadata has an empty package id".to_string());
        }
        if packages
            .insert(package.id.clone(), package.manifest_path)
            .is_some()
        {
            return Err(format!(
                "check-lint-policy: Cargo metadata repeats package id `{}`",
                package.id
            ));
        }
    }

    // Resolve the entire membership set before reading any manifest. Incomplete
    // or ambiguous discovery must not look like successful partial coverage.
    let mut members = BTreeSet::new();
    let mut manifests = BTreeSet::new();
    for id in metadata.workspace_members {
        if id.trim().is_empty() {
            return Err(
                "check-lint-policy: Cargo metadata has an empty workspace member id".to_string(),
            );
        }
        if !members.insert(id.clone()) {
            return Err(format!(
                "check-lint-policy: Cargo metadata repeats workspace member id `{id}`"
            ));
        }
        let path = packages.get(&id).ok_or_else(|| {
            format!("check-lint-policy: Cargo workspace member `{id}` has no package metadata")
        })?;
        if path.as_os_str().is_empty() {
            return Err(format!(
                "check-lint-policy: Cargo workspace member `{id}` has an empty manifest path"
            ));
        }
        if !manifests.insert(path.clone()) {
            return Err(format!(
                "check-lint-policy: Cargo workspace members repeat manifest `{}`",
                path.display()
            ));
        }
    }

    let mut violations = Vec::new();
    for path in manifests {
        let result = read_manifest(&path)
            .map_err(|error| format!("failed to read member manifest: {error}"))
            .and_then(|text| validate_inheritance(&text));
        if let Err(error) = result {
            violations.push(format!("{}: {error}", path.display()));
        }
    }
    Ok(violations)
}

fn validate_inheritance(manifest: &str) -> Result<(), String> {
    let manifest: toml::Table = toml::from_str(manifest)
        .map_err(|error| format!("invalid member Cargo.toml: {error}"))?;
    let lints = manifest.get("lints").and_then(toml::Value::as_table);
    if lints.is_some_and(|lints| {
        lints.len() == 1 && lints.get("workspace").and_then(toml::Value::as_bool) == Some(true)
    }) {
        Ok(())
    } else {
        Err(
            "workspace member must inherit the shared lint policy with `[lints] workspace = true`; member-local lint tables cannot replace or override workspace lints"
                .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{collect_inheritance_violations, validate_inheritance};
    use std::path::Path;

    fn metadata(packages: &[(&str, &str)], members: &[&str]) -> String {
        serde_json::json!({
            "packages": packages.iter().map(|(id, path)| serde_json::json!({
                "id": id,
                "manifest_path": path,
            })).collect::<Vec<_>>(),
            "workspace_members": members,
            "workspace_default_members": members.iter().take(1).collect::<Vec<_>>(),
        })
        .to_string()
    }

    #[test]
    fn accepts_current_members_and_detects_each_missing_opt_in() -> Result<(), String> {
        let manifests = [
            (
                "crates/ripr/Cargo.toml",
                include_str!("../../../crates/ripr/Cargo.toml"),
            ),
            (
                "crates/ripr-contract/Cargo.toml",
                include_str!("../../../crates/ripr-contract/Cargo.toml"),
            ),
            ("xtask/Cargo.toml", include_str!("../../Cargo.toml")),
            (
                "tools/repo-policy/Cargo.toml",
                include_str!("../../../tools/repo-policy/Cargo.toml"),
            ),
        ];
        let packages = manifests
            .iter()
            .map(|(path, _)| (*path, *path))
            .collect::<Vec<_>>();
        let members = manifests.iter().map(|(path, _)| *path).collect::<Vec<_>>();
        let metadata = metadata(&packages, &members);
        let read = |path: &Path| {
            manifests
                .iter()
                .find(|(candidate, _)| path == Path::new(candidate))
                .map(|(_, text)| (*text).to_string())
                .ok_or_else(|| format!("unexpected fixture path {}", path.display()))
        };
        assert!(collect_inheritance_violations(&metadata, read)?.is_empty());

        for (changed, _) in manifests {
            let violations = collect_inheritance_violations(&metadata, |path| {
                let text = read(path)?;
                if path == Path::new(changed) {
                    Ok(text.replace("[lints]\nworkspace = true", ""))
                } else {
                    Ok(text)
                }
            })?;
            assert_eq!(violations.len(), 1, "{changed}: {violations:?}");
            assert!(
                violations
                    .iter()
                    .any(|violation| violation.starts_with(changed))
            );
        }
        Ok(())
    }

    #[test]
    fn checks_nondefault_and_additional_members_but_not_nonmembers() -> Result<(), String> {
        let metadata = metadata(
            &[
                ("default", "default/Cargo.toml"),
                ("additional", "new/member/Cargo.toml"),
                ("dependency", "external/Cargo.toml"),
            ],
            &["default", "additional"],
        );
        let mut reads = Vec::new();
        let violations = collect_inheritance_violations(&metadata, |path| {
            reads.push(path.to_path_buf());
            if path == Path::new("default/Cargo.toml") {
                Ok("[lints]\nworkspace = true\n".to_string())
            } else {
                Ok("[package]\nname = 'new'\n".to_string())
            }
        })?;
        assert_eq!(reads.len(), 2);
        assert!(
            !reads
                .iter()
                .any(|path| path == Path::new("external/Cargo.toml"))
        );
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(
            violations
                .iter()
                .any(|error| error.starts_with("new/member/Cargo.toml:"))
        );
        Ok(())
    }

    #[test]
    fn accepts_toml_opt_in_forms_and_rejects_missing_disabled_or_replaced_lints()
    -> Result<(), String> {
        for manifest in [
            "[lints]\nworkspace = true\n",
            "lints = { workspace = true }\n",
            "lints.workspace = true\n",
        ] {
            validate_inheritance(manifest).map_err(|error| format!("{manifest}: {error}"))?;
        }
        for manifest in [
            "[package]\nname = 'member'\n",
            "[lints]\nworkspace = false\n",
            "[lints]\nworkspace = 'true'\n",
            "[lints.clippy]\npanic = 'allow'\n",
            "[lints]\nworkspace = true\n[lints.clippy]\npanic = 'allow'\n",
            "[package.metadata.lints]\nworkspace = true\n",
            "lints = true\n",
        ] {
            let error = validate_inheritance(manifest)
                .err()
                .ok_or_else(|| format!("accepted missing or replaced inheritance: {manifest}"))?;
            assert!(error.contains("must inherit the shared lint policy"), "{error}");
        }
        Ok(())
    }

    #[test]
    fn malformed_or_unreadable_members_fail_with_manifest_paths() -> Result<(), String> {
        let metadata = metadata(
            &[
                ("broken", "broken/Cargo.toml"),
                ("unreadable", "unreadable/Cargo.toml"),
            ],
            &["broken", "unreadable"],
        );
        let violations = collect_inheritance_violations(&metadata, |path| {
            if path == Path::new("broken/Cargo.toml") {
                Ok("[lints]\nworkspace = true\nworkspace = false\n".to_string())
            } else {
                Err("fixture read denied".to_string())
            }
        })?;
        assert_eq!(violations.len(), 2, "{violations:?}");
        assert!(violations.iter().any(|error| {
            error.starts_with("broken/Cargo.toml:") && error.contains("invalid member Cargo.toml")
        }));
        assert!(violations.iter().any(|error| {
            error.starts_with("unreadable/Cargo.toml:") && error.contains("fixture read denied")
        }));
        Ok(())
    }

    #[test]
    fn invalid_or_ambiguous_membership_fails_before_reading_manifests() -> Result<(), String> {
        for metadata in [
            "not JSON".to_string(),
            "{}".to_string(),
            metadata(&[], &[]),
            metadata(&[], &["missing"]),
            metadata(&[("", "Cargo.toml")], &[""]),
            metadata(&[("a", "a/Cargo.toml")], &[" "]),
            metadata(&[("a", "a/Cargo.toml"), ("a", "other/Cargo.toml")], &["a"]),
            metadata(&[("a", "a/Cargo.toml")], &["a", "a"]),
            metadata(&[("a", "")], &["a"]),
            metadata(
                &[("a", "same/Cargo.toml"), ("b", "same/Cargo.toml")],
                &["a", "b"],
            ),
        ] {
            let mut reads = 0;
            let error = collect_inheritance_violations(&metadata, |_| {
                reads += 1;
                Ok("[lints]\nworkspace = true\n".to_string())
            })
            .err()
            .ok_or_else(|| format!("accepted invalid or ambiguous membership: {metadata}"))?;
            assert!(error.starts_with("check-lint-policy:"), "{error}");
            assert_eq!(reads, 0, "{metadata}");
        }
        Ok(())
    }
}
