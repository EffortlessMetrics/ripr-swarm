//! Candidate-bound, offline subprocesses for guide and source-package checks.
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(label: &str) -> Result<Self, String> {
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = parent.join(format!("ripr-{label}-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        Ok(Self(root))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            eprintln!(
                "failed to remove owned scratch {}: {error}",
                self.0.display()
            );
        }
    }
}

fn capture(mut command: Command, budget: Duration, group: bool) -> Result<Output, String> {
    #[cfg(not(unix))]
    let _ = group;
    let scratch = Scratch::new("help-command")?;
    let stdout = scratch.0.join("stdout");
    let stderr = scratch.0.join("stderr");
    let exclusive = |path: &Path| {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
    };
    command
        .stdin(Stdio::null())
        .stdout(exclusive(&stdout).map_err(|e| e.to_string())?)
        .stderr(exclusive(&stderr).map_err(|e| e.to_string())?);
    #[cfg(unix)]
    if group {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child =
        ripr::process_owner::OwnedProcess::spawn(command).map_err(|error| error.to_string())?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < budget => {
                std::thread::sleep(Duration::from_millis(10));
            }
            observation => {
                // Cargo/rustc descendants share this owned Unix process group;
                // Windows containment belongs to OwnedProcess's Job Object.
                #[cfg(unix)]
                let group_cleanup = if group {
                    let mut kill = Command::new("kill");
                    kill.args(["-KILL", "--", &format!("-{}", child.id())]);
                    capture(kill, Duration::from_secs(5), false)
                        .and_then(|out| require_success(out, "owned process-group termination"))
                        .map(|_| ())
                } else {
                    Ok(())
                };
                #[cfg(not(unix))]
                let group_cleanup: Result<(), String> = Ok(());
                let reap = child.terminate_tree();
                group_cleanup?;
                reap?;
                return Err(format!(
                    "help proof command exceeded {budget:?} or could not be observed: {observation:?}"
                ));
            }
        }
    };
    let read = |path: &Path| -> Result<Vec<u8>, String> {
        const CAP: u64 = 4 * 1024 * 1024;
        let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        file.take(CAP + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > CAP {
            return Err(format!(
                "{} exceeded the {CAP}-byte capture cap",
                path.display()
            ));
        }
        Ok(bytes)
    };
    Ok(Output {
        status,
        stdout: read(&stdout)?,
        stderr: read(&stderr)?,
    })
}

pub fn run(command: Command) -> Result<Output, String> {
    capture(command, Duration::from_secs(600), true)
}

pub fn require_success(output: Output, label: &str) -> Result<Output, String> {
    if !output.status.success() {
        return Err(format!(
            "{label} failed ({})\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output)
}

fn cargo() -> Command {
    let mut command = Command::new(env!("CARGO"));
    command
        .current_dir(workspace())
        .env("RUSTUP_AUTO_INSTALL", "0");
    command
}

pub fn candidate_binary() -> Result<PathBuf, String> {
    static BINARY: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            // Always ask Cargo to select/build the current candidate. An existing
            // target/debug/ripr or an ambient PATH binary is not identity evidence.
            let mut command = cargo();
            command.args([
                "build",
                "-p",
                "ripr",
                "--bin",
                "ripr",
                "--locked",
                "--offline",
                "--jobs",
                "1",
                "--message-format=json",
            ]);
            let output = require_success(run(command)?, "candidate ripr build")?;
            let mut executables = BTreeSet::new();
            for line in output
                .stdout
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
            {
                let value: serde_json::Value =
                    serde_json::from_slice(line).map_err(|error| error.to_string())?;
                if value["reason"] == "compiler-artifact"
                    && value["target"]["name"] == "ripr"
                    && value["target"]["kind"]
                        .as_array()
                        .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
                    && let Some(executable) = value["executable"].as_str()
                {
                    executables.insert(
                        PathBuf::from(executable)
                            .canonicalize()
                            .map_err(|error| error.to_string())?,
                    );
                }
            }
            if executables.len() != 1 {
                return Err(format!(
                    "Cargo selected {} ripr executables: {executables:?}",
                    executables.len()
                ));
            }
            executables
                .into_iter()
                .next()
                .ok_or_else(|| "Cargo selected no ripr executable".to_string())
        })
        .clone()
}

pub fn run_ripr(args: &[&str]) -> Result<Output, String> {
    let mut command = Command::new(candidate_binary()?);
    command.args(args).current_dir(workspace());
    capture(command, Duration::from_secs(30), true)
}

pub fn rendered_help(args: &[&str]) -> Result<String, String> {
    let output = require_success(run_ripr(args)?, "candidate rendered help")?;
    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

pub fn extracted_package(root: &Path) -> Result<PathBuf, String> {
    static ARCHIVE: OnceLock<Result<Vec<u8>, String>> = OnceLock::new();
    let bytes = ARCHIVE
        .get_or_init(|| {
            let scratch = Scratch::new("help-package")?;
            let mut command = cargo();
            command
                .args([
                    "package",
                    "-p",
                    "ripr",
                    "--locked",
                    "--offline",
                    "--no-verify",
                    "--allow-dirty",
                    "--target-dir",
                ])
                .arg(&scratch.0);
            require_success(run(command)?, "source package creation")?;
            let mut archives = std::fs::read_dir(scratch.0.join("package"))
                .map_err(|error| error.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "crate"))
                .collect::<Vec<_>>();
            if archives.len() != 1 {
                return Err(format!("expected one actual .crate, got {archives:?}"));
            }
            std::fs::read(archives.remove(0)).map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)?;
    let decoder = flate2::read::GzDecoder::new(bytes.as_slice());
    let mut archive = tar::Archive::new(decoder);
    let mut paths = BTreeSet::new();
    let mut roots = BTreeSet::new();
    for entry in archive.entries().map_err(|error| error.to_string())? {
        let mut entry = entry.map_err(|error| error.to_string())?;
        let path = entry
            .path()
            .map_err(|error| error.to_string())?
            .into_owned();
        if !entry.header().entry_type().is_file()
            || path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            || !paths.insert(path.clone())
        {
            return Err(format!("unexpected package member: {}", path.display()));
        }
        let first = path
            .components()
            .next()
            .ok_or_else(|| "empty package member".to_string())?;
        roots.insert(PathBuf::from(first.as_os_str()));
        entry.unpack_in(root).map_err(|error| error.to_string())?;
    }
    if roots.len() != 1 || paths.is_empty() {
        return Err(format!("expected one nonempty package root, got {roots:?}"));
    }
    let package = root.join(
        roots
            .into_iter()
            .next()
            .ok_or_else(|| "no package root".to_string())?,
    );
    if !package.join("Cargo.toml").is_file() {
        return Err("extracted package has no manifest".to_string());
    }
    Ok(package)
}

pub fn compile_portable(package: &Path, output: &Path) -> Result<Output, String> {
    let mut command = Command::new("rustc");
    command
        .current_dir(package)
        .args([
            "--edition=2024",
            "--test",
            "tests/cli_help_hierarchy.rs",
            "-o",
        ])
        .arg(output)
        .env("CARGO_BIN_EXE_ripr", candidate_binary()?)
        .env("RUSTUP_AUTO_INSTALL", "0");
    run(command)
}
