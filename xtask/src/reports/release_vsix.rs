//! Exact, no-clobber VSIX attachment to an already existing release.
//! This transport does not create a release or authorize publication.
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::release_server::{required_release_arg, sha256_file};

pub(crate) fn release_upload_vsix(args: &[String]) -> Result<(), String> {
    let tag = required_release_arg(args, "tag", "RIPR_RELEASE_TAG")?;
    let vsix = single_vsix(Path::new("dist"))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("could not name VSIX verification directory: {error}"))?
        .as_nanos();
    let verification = std::env::temp_dir().join(format!(
        "ripr-vsix-verification-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&verification)
        .map_err(|error| format!("could not create VSIX verification directory: {error}"))?;
    attach_vsix(&tag, &vsix, &verification, &mut |arguments| {
        crate::run::run_output_owned_with_timeout(
            "gh",
            arguments,
            Duration::from_mins(2),
            "exact VSIX attachment",
        )
    })
}

fn single_vsix(directory: &Path) -> Result<PathBuf, String> {
    let mut selected = None;
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("vsix") {
            continue;
        }
        if selected.is_some() {
            return Err("expected exactly one staged VSIX; found multiple".to_string());
        }
        regular_file(&path)?;
        selected = Some(path);
    }
    selected.ok_or_else(|| "expected exactly one staged VSIX; found none".to_string())
}

fn regular_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() {
        return Err(format!("VSIX is not a regular file: {}", path.display()));
    }
    Ok(())
}

