//! Explicit dependency preparation; golden checks remain offline and never bless.
//! Network boundary: only `goldens prepare` runs Cargo's locked registry fetch;
//! ordinary fixture readiness uses offline metadata in the same environment.
use crate::run::run_output_owned_with_envs;
use std::path::Path;
use std::sync::OnceLock;

pub(super) const FIXTURE: &str = "owner_return_pin_fallible_literal";
const MANIFEST: &str = "fixtures/owner_return_pin_fallible_literal/input/Cargo.toml";
static ENVIRONMENT: OnceLock<Result<Vec<(&'static str, String)>, String>> = OnceLock::new();
static READY: OnceLock<Result<(), String>> = OnceLock::new();

/// Capture once before parallel fixture execution. Relative CARGO_HOME must be
/// resolved here, before Cargo metadata changes the child's working directory.
pub(super) fn environment() -> Result<&'static Vec<(&'static str, String)>, String> {
    ENVIRONMENT
        .get_or_init(|| {
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            let cargo_home = home::cargo_home_with_cwd(&cwd)
                .map_err(|e| format!("resolve effective Cargo home: {e}; set CARGO_HOME to a writable absolute directory"))?;
            let pin = std::fs::read_to_string("rust-toolchain.toml")
                .map_err(|e| format!("read repository toolchain pin: {e}; run from the repository root"))?;
            let toolchain = toolchain(&pin)?;
            let mut envs = vec![
                ("CARGO_HOME", path_text(&cargo_home)?),
                ("RUSTUP_TOOLCHAIN", toolchain),
            ];
            // Preserve the selected rustup store when provided, including a
            // relative store, rather than resolving it from a fixture root.
            if let Some(value) = std::env::var_os("RUSTUP_HOME") {
                let path = Path::new(&value);
                envs.push(("RUSTUP_HOME", path_text(&cwd.join(path))?));
            }
            Ok(envs)
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn path_text(path: &Path) -> Result<String, String> {
    path.to_str().map(String::from).ok_or_else(|| {
        format!(
            "fixture preparation requires a UTF-8 environment path: {}",
            path.display()
        )
    })
}

fn toolchain(pin: &str) -> Result<String, String> {
    let value: toml::Value =
        toml::from_str(pin).map_err(|e| format!("parse toolchain pin: {e}"))?;
    value
        .get("toolchain")
        .and_then(|v| v.get("channel"))
        .and_then(toml::Value::as_str)
        .filter(|v| !v.is_empty())
        .map(String::from)
        .ok_or_else(|| "rust-toolchain.toml must name toolchain.channel".into())
}

/// Cargo's offline flag is parsed after rustup selects a toolchain. Bind the
/// launcher policy as well, overriding even an ambient AUTO_INSTALL=1.
fn proxy_environment(
    environment: &[(&'static str, String)],
    offline: bool,
) -> Vec<(&'static str, String)> {
    let mut envs = environment.to_vec();
    if offline {
        envs.push(("RUSTUP_AUTO_INSTALL", "0".into()));
    }
    envs
}

fn offline_mode(requested: bool) -> bool {
    requested || std::env::var("CARGO_NET_OFFLINE").is_ok_and(|value| value == "true")
}

fn proxy_output(program: &str, args: &[String], offline: bool) -> Result<String, String> {
    let environment = proxy_environment(environment()?, offline_mode(offline));
    let envs = environment
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect::<Vec<_>>();
    run_output_owned_with_envs(program, args, &envs)
        .map_err(|e| preparation_error(&binding(&environment), &e))
}

pub(super) fn build(args: &[&str]) -> Result<(), String> {
    let environment = proxy_environment(environment()?, offline_mode(args.contains(&"--offline")));
    let envs = environment
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect::<Vec<_>>();
    crate::run::run_with_envs("cargo", args, &envs).map(|_| ())
}

fn binding(environment: &[(&str, String)]) -> String {
    environment
        .iter()
        .map(|(k, v)| format!("{k}={v:?}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn preparation_error(binding: &str, error: &str) -> String {
    format!(
        "fixture dependency preparation failed for {FIXTURE}\nenvironment: {binding}\n{error}\nnext: run `cargo xtask goldens prepare` with this same environment (or `cargo xtask goldens prepare --offline` with a seeded cache), then retry the fixture/golden check. The fixture requires its locked anyhow 1.0.100; fetching another Cargo home does not prepare this subprocess. If the pinned toolchain is missing, install it explicitly before retrying offline. Do not bless availability drift."
    )
}

fn metadata_args() -> Vec<String> {
    [
        "metadata",
        "--locked",
        "--offline",
        "--format-version",
        "1",
        "--manifest-path",
        MANIFEST,
    ]
    .map(String::from)
    .to_vec()
}

pub(super) fn ensure_ready() -> Result<(), String> {
    READY
        .get_or_init(|| proxy_output("cargo", &metadata_args(), true).map(|_| ()))
        .clone()
}

pub(super) fn prepare(args: &[String]) -> Result<(), String> {
    if !args.is_empty() && args != ["--offline"] {
        return Err("usage: cargo xtask goldens prepare [--offline]".into());
    }
    let offline = offline_mode(!args.is_empty());
    println!(
        "fixture: {FIXTURE}\nenvironment: {}",
        binding(&proxy_environment(environment()?, offline))
    );
    for program in ["cargo", "rustc"] {
        println!(
            "{}",
            proxy_output(program, &["--version".into()], offline)?.trim()
        );
    }
    let mut fetch = ["fetch", "--locked", "--manifest-path", MANIFEST]
        .map(String::from)
        .to_vec();
    fetch.extend_from_slice(args);
    proxy_output("cargo", &fetch, offline)?;
    // A successful fetch alone is not readiness: verify locked resolution with
    // the same offline metadata command the golden runner will use.
    proxy_output("cargo", &metadata_args(), true)?;
    println!("prepared {FIXTURE}; retry fixture/golden checks with the environment above");
    Ok(())
}

#[cfg(test)]
mod tests;
