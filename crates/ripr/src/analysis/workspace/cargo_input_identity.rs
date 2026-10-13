//! Raw configuration identity for the Cargo metadata harness premise.
//!
//! Keep this independent of Anyhow's stricter eligibility policy: valid Cargo
//! configuration outside that policy and malformed configuration must differ.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_CONFIG_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_CONFIG_BYTES: usize = 16 * MAX_CONFIG_BYTES;

pub(crate) fn cargo_input_identity(root: &Path) -> Option<String> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let cargo_home = match std::env::var_os("CARGO_HOME") {
        Some(value) if value.is_empty() => return None,
        Some(value) => PathBuf::from(value),
        None => std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .filter(|value| !value.is_empty())
            .map(|home| PathBuf::from(home).join(".cargo"))?,
    };
    let cargo_home = {
        let home = cargo_home;
        if home.is_absolute() {
            home
        } else {
            root.join(home)
        }
    };
    let mut environment = std::env::vars_os()
        .filter_map(|(name, value)| {
            selected_environment_name(name.as_encoded_bytes(), cfg!(windows))
                .map(|name| (name, value.as_encoded_bytes().to_vec()))
        })
        .collect::<Vec<_>>();
    environment.sort();
    let mut identity = Vec::new();
    append(&mut identity, b"cargo-metadata-configuration-v1");
    for (name, value) in environment {
        append(&mut identity, &name);
        append(&mut identity, &value);
    }
    let paths = configuration_paths(&root, Some(&cargo_home));
    append_configuration_files(&mut identity, paths)?;
    Some(crate::config::bytes_fingerprint(&identity))
}

/// Windows preserves spelling while resolving environment names without case.
/// Normalize selected names there; Unix retains its case-sensitive contract.
fn selected_environment_name(name: &[u8], case_insensitive: bool) -> Option<Vec<u8>> {
    let name = if case_insensitive {
        name.to_ascii_uppercase()
    } else {
        name.to_vec()
    };
    (name.starts_with(b"CARGO_")
        || matches!(
            name.as_slice(),
            b"RUSTUP_HOME" | b"RUSTUP_TOOLCHAIN" | b"PATH"
        ))
    .then_some(name)
}

fn configuration_paths(root: &Path, cargo_home: Option<&Path>) -> BTreeSet<PathBuf> {
    let mut paths = BTreeSet::new();
    for directory in root.ancestors() {
        for name in [
            ".cargo/config",
            ".cargo/config.toml",
            "rust-toolchain",
            "rust-toolchain.toml",
        ] {
            paths.insert(directory.join(name));
        }
    }
    if let Some(home) = cargo_home {
        paths.extend([home.join("config"), home.join("config.toml")]);
    }
    paths
}

fn append(identity: &mut Vec<u8>, bytes: &[u8]) {
    identity.extend_from_slice(&bytes.len().to_le_bytes());
    identity.extend_from_slice(bytes);
}

