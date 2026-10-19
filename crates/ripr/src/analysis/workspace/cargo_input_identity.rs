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
    cargo_input_identity_with_env(root, &home::env::OS_ENV)
}

fn cargo_input_identity_with_env(root: &Path, env: &dyn home::env::Env) -> Option<String> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    // Empty environment values do not mean that Cargo has no home config:
    // its canonical resolver falls back through OS home discovery.
    let cargo_home = home::env::cargo_home_with_cwd_env(env, &root).ok()?;
    let cargo_home = if cargo_home.is_absolute() {
        cargo_home
    } else {
        // A relative HOME/USERPROFILE fallback is interpreted by Cargo at the
        // metadata command's cwd, which can differ from the RIPR caller's cwd.
        root.join(cargo_home)
    };
    // Config-relative paths are based on the home's parent. Preserve that
    // authority before resolving the home itself (which may be a symlink).
    let cargo_home_parent = cargo_home.parent().unwrap_or(&cargo_home).to_path_buf();
    let cargo_home_parent = cargo_home_parent
        .canonicalize()
        .unwrap_or(cargo_home_parent);
    let cargo_home = cargo_home.canonicalize().unwrap_or(cargo_home);
    let mut environment = std::env::vars_os()
        .filter_map(|(name, value)| {
            selected_environment_name(name.as_encoded_bytes(), cfg!(windows))
                .map(|name| (name, value.as_encoded_bytes().to_vec()))
        })
        .collect::<Vec<_>>();
    environment.sort();
    let mut identity = Vec::new();
    append(&mut identity, b"cargo-metadata-configuration-v1");
    append(
        &mut identity,
        cargo_home_parent.as_os_str().as_encoded_bytes(),
    );
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

    #[derive(Clone)]
    struct HomeEnvironment {
        cargo_home: Option<std::ffi::OsString>,
        home: Option<PathBuf>,
        caller_cwd: PathBuf,
    }

    impl home::env::Env for HomeEnvironment {
        fn home_dir(&self) -> Option<PathBuf> {
            self.home.clone()
        }

        fn current_dir(&self) -> std::io::Result<PathBuf> {
            Ok(self.caller_cwd.clone())
        }

        fn var_os(&self, key: &str) -> Option<std::ffi::OsString> {
            (key == "CARGO_HOME")
                .then(|| self.cargo_home.clone())
                .flatten()
        }
    }

    #[test]
    fn cargo_home_fallback_and_relative_paths_bind_configuration_or_refuse_discovery()
    -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-cargo-home-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        let fallback = root.join("owned-home");
        std::fs::create_dir_all(fallback.join(".cargo")).map_err(|error| error.to_string())?;
        let env = HomeEnvironment {
            cargo_home: None,
            home: Some(fallback.clone()),
            caller_cwd: root.parent().ok_or("fixture parent")?.to_path_buf(),
        };
        let snapshot = |env: &HomeEnvironment| {
            cargo_input_identity_with_env(&root, env).ok_or("discoverable Cargo home identity")
        };
        for cargo_home in [None, Some(std::ffi::OsString::new())] {
            let env = HomeEnvironment {
                cargo_home,
                ..env.clone()
            };
            let absent = snapshot(&env)?;
            let path = fallback.join(".cargo/config.toml");
            std::fs::write(&path, "[build]\njobs=1\n").map_err(|error| error.to_string())?;
            let valid = snapshot(&env)?;
            assert_ne!(valid, absent, "OS fallback configuration participates");
            std::fs::write(&path, "[build\n").map_err(|error| error.to_string())?;
            assert_ne!(
                snapshot(&env)?,
                valid,
                "fallback malformed bytes participate"
            );
            std::fs::write(&path, "[build]\njobs=1\n").map_err(|error| error.to_string())?;
            assert_eq!(snapshot(&env)?, valid, "fallback revert restores identity");
            std::fs::remove_file(path).map_err(|error| error.to_string())?;
        }
        let relative_home = root.join("owned-relative-home/.cargo");
        std::fs::create_dir_all(&relative_home).map_err(|error| error.to_string())?;
        let relative_env = HomeEnvironment {
            home: Some(PathBuf::from("owned-relative-home")),
            ..env.clone()
        };
        let absent = snapshot(&relative_env)?;
        std::fs::write(relative_home.join("config.toml"), "[build]\njobs=1\n")
            .map_err(|error| error.to_string())?;
        assert_ne!(
            snapshot(&relative_env)?,
            absent,
            "relative fallback configuration must resolve at the analysis root"
        );
        let undiscoverable = HomeEnvironment {
            home: None,
            ..env.clone()
        };
        assert!(
            cargo_input_identity_with_env(&root, &undiscoverable).is_none(),
            "undiscoverable home cannot become a reusable no-home marker"
        );
        std::fs::create_dir_all(root.join("explicit-home")).map_err(|error| error.to_string())?;
        let mut explicit_identities = Vec::new();
        for cargo_home in [PathBuf::from("explicit-home"), root.join("explicit-home")] {
            let explicit = HomeEnvironment {
                cargo_home: Some(cargo_home.into_os_string()),
                ..undiscoverable.clone()
            };
            let before = snapshot(&explicit)?;
            let path = root.join("explicit-home/config.toml");
            std::fs::write(&path, "[build]\njobs=1\n").map_err(|error| error.to_string())?;
            let configured = snapshot(&explicit)?;
            assert_ne!(
                configured, before,
                "explicit home configuration participates without OS home"
            );
            explicit_identities.push(configured);
            std::fs::remove_file(path).map_err(|error| error.to_string())?;
        }
        assert_eq!(
            explicit_identities[0], explicit_identities[1],
            "relative and absolute explicit Cargo homes have the same authority"
        );
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())
    }

    fn home_authority_fixture_root(stamp: u128) -> Result<PathBuf, String> {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let root = parent.join(format!(
            "ripr-home-authority-{}-{stamp}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        // Parallel controls can observe the same timestamp. Claim each root
        // exclusively before creating or editing its configuration files.
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        Ok(root)
    }

    #[test]
    fn cargo_home_authority_fixtures_stay_isolated_at_the_same_timestamp() -> Result<(), String> {
        let first = home_authority_fixture_root(0)?;
        let second = home_authority_fixture_root(0)?;
        assert_ne!(first, second, "live fixtures need distinct roots");
        std::fs::write(first.join("config.toml"), "[build]\njobs=1\n")
            .map_err(|error| error.to_string())?;
        std::fs::write(second.join("config.toml"), "[build]\njobs=2\n")
            .map_err(|error| error.to_string())?;
        std::fs::write(first.join("config.toml"), "[build]\njobs=3\n")
            .map_err(|error| error.to_string())?;
        let read_second = || {
            std::fs::read_to_string(second.join("config.toml")).map_err(|error| error.to_string())
        };
        assert_eq!(read_second()?, "[build]\njobs=2\n");
        std::fs::remove_dir_all(first).map_err(|error| error.to_string())?;
        assert_eq!(read_second()?, "[build]\njobs=2\n");
        std::fs::remove_dir_all(second).map_err(|error| error.to_string())
    }

    fn check_home_directory_authority(
        aliases: impl FnOnce(&Path) -> Result<Vec<PathBuf>, String>,
    ) -> Result<(), String> {
        let root = home_authority_fixture_root(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos(),
        )?;
        for directory in ["home", "other-home", "anchor"] {
            std::fs::create_dir_all(root.join(directory)).map_err(|error| error.to_string())?;
        }
        let config = root.join("home/config.toml");
        let bytes = "[build]\njobs=1\n";
        std::fs::write(&config, bytes).map_err(|error| error.to_string())?;
        std::fs::write(root.join("other-home/config.toml"), bytes)
            .map_err(|error| error.to_string())?;
        // Mock only discovery. Raw command-environment bytes remain fixed and
        // still participate in the complete identity.
        let snapshot = |home: PathBuf| {
            cargo_input_identity_with_env(
                &root,
                &HomeEnvironment {
                    cargo_home: Some(home.into_os_string()),
                    home: None,
                    caller_cwd: root.join("anchor"),
                },
            )
            .ok_or("existing home identity".to_string())
        };
        let expected = snapshot(root.join("home"))?;
        assert_ne!(
            expected,
            snapshot(root.join("other-home"))?,
            "distinct homes with identical bytes remain distinct"
        );
        for alias in aliases(&root)? {
            assert_eq!(
                expected,
                snapshot(alias.clone())?,
                "equivalent home directory: {alias:?}"
            );
            std::fs::write(&config, "[build]\njobs=2\n").map_err(|error| error.to_string())?;
            assert_ne!(
                expected,
                snapshot(alias.clone())?,
                "configuration edits invalidate identity"
            );
            std::fs::write(&config, bytes).map_err(|error| error.to_string())?;
            assert_eq!(
                expected,
                snapshot(alias)?,
                "configuration revert restores identity"
            );
        }
        std::fs::remove_dir_all(root).map_err(|error| error.to_string())
    }

    #[test]
    fn cargo_home_parent_alias_preserves_directory_authority() -> Result<(), String> {
        check_home_directory_authority(|root| Ok(vec![root.join("anchor/../home")]))
    }

    #[cfg(windows)]
    #[test]
    fn cargo_home_windows_relative_absolute_and_verbatim_preserve_directory_authority()
    -> Result<(), String> {
        check_home_directory_authority(|root| {
            let verbatim = root
                .join("home")
                .canonicalize()
                .map_err(|error| error.to_string())?;
            let mut components = verbatim.components();
            let Some(std::path::Component::Prefix(prefix)) = components.next() else {
                return Err("Windows fixture needs a rooted directory".to_string());
            };
            let mut absolute = match prefix.kind() {
                std::path::Prefix::VerbatimDisk(drive) => {
                    PathBuf::from(format!("{}:", char::from(drive)))
                }
                std::path::Prefix::VerbatimUNC(server, share) => {
                    PathBuf::from(r"\\").join(server).join(share)
                }
                _ => return Err("Windows fixture needs a verbatim drive or UNC root".to_string()),
            };
            for component in components {
                absolute.push(component.as_os_str());
            }
            assert_eq!(
                absolute.canonicalize().map_err(|error| error.to_string())?,
                verbatim,
                "fixture spellings must resolve to the same directory"
            );
            assert_ne!(
                absolute.as_os_str(),
                verbatim.as_os_str(),
                "fixture must exercise distinct Windows path spellings"
            );
            Ok(vec![PathBuf::from("home"), absolute, verbatim])
        })
    }

    #[cfg(unix)]
    #[test]
    fn cargo_home_unix_literal_backslashes_remain_distinct() -> Result<(), String> {
        check_home_directory_authority(|root| {
            let literal_name = Path::new(r"\\?\home");
            assert!(
                literal_name.is_relative(),
                "Unix backslashes are not path separators"
            );
            let literal = root.join(literal_name);
            std::fs::create_dir_all(&literal).map_err(|error| error.to_string())?;
            let config = literal.join("config.toml");
            std::fs::write(&config, "[build]\njobs=1\n").map_err(|error| error.to_string())?;
            let env = HomeEnvironment {
                cargo_home: Some(literal.into_os_string()),
                home: None,
                caller_cwd: root.to_path_buf(),
            };
            let ordinary = HomeEnvironment {
                cargo_home: Some(root.join("home").into_os_string()),
                ..env.clone()
            };
            let literal =
                cargo_input_identity_with_env(root, &env).ok_or("literal home identity")?;
            let ordinary =
                cargo_input_identity_with_env(root, &ordinary).ok_or("ordinary home identity")?;
            assert_ne!(
                literal, ordinary,
                "Unix backslashes are literal filename bytes"
            );
            std::fs::write(&config, "[build]\njobs=2\n").map_err(|error| error.to_string())?;
            let edited =
                cargo_input_identity_with_env(root, &env).ok_or("edited literal-home identity")?;
            assert_ne!(
                literal, edited,
                "literal-home configuration edits invalidate identity"
            );
            std::fs::write(&config, "[build]\njobs=1\n").map_err(|error| error.to_string())?;
            let reverted = cargo_input_identity_with_env(root, &env)
                .ok_or("reverted literal-home identity")?;
            assert_eq!(
                literal, reverted,
                "literal-home configuration revert restores identity"
            );
            Ok(vec![PathBuf::from("home")])
        })
    }

    #[cfg(unix)]
    #[test]
    fn cargo_home_symlink_keeps_configuration_parent_authority() -> Result<(), String> {
        check_home_directory_authority(|root| {
            let linked = root.join("anchor/linked-home");
            std::os::unix::fs::symlink(root.join("home"), &linked)
                .map_err(|error| error.to_string())?;
            let snapshot = |home: PathBuf| {
                cargo_input_identity_with_env(
                    root,
                    &HomeEnvironment {
                        cargo_home: Some(home.into_os_string()),
                        home: None,
                        caller_cwd: root.to_path_buf(),
                    },
                )
                .ok_or("symlink home identity")
            };
            assert_ne!(
                snapshot(root.join("home"))?,
                snapshot(linked)?,
                "same files through different config-relative parents remain distinct"
            );
            Ok(vec![PathBuf::from("home")])
        })
    }

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
