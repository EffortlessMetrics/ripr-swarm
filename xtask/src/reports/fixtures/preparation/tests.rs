use super::*;

#[test]
fn readiness_uses_the_locked_fixture_offline() {
    assert_eq!(
        metadata_args(),
        [
            "metadata",
            "--locked",
            "--offline",
            "--format-version",
            "1",
            "--manifest-path",
            "fixtures/owner_return_pin_fallible_literal/input/Cargo.toml"
        ]
    );
}

#[test]
fn missing_dependency_error_names_binding_and_repair() {
    let error = preparation_error(
        "CARGO_HOME=\"/selected cache\" RUSTUP_TOOLCHAIN=\"1.95.0\"",
        "no matching package named anyhow",
    );
    for expected in [
        "/selected cache",
        "1.95.0",
        "no matching package named anyhow",
        "cargo xtask goldens prepare",
        "locked anyhow 1.0.100",
        "Do not bless",
    ] {
        assert!(error.contains(expected), "missing {expected}: {error}");
    }
}

#[test]
fn preparation_rejects_unknown_arguments() -> Result<(), String> {
    let Err(error) = prepare(&["--force".into()]) else {
        return Err("unknown preparation argument was accepted".into());
    };
    assert_eq!(error, "usage: cargo xtask goldens prepare [--offline]");
    Ok(())
}

#[test]
fn repository_toolchain_is_required() -> Result<(), String> {
    assert_eq!(toolchain("[toolchain]\nchannel = '1.95.0'\n")?, "1.95.0");
    let Err(error) = toolchain("[toolchain]\ncomponents = []") else {
        return Err("missing repository toolchain channel was accepted".into());
    };
    assert_eq!(error, "rust-toolchain.toml must name toolchain.channel");
    Ok(())
}

use crate::run::{TimedOutput, capture_output_measured};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct OwnedRoot(PathBuf);

impl OwnedRoot {
    fn new() -> Result<Self, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("ripr-preparation-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        for name in ["cargo", "rustup"] {
            std::fs::create_dir(path.join(name)).map_err(|e| e.to_string())?;
        }
        Ok(Self(path))
    }
}