fn attach_vsix(
    tag: &str,
    vsix: &Path,
    verification: &Path,
    gh: &mut impl FnMut(&[String]) -> Result<String, String>,
) -> Result<(), String> {
    if tag.is_empty() || tag.starts_with('-') || tag.chars().any(char::is_control) {
        return Err("release tag is empty or invalid".to_string());
    }
    regular_file(vsix)?;
    let name = vsix
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "VSIX filename is not UTF-8".to_string())?;
    // gh download --pattern is a glob and upload accepts a #label suffix.
    // A literal basename avoids interpreting either syntax as another asset.
    if name.starts_with('-')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(
            "VSIX filename must contain only ASCII letters, digits, dot, dash or underscore"
                .to_string(),
        );
    }
    let expected = sha256_file(vsix)?;
    let view = gh(&[
        "release".into(),
        "view".into(),
        tag.into(),
        "--json".into(),
        "assets".into(),
    ])?;
    let release: serde_json::Value = serde_json::from_str(&view)
        .map_err(|error| format!("invalid release asset inventory: {error}"))?;
    let assets = release
        .get("assets")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "release asset inventory is missing assets".to_string())?;
    let mut matches = 0;
    for asset in assets {
        let asset_name = asset
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "release asset inventory contains an unnamed asset".to_string())?;
        if asset_name == name {
            matches += 1;
        }
    }
    if matches > 1 {
        return Err("release contains duplicate VSIX asset names".to_string());
    }
    if matches == 0 {
        // No overwrite flag: an asset appearing concurrently fails closed.
        gh(&[
            "release".into(),
            "upload".into(),
            tag.into(),
            vsix.to_string_lossy().into_owned(),
        ])?;
    }
    gh(&[
        "release".into(),
        "download".into(),
        tag.into(),
        "--pattern".into(),
        name.into(),
        "--dir".into(),
        verification.to_string_lossy().into_owned(),
    ])?;
    let downloaded = verification.join(name);
    regular_file(&downloaded)?;
    if sha256_file(&downloaded)? != expected {
        return Err("GitHub release VSIX SHA-256 differs from the staged VSIX".to_string());
    }
    println!("Verified exact VSIX attachment; no release creation or replacement performed.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Result<PathBuf, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "vsix-attachment-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        Ok(path)
    }

    type ScenarioResult = (Result<(), String>, Vec<Vec<String>>);

    fn scenario(
        inventory: &str,
        observed: &[u8],
        fail: Option<&str>,
    ) -> Result<ScenarioResult, String> {
        let root = fixture("transport")?;
        let local = root.join("ripr-0.11.0.vsix");
        fs::write(&local, b"exact VSIX").map_err(|error| error.to_string())?;
        let verification = root.join("verified");
        fs::create_dir(&verification).map_err(|error| error.to_string())?;
        let mut calls = Vec::new();
        let result = attach_vsix("v0.11.0", &local, &verification, &mut |args| {
            calls.push(args.to_vec());
            let operation = args.get(1).map(String::as_str);
            if operation == fail {
                return Err("transport refused".into());
            }
            if operation == Some("view") {
                return Ok(inventory.into());
            }
            if operation == Some("download") {
                fs::write(verification.join("ripr-0.11.0.vsix"), observed)
                    .map_err(|error| error.to_string())?;
            }
            Ok(String::new())
        });
        fs::remove_dir_all(root).map_err(|error| error.to_string())?;
        Ok((result, calls))
    }

    fn failure<T>(result: Result<T, String>) -> Result<String, String> {
        match result {
            Err(message) => Ok(message),
            Ok(_) => Err("expected attachment rejection, observed success".to_string()),
        }
    }

    fn operations(calls: &[Vec<String>]) -> Vec<&str> {
        calls
            .iter()
            .filter_map(|args| args.get(1).map(String::as_str))
            .collect()
    }

    /// The publish-extension workflow in this repository attaches the VSIX with
    /// an inline no-clobber release step (the release version is a dispatch
    /// input, and the attach job holds no checkout to build the helper from),
    /// so the prebuilt-helper transport is not a CI-enforced invocation here.
    /// This pins that boundary: the helper stays dispatchable, and the
    /// workflow's attachment never reaches it implicitly or bypasses the
    /// no-replacement rule.
    #[test]
    fn release_vsix_prebuilt_helper_is_cataloged_as_enforced() {
        let workflow = include_str!("../../../.github/workflows/publish-extension.yml");
        assert!(matches!(
            crate::command::XtaskCommand::parse(
                ["release-upload-vsix", "--tag", "v0.11.0"].map(str::to_string)
            ),
            crate::command::XtaskCommand::ReleaseUploadVsix(args)
                if args == ["--tag".to_string(), "v0.11.0".to_string()]
        ));
        assert!(
            !crate::ci_enforced_xtask_invocations(workflow)
                .contains(&("release-upload-vsix".to_string(), String::new())),
            "the helper is not wired into this workflow; update the docs and this test together if it becomes so"
        );
        let attachment = workflow
            .split("  attach-release-asset:")
            .nth(1)
            .and_then(|rest| rest.split("\n  publish-vs-marketplace:").next());
        assert!(attachment.is_some());
        if let Some(attachment) = attachment {
            assert!(attachment.contains("gh release upload \"$release_tag\" \"$vsix\""));
            assert!(!attachment.contains("--clobber"));
        }
    }

    #[test]
    fn release_vsix_workflow_passes_tag_as_data() {
        let workflow = include_str!("../../../.github/workflows/publish-extension.yml");
        let attachment = workflow
            .split("  attach-release-asset:")
            .nth(1)
            .and_then(|rest| rest.split("\n  publish-vs-marketplace:").next());
        assert!(attachment.is_some());
        if let Some(attachment) = attachment {
            // The dispatch input reaches the script through env, never through
            // template expansion inside the script text.
            assert!(attachment.contains("RAW_VERSION: ${{ github.event.inputs.version }}"));
            assert!(attachment.contains("release_tag=\"v${RAW_VERSION#v}\""));
            assert!(!attachment.contains("tag=\"${{"));
            assert!(!attachment.contains("--clobber"));
            assert!(!attachment.contains("gh release create"));
        }
    }

    #[test]
    fn release_vsix_existing_equal_is_verify_only() -> Result<(), String> {
        let (result, calls) = scenario(
            r#"{"assets":[{"name":"ripr-0.11.0.vsix"}]}"#,
            b"exact VSIX",
            None,
        )?;
        result?;
        assert_eq!(operations(&calls), ["view", "download"]);
        Ok(())
    }

    #[test]
    fn release_vsix_absent_uploads_once_then_verifies() -> Result<(), String> {
        let (result, calls) = scenario(r#"{"assets":[]}"#, b"exact VSIX", None)?;
        result?;
        assert_eq!(operations(&calls), ["view", "upload", "download"]);
        assert!(
            calls
                .iter()
                .flatten()
                .all(|arg| arg != "--clobber" && arg != "create")
        );
        Ok(())
    }

    #[test]
    fn release_vsix_mismatch_never_overwrites() -> Result<(), String> {
        for inventory in [
            r#"{"assets":[]}"#,
            r#"{"assets":[{"name":"ripr-0.11.0.vsix"}]}"#,
        ] {
            let (result, calls) = scenario(inventory, b"different VSIX", None)?;
            assert!(!failure(result)?.is_empty());
            assert_eq!(
                operations(&calls)
                    .iter()
                    .filter(|operation| **operation == "upload")
                    .count(),
                usize::from(inventory == r#"{"assets":[]}"#)
            );
        }
        Ok(())
    }

    #[test]
    fn release_vsix_unknown_inventory_does_not_upload() -> Result<(), String> {
        for inventory in [
            "invalid",
            "{}",
            r#"{"assets":[{}]}"#,
            r#"{"assets":[{"name":"ripr-0.11.0.vsix"},{"name":"ripr-0.11.0.vsix"}]}"#,
        ] {
            let (result, calls) = scenario(inventory, b"exact VSIX", None)?;
            assert!(!failure(result)?.is_empty());
            assert_eq!(operations(&calls), ["view"]);
        }
        Ok(())
    }

    #[test]
    fn release_vsix_transport_failures_stop_without_retry() -> Result<(), String> {
        for operation in ["view", "upload", "download"] {
            let (result, calls) = scenario(r#"{"assets":[]}"#, b"exact VSIX", Some(operation))?;
            assert!(!failure(result)?.is_empty());
            assert_eq!(operations(&calls).last().copied(), Some(operation));
        }
        Ok(())
    }

    #[test]
    fn release_vsix_requires_single_staged_file() -> Result<(), String> {
        let root = fixture("inventory")?;
        assert!(!failure(single_vsix(&root))?.is_empty());
        fs::write(root.join("first.vsix"), b"one").map_err(|error| error.to_string())?;
        assert_eq!(single_vsix(&root)?, root.join("first.vsix"));
        fs::write(root.join("second.vsix"), b"two").map_err(|error| error.to_string())?;
        assert!(!failure(single_vsix(&root))?.is_empty());
        fs::remove_dir_all(root).map_err(|error| error.to_string())?;
        Ok(())
    }
}
