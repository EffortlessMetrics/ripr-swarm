//! A Cargo configuration found in or above the analyzed tree must not choose
//! a program, flag or environment for the resolving `cargo metadata` probe.

use super::*;

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Result<Self, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-discovered-config-{label}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        // Never take ownership of a pre-existing directory.
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let root = root.canonicalize().map_err(|error| error.to_string())?;
        Ok(Self { root })
    }

    fn write(&self, path: &str, contents: &str) -> Result<(), String> {
        let path = self.root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(path, contents).map_err(|error| error.to_string())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct OwnedHome(PathBuf);

impl home::env::Env for OwnedHome {
    fn home_dir(&self) -> Option<PathBuf> {
        None
    }

    fn current_dir(&self) -> std::io::Result<PathBuf> {
        Err(std::io::Error::other("snapshot must use the metadata cwd"))
    }

    fn var_os(&self, key: &str) -> Option<std::ffi::OsString> {
        (key == "CARGO_HOME").then(|| self.0.clone().into_os_string())
    }
}

const EXECUTING: [&str; 10] = [
    "[build]\nrustc = 'tools/rustc'",
    "[build]\nrustc-wrapper = 'tools/wrap'",
    "[build]\nrustc-workspace-wrapper = 'tools/wrap'",
    "[build]\nrustflags = ['-Zcodegen-backend=tools/backend.so']",
    "[target.x86_64-unknown-linux-gnu]\nrustflags = ['-Zcodegen-backend=x']",
    "[target.'cfg(unix)']\nrustflags = ['-Zcodegen-backend=x']",
    "[env]\nLD_PRELOAD = { value = 'tools/p.so', relative = true }",
    "[env]\nRUSTC_BOOTSTRAP = '1'",
    "include = ['other.toml']",
    "[unstable]\nhost-config = true\n[host]\nrustflags = ['x']",
];

#[test]
fn discovered_config_refuses_every_probe_program_choice() -> Result<(), String> {
    for source in EXECUTING {
        let value: toml::Value = toml::from_str(source).map_err(|e| e.to_string())?;
        assert!(!discovered_config_allowed(&value), "{source}");
    }
    // The env allowlist is per key, not a prefix: an extra key refuses.
    let mixed: toml::Value = toml::from_str("[env]\nTMPDIR = 'target'\nRUSTUP_TOOLCHAIN = '/x'")
        .map_err(|e| e.to_string())?;
    assert!(!discovered_config_allowed(&mixed));
    // ripr's own checked-in configuration (aliases, a linker, temp dirs) and
    // other common keys that cannot pick what the probe runs still qualify.
    let ordinary: toml::Value = toml::from_str(
        "[alias]\nxtask = 'run -p xtask --'\n\
         [build]\njobs = 2\ntarget-dir = 'target'\n\
         [target.x86_64-pc-windows-msvc]\nlinker = 'rust-lld.exe'\n\
         [env]\nTEMP = { value = 'target', relative = true, force = true }\n\
         TMP = 'target'\ntmpdir = 'target'\nRUST_BACKTRACE = '1'\n",
    )
    .map_err(|e| e.to_string())?;
    assert!(discovered_config_allowed(&ordinary));
    Ok(())
}

#[test]
fn snapshot_refuses_a_discovered_config_but_keeps_the_users_cargo_home() -> Result<(), String> {
    let fixture = Fixture::new("snapshot")?;
    let home = fixture.root.join("home");
    std::fs::create_dir(&home).map_err(|e| e.to_string())?;
    let repo = fixture.root.join("repo");
    let nested = repo.join("crates/a");
    std::fs::create_dir_all(&nested).map_err(|e| e.to_string())?;
    let home_env = OwnedHome(home.clone());
    let env = BTreeMap::new();
    let read = |root: &Path| snapshot_with_environment(root, &env, &home_env);
    assert!(read(&repo).is_some(), "an unconfigured tree qualifies");
    for source in EXECUTING {
        for config in [".cargo/config.toml", ".cargo/config"] {
            fixture.write(&format!("repo/{config}"), source)?;
            assert!(read(&repo).is_none(), "{config}: {source}");
            // An analysis root below the configured directory is covered too.
            assert!(read(&nested).is_none(), "nested {config}: {source}");
            std::fs::remove_file(repo.join(config)).map_err(|e| e.to_string())?;
        }
        // The same keys in the user's own Cargo home are the user's choice
        // (an sccache wrapper, for example) and keep the identity.
        std::fs::write(home.join("config.toml"), source).map_err(|e| e.to_string())?;
        assert!(read(&repo).is_some(), "home: {source}");
        std::fs::remove_file(home.join("config.toml")).map_err(|e| e.to_string())?;
    }
    assert!(
        read(&repo).is_some(),
        "removing the stimulus restores identity"
    );
    Ok(())
}

/// The production path: a tree naming its own `rustc-wrapper` must not get
/// that program run by the assertion-dependency identity probe. The control
/// runs the same fixed-argv probe directly and proves the stimulus is live.
#[cfg(unix)]
#[test]
fn identity_probe_never_runs_a_wrapper_the_analyzed_tree_names() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new("wrapper")?;
    let marker = fixture.root.join("wrapper-ran");
    let wrapper = fixture.root.join("tools/wrap");
    fixture.write(
        "tools/wrap",
        &format!("#!/bin/sh\necho ran >> '{}'\nexit 1\n", marker.display()),
    )?;
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    fixture.write(
        "Cargo.toml",
        "[package]\nname = 'ripr-discovered-config'\nversion = '0.0.0'\nedition = '2024'\n\
         [dependencies]\nserde_json = '1'\n[workspace]\n",
    )?;
    fixture.write("src/lib.rs", "pub fn observed() -> bool { true }\n")?;
    fixture.write(
        ".cargo/config.toml",
        &format!("[build]\nrustc-wrapper = '{}'\n", wrapper.display()),
    )?;
    assert!(
        eligible_json_root(&fixture.root),
        "fixture must reach the probe premise"
    );

    assert!(verified_json_identity(&fixture.root).is_none());
    assert!(
        !marker.exists(),
        "the identity probe ran a program the analyzed tree named"
    );

    // Control: the probe's own fixed command, with no snapshot admission,
    // does run the wrapper, so the refusal above is the snapshot and not a
    // dead stimulus. A caller's RUSTC/RUSTC_WRAPPER (an exported sccache, for
    // example) outranks the config key in Cargo, so only the control drops
    // those; the production path above keeps the caller's environment.
    let output =
        std::fs::File::create(fixture.root.join("control.json")).map_err(|e| e.to_string())?;
    let mut control = metadata_command(&fixture.root, output);
    for key in [
        "RUSTC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "CARGO_BUILD_RUSTC",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
    ] {
        control.env_remove(key);
    }
    let status = control.status().map_err(|e| e.to_string())?;
    assert!(!status.success(), "the failing wrapper must fail the probe");
    assert!(marker.exists(), "control: cargo metadata runs the wrapper");
    Ok(())
}