impl Drop for OwnedRoot {
    fn drop(&mut self) {
        // Only this exclusively created test root; never the user's cache/store.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct RejectingDistribution {
    url: String,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl RejectingDistribution {
    fn new() -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let url = format!(
            "http://{}",
            listener.local_addr().map_err(|e| e.to_string())?
        );
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let count = Arc::clone(&requests);
        let done = Arc::clone(&stop);
        let worker = std::thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let mut request = [0; 4096];
                        if stream.read(&mut request).is_ok_and(|n| n > 0) {
                            count.fetch_add(1, Ordering::SeqCst);
                        }
                        let _ = stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            url,
            requests,
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for RejectingDistribution {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn proxy_directory() -> Result<PathBuf, String> {
    let rustup = format!("rustup{}", std::env::consts::EXE_SUFFIX);
    let path = std::env::var_os("PATH").ok_or("PATH is required for rustup proxy controls")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(&rustup);
        if candidate.is_file() {
            let bytes = std::fs::read(candidate).map_err(|e| e.to_string())?;
            let identity = Sha256::digest(bytes);
            for name in ["cargo", "rustc"] {
                let bytes = std::fs::read(
                    directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)),
                )
                .map_err(|e| format!("rustup proxy control requires sibling {name}: {e}"))?;
                if Sha256::digest(bytes) != identity {
                    return Err(format!("{name} beside rustup is not its proxy"));
                }
            }
            return std::path::absolute(directory).map_err(|e| e.to_string());
        }
    }
    Err("rustup proxies are required for fixture preparation launcher controls".into())
}

fn control_environment(
    root: &OwnedRoot,
    rustup: &Path,
    directory: &Path,
    server: &RejectingDistribution,
) -> Result<Vec<(&'static str, String)>, String> {
    let inherited = std::env::var_os("PATH").ok_or("PATH required for proxy controls")?;
    let search = std::env::join_paths(
        std::iter::once(directory.to_path_buf()).chain(std::env::split_paths(&inherited)),
    )
    .map_err(|e| e.to_string())?
    .into_string()
    .map_err(|_| "control PATH must be UTF-8")?;
    Ok(vec![
        ("PATH", search),
        ("NO_PROXY", "*".into()),
        ("no_proxy", "*".into()),
        (
            "CARGO_HOME",
            root.0.join("cargo").to_string_lossy().into_owned(),
        ),
        ("RUSTUP_HOME", rustup.to_string_lossy().into_owned()),
        ("RUSTUP_TOOLCHAIN", "1.95.0".into()),
        ("RUSTUP_AUTO_INSTALL", "1".into()),
        ("RUSTUP_DIST_SERVER", server.url.clone()),
        ("RUSTUP_UPDATE_ROOT", server.url.clone()),
        ("CARGO_NET_OFFLINE", "true".into()),
        ("CARGO_REGISTRIES_CRATES_IO_PROTOCOL", "sparse".into()),
    ])
}

fn invoke_proxy(
    directory: &Path,
    program: &str,
    args: &[String],
    environment: &[(&'static str, String)],
    offline: bool,
) -> Result<TimedOutput, String> {
    let environment = proxy_environment(environment, offline);
    let envs = environment
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect::<Vec<_>>();
    let executable = directory.join(format!("{program}{}", std::env::consts::EXE_SUFFIX));
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask parent missing")?;
    capture_output_measured(
        &path_text(&executable)?,
        args,
        Some(repo),
        &envs,
        Duration::from_secs(15),
        "fixture preparation proxy control",
    )
    .map(|r| r.output)
}

#[test]
fn preparation_child_entry() -> Result<(), String> {
    match std::env::var("RIPR_PREPARATION_CONTROL").ok().as_deref() {
        None => Ok(()),
        Some("ready") => ensure_ready(),
        Some("offline") => prepare(&["--offline".into()]),
        Some("online") => prepare(&[]),
        Some(other) => Err(format!("unknown preparation control: {other}")),
    }
}

fn invoke_child(mode: &str, environment: &[(&'static str, String)]) -> Result<TimedOutput, String> {
    let mut environment = environment.to_vec();
    environment.push(("RIPR_PREPARATION_CONTROL", mode.into()));
    let envs = environment
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect::<Vec<_>>();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask parent missing")?;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let args = [
        "reports::fixtures::preparation::tests::preparation_child_entry",
        "--exact",
        "--nocapture",
    ]
    .map(String::from);
    capture_output_measured(
        &path_text(&executable)?,
        &args,
        Some(repo),
        &envs,
        Duration::from_secs(20),
        "production preparation control",
    )
    .map(|r| r.output)
}

#[test]
fn absent_pin_refuses_all_offline_proxies_without_installation() -> Result<(), String> {
    let _cwd = crate::acquire_test_cwd_read_guard();
    let root = OwnedRoot::new()?;
    let server = RejectingDistribution::new()?;
    let directory = proxy_directory()?;
    let environment = control_environment(&root, &root.0.join("rustup"), &directory, &server)?;
    for (program, args) in [
        ("cargo", vec!["--version".into()]),
        ("rustc", vec!["--version".into()]),
        ("cargo", metadata_args()),
        (
            "cargo",
            [
                "fetch",
                "--locked",
                "--offline",
                "--manifest-path",
                MANIFEST,
            ]
            .map(String::from)
            .to_vec(),
        ),
    ] {
        let output = invoke_proxy(&directory, program, &args, &environment, true)?;
        assert!(!output.timed_out, "{program}: {}", output.stderr);
        assert_eq!(
            output.status.and_then(|s| s.code()),
            Some(1),
            "{}",
            output.stderr
        );
        assert!(
            output.stderr.contains("not installed") && output.stderr.contains("1.95.0"),
            "{}",
            output.stderr
        );
        assert_eq!(
            server.requests.load(Ordering::SeqCst),
            0,
            "offline proxy attempted distribution access"
        );
        let installed = root.0.join("rustup/toolchains");
        if installed.exists() {
            assert!(
                std::fs::read_dir(installed)
                    .map_err(|e| e.to_string())?
                    .next()
                    .is_none()
            );
        }
    }
    for mode in ["ready", "offline"] {
        let output = invoke_child(mode, &environment)?;
        let detail = format!("{}{}", output.stdout, output.stderr);
        assert!(!output.timed_out);
        assert_eq!(output.status.and_then(|s| s.code()), Some(101), "{detail}");
        for expected in [
            "not installed",
            "RUSTUP_AUTO_INSTALL",
            "CARGO_HOME",
            "RUSTUP_HOME",
            "1.95.0",
        ] {
            assert!(detail.contains(expected), "missing {expected}: {detail}");
        }
        assert_eq!(server.requests.load(Ordering::SeqCst), 0);
    }
    // Sensitivity control through actual online preparation. Both distribution
    // endpoints still reject on loopback; no public download can succeed.
    let mut online = environment.clone();
    online.push(("CARGO_NET_OFFLINE", "false".into()));
    let output = invoke_child("online", &online)?;
    assert!(!output.timed_out);
    assert_eq!(output.status.and_then(|s| s.code()), Some(101));
    assert!(
        server.requests.load(Ordering::SeqCst) > 0,
        "online preparation did not reach the rejecting server: {}{}",
        output.stdout,
        output.stderr
    );
    Ok(())
}

#[test]
fn installed_pin_missing_locked_dependency_refuses_then_seeded_cache_succeeds() -> Result<(), String>
{
    let _cwd = crate::acquire_test_cwd_read_guard();
    let root = OwnedRoot::new()?;
    let server = RejectingDistribution::new()?;
    let directory = proxy_directory()?;
    let rustup = std::env::var_os("RUSTUP_HOME")
        .map(PathBuf::from)
        .or_else(|| home::home_dir().map(|p| p.join(".rustup")))
        .ok_or("installed Rustup home unavailable")?;
    let environment = control_environment(
        &root,
        &std::path::absolute(rustup).map_err(|e| e.to_string())?,
        &directory,
        &server,
    )?;
    let output = invoke_proxy(&directory, "cargo", &metadata_args(), &environment, true)?;
    assert!(!output.timed_out);
    assert!(output.status.is_some_and(|s| !s.success()));
    assert!(
        output.stderr.contains("anyhow"),
        "missing-dependency control did not reach Cargo: {}",
        output.stderr
    );
    for mode in ["ready", "offline"] {
        let output = invoke_child(mode, &environment)?;
        let detail = format!("{}{}", output.stdout, output.stderr);
        assert!(!output.timed_out);
        assert_eq!(output.status.and_then(|s| s.code()), Some(101), "{detail}");
        assert!(
            detail.contains("anyhow") && detail.contains("goldens prepare"),
            "{detail}"
        );
        if mode == "offline" {
            assert!(
                detail.contains("cargo 1.95.0") && detail.contains("rustc 1.95.0"),
                "version probes did not complete: {detail}"
            );
        }
    }
    let source = home::cargo_home().map_err(|e| e.to_string())?;
    let index = std::fs::read_dir(source.join("registry/index")).map_err(|e| e.to_string())?
        .filter_map(Result::ok).map(|e| e.path())
        .find(|p| p.join(".cache/an/yh/anyhow").is_file() && source.join("registry/cache").join(p.file_name().unwrap_or_default()).join("anyhow-1.0.100.crate").is_file())
        .ok_or("prepare canonical fixture dependencies first: cargo xtask goldens prepare (sparse registry cache)")?;
    let id = index.file_name().ok_or("registry id missing")?;
    let archive = source
        .join("registry/cache")
        .join(id)
        .join("anyhow-1.0.100.crate");
    let bytes = std::fs::read(&archive).map_err(|e| e.to_string())?;
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "a23eb6b1614318a8071c9b2521f36b424b2c83db5eb3a0fead4a6c0809af6e61"
    );
    for (from, to) in [
        (
            archive,
            root.0
                .join("cargo/registry/cache")
                .join(id)
                .join("anyhow-1.0.100.crate"),
        ),
        (
            index.join("config.json"),
            root.0
                .join("cargo/registry/index")
                .join(id)
                .join("config.json"),
        ),
        (
            index.join(".cache/an/yh/anyhow"),
            root.0
                .join("cargo/registry/index")
                .join(id)
                .join(".cache/an/yh/anyhow"),
        ),
    ] {
        std::fs::create_dir_all(to.parent().ok_or("seed parent missing")?)
            .map_err(|e| e.to_string())?;
        std::fs::copy(from, to).map_err(|e| e.to_string())?;
    }
    let output = invoke_proxy(&directory, "cargo", &metadata_args(), &environment, true)?;
    assert!(!output.timed_out);
    assert!(
        output.status.is_some_and(|s| s.success()),
        "{}",
        output.stderr
    );
    let metadata: serde_json::Value =
        serde_json::from_str(&output.stdout).map_err(|e| e.to_string())?;
    assert!(metadata["packages"].as_array().is_some_and(|packages| {
        packages
            .iter()
            .any(|p| p["name"] == "anyhow" && p["version"] == "1.0.100")
    }));
    for mode in ["ready", "offline", "online"] {
        let output = invoke_child(mode, &environment)?;
        assert!(!output.timed_out);
        assert_eq!(
            output.status.and_then(|s| s.code()),
            Some(0),
            "{}{}",
            output.stdout,
            output.stderr
        );
    }
    assert_eq!(server.requests.load(Ordering::SeqCst), 0);
    Ok(())
}
