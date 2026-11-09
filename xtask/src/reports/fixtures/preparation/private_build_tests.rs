//! Exercise the private output owners with real Cargo builds without registry dependencies.
//! Both workspaces predate every compile. Their equal-length path dependency
//! sources have old mtimes and different behavior, as in #5036/#5038. A private
//! final target alone must not admit shared intermediate fingerprints or OUT_DIR.
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::native_libtest::{CompileRequest, compile_libtest, replay_filter};
use crate::run::capture_bytes_in_dir_with_timeout;

const CHILD_ROOT: &str = "RIPR_PRIVATE_BUILD_TEST_ROOT";
const CHILD_SUBJECT: &str = "RIPR_PRIVATE_BUILD_TEST_SUBJECT";
const OBSERVATION: &str = "RIPR_PRIVATE_BUILD_OBSERVATION";
const CHILD_TEST: &str = "reports::fixtures::preparation::private_build_tests::private_build_child";
const OWNERS: [&str; 3] = ["libtest", "default", "perl"];

fn write(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("fixture path has no parent")?)
        .map_err(|error| error.to_string())?;
    fs::write(path, contents).map_err(|error| error.to_string())
}

fn fixture(root: &Path, subject: &str, behavior: &str, config: &str) -> Result<(), String> {
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"ripr\"\nversion = \"0.0.1\"\nedition = \"2024\"\n\
         [workspace]\nmembers = [\"core\"]\nresolver = \"2\"\n\
         [dependencies]\nprivate-build-core = { path = \"core\" }\n\
         [features]\nlang-perl = []\n\
         [profile.dev]\ndebug = 0\nincremental = false\n",
    )?;
    write(
        &root.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"private-build-core\"\nversion = \"0.0.1\"\n\n\
         [[package]]\nname = \"ripr\"\nversion = \"0.0.1\"\n\
         dependencies = [\"private-build-core\"]\n",
    )?;
    write(
        &root.join("core/Cargo.toml"),
        "[package]\nname = \"private-build-core\"\nversion = \"0.0.1\"\nedition = \"2024\"\n",
    )?;
    let core = root.join("core/src/lib.rs");
    write(
        &core,
        format!("pub fn behavior() -> &'static str {{ \"{behavior}\" }}\n"),
    )?;
    fs::OpenOptions::new()
        .write(true)
        .open(core)
        .and_then(|file| file.set_modified(UNIX_EPOCH + Duration::from_hours(262_968)))
        .map_err(|error| error.to_string())?;
    write(
        &root.join("build.rs"),
        "fn main() {\n\
         println!(\"cargo::rerun-if-changed=build.rs\");\n\
         let out = std::path::PathBuf::from(std::env::var_os(\"OUT_DIR\").unwrap());\n\
         std::fs::write(out.join(\"owner-marker\"), b\"private build artifact\\n\").unwrap();\n\
         }\n",
    )?;
    write(
        &root.join("src/lib.rs"),
        format!(
            "pub fn observation() -> String {{\n\
             format!(\"subject={subject}\\ncore={{}}\\nconfig={{}}\\nperl={{}}\\nout={{}}\\n\",\n\
             private_build_core::behavior(), env!(\"RIPR_PRIVATE_BUILD_CONFIG_MARKER\"),\n\
             cfg!(feature = \"lang-perl\"), env!(\"OUT_DIR\"))\n\
             }}\n\
             #[cfg(test)] mod tests {{\n\
             #[test] fn private_build_observation() {{\n\
             let path = std::env::var_os(\"{OBSERVATION}\").unwrap();\n\
             std::fs::write(path, super::observation()).unwrap();\n\
             }}\n}}\n"
        ),
    )?;
    // Ensure the current top-level source rebuilds even if a broken owner
    // reuses the old dependency's fingerprints from the other workspace.
    fs::OpenOptions::new()
        .write(true)
        .open(root.join("src/lib.rs"))
        .and_then(|file| file.set_modified(SystemTime::now() + Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    write(
        &root.join("src/main.rs"),
        "fn main() { print!(\"{}\", ripr::observation()); }\n",
    )?;
    write(&root.join(".cargo/config.toml"), config)
}

#[test]
#[ignore = "isolated subprocess entry, exercised by the private-output regressions"]
fn private_build_child() -> Result<(), String> {
    let root = PathBuf::from(std::env::var_os(CHILD_ROOT).ok_or("missing fixture root")?);
    let subject = std::env::var(CHILD_SUBJECT).map_err(|error| error.to_string())?;
    let workspace = std::env::current_dir().map_err(|error| error.to_string())?;
    let evidence = root.join("evidence");
    for owner in OWNERS {
        let target = root.join("owned").join(&subject).join(owner);
        println!("private output probe: subject={subject} owner={owner}");
        if owner == "libtest" {
            let receipt = compile_libtest(&CompileRequest {
                root: &workspace,
                target_dir: &target,
                package: "ripr",
                locked: true,
                offline: true,
                timeout: Duration::from_secs(30),
            })?;
            assert_eq!(receipt.schema_version, "ripr_native_libtest_build.v1");
            assert_eq!(
                receipt.cargo_argv,
                [
                    "cargo",
                    "test",
                    "--locked",
                    "--offline",
                    "-p",
                    "ripr",
                    "--lib",
                    "--no-run",
                    "--message-format=json"
                ]
            );
            write(
                &evidence.join(format!("{subject}-libtest.receipt.json")),
                serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?,
            )?;
            let replay = replay_filter(&receipt, &workspace, "tests::private_build_observation")?;
            assert_eq!(replay.executed, 1);
            assert_eq!(replay.passed, 1);
            assert!(replay.process_tree_settled);
        } else {
            if owner == "default" {
                super::build_default_isolated(&target)?;
            } else {
                super::build_perl_variant(&target)?;
            }
            let binary = target
                .join("debug")
                .join(format!("ripr{}", std::env::consts::EXE_SUFFIX));
            let output = capture_bytes_in_dir_with_timeout(
                &binary,
                &[],
                &workspace,
                &[],
                &[],
                Duration::from_secs(10),
                "private fixture binary observation",
            )?;
            if output.timed_out || !output.status.is_some_and(|status| status.success()) {
                return Err(format!(
                    "{owner} fixture did not execute successfully: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            write(
                &evidence.join(format!("{subject}-{owner}.txt")),
                output.stdout,
            )?;
        }
    }
    Ok(())
}

fn run_case(inherited: bool) -> Result<(), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr private output {}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&root).map_err(|error| error.to_string())?;
    println!("private-build regression root: {}", root.display());
    let result = run_case_in(&root, inherited);
    if result.is_ok() {
        // All owned children returned terminal status. Never remove a caller's
        // configured build store; only this exclusively created fixture root.
        fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    }
    result.map_err(|error| {
        format!(
            "{error}; retained private-build evidence: {}",
            root.display()
        )
    })
}

fn run_case_in(root: &Path, inherited: bool) -> Result<(), String> {
    let configured = root.join("configured pool");
    let ambient = root.join("inherited intermediates");
    let final_redirect = root.join("inherited final output");
    let cargo_home = root.join("cargo-home");
    let temp = root.join("temp");
    for directory in [&cargo_home, &temp, &root.join("evidence")] {
        fs::create_dir(directory).map_err(|error| error.to_string())?;
    }
    // The configured pool exercises the workspace-path-hash layout as well as
    // file precedence. An inherited override instead points both workspaces at
    // one shared directory, reproducing the stale-dependency risk from #5036.
    let config = format!(
        "[build]\nbuild-dir = {}\ntarget-dir = {}\n[env]\n\
         RIPR_PRIVATE_BUILD_CONFIG_MARKER = {{ value = \"preserved\", force = true }}\n\
         TEMP = {{ value = {temp}, force = true }}\n\
         TMP = {{ value = {temp}, force = true }}\n\
         TMPDIR = {{ value = {temp}, force = true }}\n",
        toml::Value::String(
            configured
                .join("{workspace-path-hash}")
                .to_string_lossy()
                .into_owned()
        ),
        toml::Value::String(final_redirect.to_string_lossy().into_owned()),
        temp = toml::Value::String(temp.to_string_lossy().into_owned()),
    );
    fixture(&root.join("A"), "A", "OLD", &config)?;
    fixture(&root.join("B"), "B", "NEW", &config)?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    for subject in ["A", "B"] {
        let observation = root.join("evidence").join(format!("{subject}-libtest.txt"));
        let mut environment = vec![
            (CHILD_ROOT, path_text(root)?),
            (CHILD_SUBJECT, subject.to_string()),
            (OBSERVATION, path_text(&observation)?),
            ("CARGO_HOME", path_text(&cargo_home)?),
            ("CARGO_TARGET_DIR", path_text(&final_redirect)?),
            ("CARGO_NET_OFFLINE", "true".into()),
            ("RUSTUP_AUTO_INSTALL", "0".into()),
            ("CARGO_BUILD_JOBS", "1".into()),
            ("CARGO_INCREMENTAL", "0".into()),
        ];
        if inherited {
            environment.push(("CARGO_BUILD_BUILD_DIR", path_text(&ambient)?));
        }
        let environment = environment
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect::<Vec<_>>();
        let output = capture_bytes_in_dir_with_timeout(
            &executable,
            &[
                "--exact".into(),
                CHILD_TEST.into(),
                "--ignored".into(),
                "--nocapture".into(),
            ],
            &root.join(subject),
            &environment,
            &[
                "CARGO_BUILD_BUILD_DIR",
                "CARGO_BUILD_TARGET",
                "RUSTC",
                "RUSTFLAGS",
                "CARGO_ENCODED_RUSTFLAGS",
                "RUSTC_WRAPPER",
                "RUSTC_WORKSPACE_WRAPPER",
                "CARGO_BUILD_RUSTC_WRAPPER",
                "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
            ],
            Duration::from_secs(60),
            "private Cargo output owner regression",
        )?;
        write(&root.join(format!("{subject}-stdout.bin")), &output.stdout)?;
        write(&root.join(format!("{subject}-stderr.bin")), &output.stderr)?;
        if output.timed_out || !output.status.is_some_and(|status| status.success()) {
            return Err(format!(
                "private-build child failed: stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    for (subject, behavior) in [("A", "OLD"), ("B", "NEW")] {
        assert_eq!(
            fs::read_to_string(root.join(subject).join(".cargo/config.toml"))
                .map_err(|error| error.to_string())?,
            config
        );
        for owner in OWNERS {
            let target = root.join("owned").join(subject).join(owner);
            let observation =
                fs::read_to_string(root.join("evidence").join(format!("{subject}-{owner}.txt")))
                    .map_err(|error| error.to_string())?;
            let prefix = format!(
                "subject={subject}\ncore={behavior}\nconfig=preserved\nperl={}\nout=",
                owner == "perl"
            );
            let out_dir = observation.strip_prefix(&prefix).ok_or_else(|| {
                format!("wrong private-build behavior for {subject}/{owner}: {observation}")
            })?;
            let out_dir =
                fs::canonicalize(out_dir.trim_end()).map_err(|error| error.to_string())?;
            let canonical_target = fs::canonicalize(&target).map_err(|error| error.to_string())?;
            if !out_dir.starts_with(&canonical_target) {
                return Err(format!(
                    "{subject}/{owner} build-script OUT_DIR escaped private owner: {}",
                    out_dir.display()
                ));
            }
            assert_eq!(
                fs::read(out_dir.join("owner-marker")).map_err(|error| error.to_string())?,
                b"private build artifact\n"
            );
            assert!(target.join("debug/.fingerprint").is_dir());
            // Receipts and observations are independent retained evidence, not
            // descendants of the disposable final/intermediate output owner.
            let retained = root.join("evidence").join(format!("{subject}-{owner}.txt"));
            fs::remove_dir_all(&target).map_err(|error| error.to_string())?;
            assert_eq!(
                fs::read_to_string(retained).map_err(|error| error.to_string())?,
                observation
            );
        }
        assert!(
            root.join("evidence")
                .join(format!("{subject}-libtest.receipt.json"))
                .is_file()
        );
        assert!(!root.join(subject).join("target").exists());
    }
    for redirect in [configured, ambient, final_redirect] {
        if redirect.exists() {
            return Err(format!(
                "private build populated an unowned redirect: {}",
                redirect.display()
            ));
        }
    }
    Ok(())
}

fn path_text(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(String::from)
        .ok_or("fixture path is not UTF-8".into())
}

#[test]
fn private_build_owners_override_inherited_intermediates() -> Result<(), String> {
    run_case(true)
}

#[test]
fn private_build_owners_override_configured_intermediate_pool() -> Result<(), String> {
    run_case(false)
}