fn append_configuration_files(identity: &mut Vec<u8>, paths: BTreeSet<PathBuf>) -> Option<()> {
    let mut total = 0usize;
    for path in paths {
        super::super::cancellation::checkpoint().ok()?;
        append(identity, path.as_os_str().as_encoded_bytes());
        // Metadata handles regular files and symlinks to regular files. Refuse
        // device/FIFO/directory inputs before opening, so key acquisition does
        // not block outside the Cargo subprocess deadline.
        match std::fs::metadata(&path) {
            Ok(metadata) if !metadata.is_file() => return None,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                append(identity, b"absent");
                continue;
            }
            Err(_) => return None,
        }
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(_) => return None,
        };
        append(identity, b"present");
        append(
            identity,
            path.canonicalize().ok()?.as_os_str().as_encoded_bytes(),
        );
        let mut bytes = Vec::new();
        file.take((MAX_CONFIG_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > MAX_CONFIG_BYTES {
            return None;
        }
        total = total.checked_add(bytes.len())?;
        if total > MAX_TOTAL_CONFIG_BYTES {
            return None;
        }
        append(identity, &bytes);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_environment_names_preserve_platform_semantics_and_raw_values() {
        for name in [b"Path".as_slice(), b"path", b"PATH"] {
            assert_eq!(
                selected_environment_name(name, true),
                Some(b"PATH".to_vec())
            );
        }
        assert_eq!(
            selected_environment_name(b"cargo_build_target", true),
            Some(b"CARGO_BUILD_TARGET".to_vec())
        );
        assert_eq!(
            selected_environment_name(b"rustup_toolchain", true),
            Some(b"RUSTUP_TOOLCHAIN".to_vec())
        );
        for name in [
            b"Path".as_slice(),
            b"path",
            b"cargo_build_target",
            b"rustup_toolchain",
        ] {
            assert_eq!(selected_environment_name(name, false), None);
        }
        assert_eq!(
            selected_environment_name(b"CARGO_BUILD_TARGET", false),
            Some(b"CARGO_BUILD_TARGET".to_vec())
        );
        assert_eq!(selected_environment_name(b"UNRELATED", true), None);
        let identity = |name: &[u8], value: &[u8]| {
            let mut result = Vec::new();
            if let Some(name) = selected_environment_name(name, true) {
                append(&mut result, &name);
                append(&mut result, value);
            }
            result
        };
        assert_eq!(
            identity(b"Path", b"tool-bin"),
            identity(b"PATH", b"tool-bin")
        );
        assert_ne!(
            identity(b"Path", b"tool-bin"),
            identity(b"Path", b"missing-bin")
        );
        assert_ne!(identity(b"Path", b"\xff"), identity(b"Path", b"\xfe"));
    }

    #[test]
    fn cargo_input_identity_covers_ancestor_home_legacy_and_absence() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-cargo-inputs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        let member = root.join("member");
        let home = root.join("cargo-home");
        std::fs::create_dir_all(&member).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(root.join(".cargo")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(&home).map_err(|error| error.to_string())?;
        let paths = configuration_paths(&member, Some(&home));
        let snapshot = || {
            let mut identity = Vec::new();
            append_configuration_files(&mut identity, paths.clone())
                .ok_or("configuration identity unavailable")?;
            Ok::<_, String>(identity)
        };
        let absent = snapshot()?;
        for file in [
            root.join(".cargo/config"),
            root.join(".cargo/config.toml"),
            home.join("config"),
            home.join("config.toml"),
            root.join("rust-toolchain.toml"),
            root.join("rust-toolchain"),
        ] {
            std::fs::write(&file, "[build]\njobs=1\n").map_err(|error| error.to_string())?;
            let valid = snapshot()?;
            assert_ne!(valid, absent, "creation participates: {}", file.display());
            std::fs::write(&file, "[build\n").map_err(|error| error.to_string())?;
            assert_ne!(
                snapshot()?,
                valid,
                "malformed and valid bytes remain distinct"
            );
            std::fs::remove_file(&file).map_err(|error| error.to_string())?;
            assert_eq!(snapshot()?, absent, "deletion restores absence");
        }
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())
    }

    #[test]
    fn cargo_input_identity_refuses_unreadable_and_oversized_configuration() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-cargo-input-refusal-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let path = root.join("config.toml");
        let paths = BTreeSet::from([path.clone()]);
        std::fs::write(&path, vec![b' '; MAX_CONFIG_BYTES + 1])
            .map_err(|error| error.to_string())?;
        assert!(append_configuration_files(&mut Vec::new(), paths.clone()).is_none());
        std::fs::remove_file(&path).map_err(|error| error.to_string())?;
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        assert!(
            append_configuration_files(&mut Vec::new(), paths).is_none(),
            "a directory cannot be treated as absent configuration"
        );
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())
    }

    #[test]
    fn unavailable_cargo_inputs_refuse_current_key_without_claiming_corpus_provenance()
    -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-cargo-current-key-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(root.join(".cargo/config.toml"))
            .map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn shipping(kilos: i32) -> i32 { kilos * 4 }\n",
        )
        .map_err(|error| error.to_string())?;
        let config = crate::config::RiprConfig::default();
        match super::super::super::seam_inventory::workspace_cache_key_at_with_config(
            &root, &config,
        ) {
            Err(reason) => assert!(reason.contains("cargo_configuration_identity_unavailable")),
            Ok(_) => {
                return Err(
                    "unavailable configuration returned a comparable current key".to_string(),
                );
            }
        }
        let report =
            super::super::super::seam_inventory::inventory_classified_seams_report_at_with_config(
                &root, &config,
            )?;
        assert_eq!(
            report.classified.len(),
            1,
            "analysis still returns the actual production subject"
        );
        assert!(
            report.corpus_key.is_none(),
            "unknown configuration cannot stamp comparable corpus provenance"
        );
        // Inventory stores the corpus fingerprint mapping even when classified
        // publication is refused. The warmed current-key route must refuse it
        // just like the initial content-derived route above.
        match super::super::super::seam_inventory::workspace_cache_key_at_with_config(
            &root, &config,
        ) {
            Err(reason) => assert!(reason.contains("cargo_configuration_identity_unavailable")),
            Ok(_) => {
                return Err(
                    "unavailable configuration returned a warmed comparable current key"
                        .to_string(),
                );
            }
        }
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())
    }
}
