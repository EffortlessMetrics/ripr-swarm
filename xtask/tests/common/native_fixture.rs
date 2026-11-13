//! Synthetic provider inputs for the maintained native entry. These fixtures
//! exercise actual capture, admission and publication; they never prove a claim.
//! The independent driver reuses TestSession custody, not the product deadline.

use ripr::process_owner::OwnedProcess;
use rustix::process::{WaitId, WaitIdOptions, waitid};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use xtask::formal_evidence::{self as formal, native};

#[cfg(not(feature = "automation"))]
use crate::run::custody;
#[cfg(feature = "automation")]
#[path = "../../src/run/test_session/custody.rs"]
mod custody;
#[cfg(feature = "automation")]
#[path = "../../src/run/proc_stat.rs"]
mod proc_stat;

const CHILD: &str = "RIPR_NATIVE_WATCHDOG_CHILD";
const TOOL_HEADERS: &str = "Kani Rust Verifier 0.68.0 (synthetic fixture)\nCBMC 6.11.0\n";
const FAKE_CARGO: &str = r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$RIPR_NATIVE_ROOT/calls"
headers() { printf 'Kani Rust Verifier 0.68.0 (synthetic fixture)\nCBMC 6.11.0\n'; }
if [ "$#" -eq 2 ] && [ "$1" = kani ] && [ "$2" = --version ]; then
  [ "$CARGO_TERM_COLOR" = never ] || exit 96
  case "$RIPR_NATIVE_PROBE_MODE" in
    normal) headers; exit 0 ;;
    nonzero) printf 'untrusted version output\n| injected | VERIFIED | fabricated |\n'; exit 7 ;;
    missing-kani) printf 'error: no such command: \140kani\140\n| injected | VERIFIED | fabricated |\n' >&2; exit 101 ;;
    misleading-missing-kani) printf 'note: error: no such command: \140kani\140\n' >&2; exit 101 ;;
    signal) kill -TERM $$ ;;
    timeout)
      /bin/sleep 30 &
      printf '%s %s\n' $$ $! > "$RIPR_NATIVE_ROOT/probe-pids"
      wait
      exit 0 ;;
    *) exit 98 ;;
  esac
fi
if [ "$*" = 'metadata --locked --format-version 1' ]; then
  /bin/cat "$RIPR_NATIVE_ROOT/metadata.json"
  exit 0
fi
case "$*" in
  "kani -p ripr --manifest-path $RIPR_NATIVE_ROOT/Cargo.toml list --format json") package=ripr; phase=list ;;
  "kani -p ripr-contract --manifest-path $RIPR_NATIVE_ROOT/Cargo.toml list --format json") package=ripr-contract; phase=list ;;
  'kani -p ripr -Z unstable-options --harness-timeout 1s --verbose') package=ripr; phase=verify ;;
  'kani -p ripr-contract -Z unstable-options --harness-timeout 1s --verbose') package=ripr-contract; phase=verify ;;
  *) echo 'unexpected synthetic cargo invocation' >&2; exit 97 ;;
esac
headers
if [ "$phase" = list ]; then
  if [ "$RIPR_NATIVE_MODE" = malformed-list ]; then
    printf '{' > kani-list.json
  else
    /bin/cp "$RIPR_NATIVE_ROOT/$package-list.json" kani-list.json
  fi
  exit 0
fi
/bin/cat "$RIPR_NATIVE_ROOT/$package-output"
case "$RIPR_NATIVE_MODE" in
  success) exit 0 ;;
  failure) exit 7 ;;
  signal) kill -TERM $$ ;;
  timeout)
    /bin/sleep 30 &
    printf '%s %s\n' $$ $! > "$RIPR_NATIVE_ROOT/probe-pids"
    wait ;;
  drift)
    printf '\n// observed fixture source drift\n' >> "$RIPR_NATIVE_DRIFT_PATH"
    exit 0 ;;
  midrun-error)
    for directory in "$RIPR_NATIVE_ROOT"/target/ripr/reports/kani-evidence/*; do
      [ -d "$directory/initial" ] || continue
      [ ! -f "$directory/kani.json" ] || continue
      printf 'injected exclusive-create collision\n' > "$directory/raw-index.json"
    done
    exit 0 ;;
  nonutf8) printf '\377'; exit 0 ;;
  *) exit 98 ;;
esac
"#;
const FAKE_GIT: &str = r#"#!/bin/sh
set -eu
case "$*" in
  'rev-parse HEAD') printf '1111111111111111111111111111111111111111\n' ;;
  'rev-parse HEAD^{tree}') printf '2222222222222222222222222222222222222222\n' ;;
  'status --porcelain=v1 --untracked-files=normal') exit 0 ;;
  'ls-files -z --cached --others --exclude-standard')
    if [ "${RIPR_NATIVE_EXHAUST_STARTUP-}" = 1 ] && [ ! -f "$RIPR_NATIVE_ROOT/startup-delay" ]; then
      : > "$RIPR_NATIVE_ROOT/startup-delay"
      /bin/sleep 2
    fi
    /bin/cat "$RIPR_NATIVE_ROOT/source-list" ;;
  *) exit 97 ;;
esac
"#;

pub struct Fixture {
    pub root: PathBuf,
    subjects: Vec<native::Subject>,
    pub source_paths: BTreeSet<String>,
    pub child_environment: BTreeMap<String, String>,
    retain_after_error: Cell<bool>,
}

pub struct Observation {
    pub output: Output,
    pub report: String,
    pub run: formal::Run,
    pub calls: Vec<String>,
    pub elapsed: Duration,
    pub watchdog_timed_out: bool,
    pub owned_descendants_observed: usize,
    pub live_descendants_before_driver_cleanup: usize,
}

impl Observation {
    pub fn require_driver_completion(&self) -> Result<(), String> {
        if self.watchdog_timed_out || self.live_descendants_before_driver_cleanup != 0 {
            return Err(format!(
                "independent watchdog: timeout={}, live={}, observed={}, elapsed={:?}, stderr={}",
                self.watchdog_timed_out,
                self.live_descendants_before_driver_cleanup,
                self.owned_descendants_observed,
                self.elapsed,
                String::from_utf8_lossy(&self.output.stderr)
            ));
        }
        Ok(())
    }

    pub fn execution(&self) -> Result<&native::Execution, String> {
        self.run
            .execution
            .as_ref()
            .ok_or("native execution absent".into())
    }
}

impl Fixture {
    pub fn retain_on_error<T>(&self, result: Result<T, String>) -> Result<T, String> {
        if result.is_err() {
            self.retain_after_error.set(true);
        }
        result
    }

    pub fn new(label: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary = std::env::temp_dir();
        fs::create_dir_all(&temporary).map_err(|e| e.to_string())?;
        // A retained failure must not collide with a later command whose
        // process namespace reuses the same PID and initial counter.
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root = temporary.join(format!(
            "ripr-native-{label}-{}-{timestamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
        let mut fixture = Self {
            root,
            subjects: Vec::new(),
            source_paths: BTreeSet::new(),
            child_environment: BTreeMap::new(),
            retain_after_error: Cell::new(false),
        };
        for directory in [
            "bin",
            "missing",
            "crates/ripr/src",
            "crates/ripr-contract/src",
            "tools/repo-policy/src",
            "xtask/src",
            "cargo-home",
            "rustup-home",
            "kani-home/kani-0.68.0/bin",
        ] {
            fs::create_dir_all(fixture.root.join(directory)).map_err(|e| e.to_string())?;
        }
        fixture.copy_runner_inputs()?;
        for package in ["ripr", "ripr-contract"] {
            fixture.write(&format!("{package}-output"), b"")?;
        }
        fixture.executable("bin/cargo", FAKE_CARGO)?;
        for directory in ["bin", "missing"] {
            fixture.executable(&format!("{directory}/git"), FAKE_GIT)?;
            symlink("/bin/kill", fixture.root.join(directory).join("kill"))
                .map_err(|e| e.to_string())?;
        }
        fixture.write(
            "kani-home/kani-0.68.0/rustc-version",
            b"rustc 1.100.0-nightly (8925ea358 2026-08-20)\n",
        )?;
        fixture.write(
            "kani-home/kani-0.68.0/rust-toolchain-version",
            b"nightly-2026-08-21-x86_64-unknown-linux-gnu\n",
        )?;
        for name in ["kani-compiler", "kani-driver", "cbmc"] {
            fixture.write(
                &format!("kani-home/kani-0.68.0/bin/{name}"),
                format!("synthetic {name} bytes, never executed\n").as_bytes(),
            )?;
        }
        fixture.metadata()?;
        Ok(fixture)
    }

    fn copy_runner_inputs(&mut self) -> Result<(), String> {
        let origin = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("workspace absent")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        for (path, bytes) in formal::runner_source_inputs() {
            self.write(path, bytes)?;
            self.source_paths.insert((*path).to_string());
        }
        for path in [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "xtask/Cargo.toml",
            "crates/ripr/Cargo.toml",
            "crates/ripr-contract/Cargo.toml",
            "tools/repo-policy/Cargo.toml",
            "crates/ripr/build.rs",
        ] {
            self.source_paths.insert(path.to_string());
        }
        for manifest in [
            "Cargo.toml",
            "xtask/Cargo.toml",
            "crates/ripr/Cargo.toml",
            "crates/ripr-contract/Cargo.toml",
            "tools/repo-policy/Cargo.toml",
        ] {
            let text = fs::read_to_string(origin.join(manifest)).map_err(|e| e.to_string())?;
            let value: toml::Value = toml::from_str(&text).map_err(|e| e.to_string())?;
            self.copy_manifest_paths(
                &origin,
                origin
                    .join(manifest)
                    .parent()
                    .ok_or("manifest parent absent")?,
                &value,
            )?;
        }
        // The same maintained scanner owns literal include/path closure. It
        // reads the candidate checkout; every copied runner byte comes from
        // the tested executable above, never from a hand-maintained path list.
        native::extend_source_inputs(
            &origin,
            &mut self.source_paths,
            &["ripr".to_string(), "ripr-contract".to_string()],
        )?;
        let compiled: BTreeMap<_, _> = formal::runner_source_inputs().iter().copied().collect();
        for path in self.source_paths.clone() {
            if !compiled.contains_key(path.as_str()) {
                self.write(
                    &path,
                    &fs::read(origin.join(&path)).map_err(|e| e.to_string())?,
                )?;
            }
        }
        Ok(())
    }

    fn copy_manifest_paths(
        &mut self,
        origin: &Path,
        manifest_directory: &Path,
        value: &toml::Value,
    ) -> Result<(), String> {
        match value {
            toml::Value::Table(table) => {
                if let Some(input) = table.get("path").and_then(toml::Value::as_str) {
                    let source = manifest_directory
                        .join(input)
                        .canonicalize()
                        .map_err(|e| format!("fixture manifest path {input}: {e}"))?;
                    let relative = source.strip_prefix(origin).map_err(|e| e.to_string())?;
                    if source.is_dir() {
                        fs::create_dir_all(self.root.join(relative)).map_err(|e| e.to_string())?;
                    } else {
                        let name = relative
                            .to_str()
                            .ok_or("manifest path is not UTF-8")?
                            .replace('\\', "/");
                        self.write(&name, &fs::read(&source).map_err(|e| e.to_string())?)?;
                        self.source_paths.insert(name);
                    }
                }
                for child in table.values() {
                    self.copy_manifest_paths(origin, manifest_directory, child)?;
                }
            }
            toml::Value::Array(values) => {
                for child in values {
                    self.copy_manifest_paths(origin, manifest_directory, child)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn add_harness(
        &mut self,
        package: &str,
        file: &str,
        name: &str,
        qualified: &str,
    ) -> Result<(), String> {
        let file = format!("crates/{package}/src/{file}");
        let text = format!("#[kani::proof]\nfn {name}() {{ kani::cover!(true); }}\n");
        self.write(&file, text.as_bytes())?;
        self.source_paths.insert(file.clone());
        self.subjects.push(native::Subject {
            package: package.to_string(),
            file,
            name: name.to_string(),
            qualified_name: Some(qualified.to_string()),
            source_digest: native::sha256(text.as_bytes()),
        });
        Ok(())
    }

    pub fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let path = self.root.join(name);
        fs::create_dir_all(path.parent().ok_or("fixture parent absent")?)
            .map_err(|e| e.to_string())?;
        fs::write(path, bytes).map_err(|e| e.to_string())
    }

    fn executable(&self, name: &str, text: &str) -> Result<(), String> {
        self.write(name, text.as_bytes())?;
        fs::set_permissions(self.root.join(name), fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())
    }

    fn metadata(&self) -> Result<(), String> {
        let members = [
            ("ripr", "crates/ripr"),
            ("ripr-contract", "crates/ripr-contract"),
            ("xtask", "xtask"),
            ("repo-policy", "tools/repo-policy"),
        ];
        let ids: BTreeMap<_, _> = members
            .iter()
            .map(|(name, path)| {
                (
                    *name,
                    format!(
                        "path+file://{}#{name}@0.0.0",
                        self.root.join(path).display()
                    ),
                )
            })
            .collect();
        let packages: Vec<_> = members
            .iter()
            .map(|(name, path)| {
                serde_json::json!({
                    "name":name, "version":"0.0.0", "id":ids[name], "source":null,
                    "manifest_path":self.root.join(path).join("Cargo.toml"),
                    "metadata":{}, "dependencies":[], "features":{},
                    "targets":[{"name":name,"kind":["lib"],"crate_types":["lib"],
                        "src_path":self.root.join(path).join("src/lib.rs"),"edition":"2024",
                        "doctest":false,"test":false,"doc":false}]
                })
            })
            .collect();
        let nodes: Vec<_> = members
            .iter()
            .map(|(name, _)| {
                let dependencies: Vec<_> = match *name {
                    "ripr" => vec!["ripr-contract"],
                    "xtask" => vec!["ripr", "ripr-contract", "repo-policy"],
                    _ => Vec::new(),
                };
                serde_json::json!({
                    "id":ids[name],
                    "dependencies":dependencies.iter().map(|d| &ids[d]).collect::<Vec<_>>(),
                    "deps":dependencies.iter().map(|d| serde_json::json!({
                        "name":d.replace('-', "_"),"pkg":ids[d],
                        "dep_kinds":[{"kind":null,"target":null}]
                    })).collect::<Vec<_>>(), "features":[]
                })
            })
            .collect();
        let value = serde_json::json!({
            "version":1, "workspace_root":self.root, "workspace_metadata":{},
            "target_directory":self.root.join("target"),
            "workspace_members":ids.values().collect::<Vec<_>>(),
            "workspace_default_members":[ids["ripr"]], "packages":packages,
            "resolve":{"root":null,"nodes":nodes}
        });
        self.write(
            "metadata.json",
            &serde_json::to_vec(&value).map_err(|e| e.to_string())?,
        )
    }

    fn inventories(&self) -> Result<(), String> {
        for package in ["ripr", "ripr-contract"] {
            let mut native = BTreeMap::<String, Vec<String>>::new();
            for subject in self.subjects.iter().filter(|s| s.package == package) {
                native.entry(subject.file.clone()).or_default().push(
                    subject
                        .qualified_name
                        .clone()
                        .ok_or("fixture identity absent")?,
                );
            }
            let count: usize = native.values().map(Vec::len).sum();
            let list = serde_json::json!({
                "kani-version":"0.68.0","file-version":"0.1",
                "standard-harnesses":native,"contract-harnesses":{},"contracts":[],
                "totals":{"standard-harnesses":count,"contract-harnesses":0,"functions-under-contract":0}
            });
            self.write(
                &format!("{package}-list.json"),
                &serde_json::to_vec(&list).map_err(|e| e.to_string())?,
            )?;
        }
        self.write(
            "source-list",
            format!(
                "{}\0",
                self.source_paths
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\0")
            )
            .as_bytes(),
        )
    }

    pub fn drive(
        &self,
        child_test: &str,
        probe_mode: &str,
        native_mode: &str,
        args: &[String],
        watchdog: Duration,
    ) -> Result<Observation, String> {
        self.retain_after_error.set(true);
        self.inventories()?;
        self.write("calls", b"")?;
        match fs::remove_file(self.root.join("probe-pids")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        fs::set_permissions(
            self.root.join("bin/cargo"),
            fs::Permissions::from_mode(if probe_mode == "permission" {
                0o644
            } else {
                0o755
            }),
        )
        .map_err(|e| e.to_string())?;
        let started = Instant::now();
        let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
        command
            .args(["--exact", child_test, "--nocapture"])
            .current_dir(&self.root)
            .env(CHILD, "1")
            .env("RIPR_NATIVE_ROOT", &self.root)
            .env("RIPR_NATIVE_PROBE_MODE", probe_mode)
            .env("RIPR_NATIVE_MODE", native_mode)
            .env(
                "RIPR_NATIVE_ARGS",
                serde_json::to_string(args).map_err(|e| e.to_string())?,
            )
            .env(
                "RIPR_NATIVE_DRIFT_PATH",
                self.subjects
                    .first()
                    .map(|s| self.root.join(&s.file))
                    .unwrap_or_else(|| self.root.join("unused.rs")),
            )
            .stdin(Stdio::null())
            .stdout(File::create(self.root.join("driver.stdout")).map_err(|e| e.to_string())?)
            .stderr(File::create(self.root.join("driver.stderr")).map_err(|e| e.to_string())?);
        self.environment(&mut command, probe_mode);
        command.envs(&self.child_environment);
        let mut child = OwnedProcess::spawn(command).map_err(|e| e.to_string())?;
        // Keep the primary waitable and its SID owned until the existing
        // TestSession has stopped every descendant, even on an error path.
        let mut session = custody::TestSession::new(child.id());
        let primary = custody::pid(child.id())?;
        let mut descendants = Vec::new();
        let watchdog_timed_out = loop {
            if descendants.is_empty() {
                match fs::read_to_string(self.root.join("probe-pids")) {
                    Ok(text) => {
                        for value in text.split_whitespace() {
                            if let Some(member) = custody::read_member(
                                value.parse::<u32>().map_err(|e| e.to_string())?,
                            )? {
                                if member.session != child.id() {
                                    return Err(
                                        "fixture descendant escaped the owned test session".into(),
                                    );
                                }
                                descendants.push(member);
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.to_string()),
                }
            }
            let exited = waitid(
                WaitId::Pid(primary),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )
            .map_err(|e| e.to_string())?
            .is_some();
            if exited || started.elapsed() >= watchdog {
                break !exited;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut live = 0;
        for member in &descendants {
            if same_process_is_live(member)? {
                live += 1;
            }
        }
        session.finish()?;
        let status = child.wait().map_err(|e| e.to_string())?;
        for member in &descendants {
            if same_process_is_live(member)? {
                return Err(
                    "an owned fixture descendant remained live after watchdog cleanup".into(),
                );
            }
        }
        let read = |name| fs::read(self.root.join(name)).map_err(|e| format!("{name}: {e}"));
        let output = Output {
            status,
            stdout: read("stdout")?,
            stderr: read("stderr")?,
        };
        let directory = self.root.join("target/ripr/reports");
        let json = fs::read(directory.join("kani.json")).map_err(|e| {
            format!(
                "canonical native JSON absent: {e}; stderr={}; driver={}",
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&read("driver.stderr").unwrap_or_default())
            )
        })?;
        if let Some(directory) = std::env::var_os("RIPR_NATIVE_SCHEMA_SAMPLES") {
            static NEXT_SAMPLE: AtomicU64 = AtomicU64::new(0);
            let directory = PathBuf::from(directory);
            fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
            let label = self
                .root
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("fixture name is not UTF-8")?;
            fs::write(
                directory.join(format!(
                    "{label}-{}.json",
                    NEXT_SAMPLE.fetch_add(1, Ordering::Relaxed)
                )),
                &json,
            )
            .map_err(|e| e.to_string())?;
        }
        let run: formal::Run = formal::parse_json(&json)?;
        formal::audit(&[], Some(&run))?;
        let report = fs::read_to_string(directory.join("kani.md")).map_err(|e| e.to_string())?;
        let calls = fs::read_to_string(self.root.join("calls"))
            .map_err(|e| e.to_string())?
            .lines()
            .map(str::to_string)
            .collect();
        self.retain_after_error.set(false);
        Ok(Observation {
            output,
            report,
            run,
            calls,
            elapsed: started.elapsed(),
            watchdog_timed_out,
            owned_descendants_observed: descendants.len(),
            live_descendants_before_driver_cleanup: live,
        })
    }

    fn environment(&self, command: &mut Command, probe_mode: &str) {
        // Build-only flags belong to this test's compiler, not its child
        // provider. Leave the production profile validator fully enabled.
        for (name, _) in std::env::vars_os() {
            let key = name.to_string_lossy();
            if key.starts_with("KANI_")
                || key.starts_with("CARGO_PROFILE_")
                || key.starts_with("CARGO_TARGET_")
                || key.starts_with("CARGO_UNSTABLE_")
                || (key.starts_with("CARGO_")
                    && (key.contains("RUSTFLAGS") || key.contains("RUSTC")))
                || matches!(
                    key.as_ref(),
                    "RUSTFLAGS"
                        | "RUSTC"
                        | "RUSTC_WRAPPER"
                        | "RUSTC_WORKSPACE_WRAPPER"
                        | "RUSTDOCFLAGS"
                        | "RUSTC_BOOTSTRAP"
                        | "CARGO_BUILD_TARGET"
                        | "CARGO_ENCODED_RUSTDOCFLAGS"
                        | "GITHUB_RUN_ID"
                        | "GITHUB_RUN_ATTEMPT"
                        | "GITHUB_JOB"
                )
            {
                command.env_remove(name);
            }
        }
        command
            .env(
                "PATH",
                self.root.join(if probe_mode == "missing" {
                    "missing"
                } else {
                    "bin"
                }),
            )
            .env("KANI_HOME", self.root.join("kani-home"))
            .env("CARGO_HOME", self.root.join("cargo-home"))
            .env("RUSTUP_HOME", self.root.join("rustup-home"));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.retain_after_error.get() || std::thread::panicking() {
            eprintln!("retained native fixture: {}", self.root.display());
        } else {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

fn same_process_is_live(before: &custody::Member) -> Result<bool, String> {
    Ok(custody::read_member(before.pid)?.is_some_and(|after| {
        after.start == before.start && after.state != "Z" && after.state != "X"
    }))
}

pub fn child_selector(name: &str) -> Result<&str, String> {
    name.split_once("::")
        .map(|(_, selector)| selector)
        .ok_or("missing test crate prefix".into())
}

pub fn watchdog_child() -> Result<(), String> {
    if std::env::var_os(CHILD).is_none() {
        return Ok(());
    }
    custody::enter_test_session()?;
    let root = PathBuf::from(std::env::var_os("RIPR_NATIVE_ROOT").ok_or("fixture root absent")?);
    let args: Vec<String> =
        serde_json::from_str(&std::env::var("RIPR_NATIVE_ARGS").map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    #[cfg(feature = "automation")]
    let mut command = {
        let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
        command.arg("kani");
        command
    };
    #[cfg(not(feature = "automation"))]
    let mut command = Command::new(env!("CARGO_BIN_EXE_formal-kani"));
    command
        .args(args)
        .stdout(File::create(root.join("stdout")).map_err(|e| e.to_string())?)
        .stderr(File::create(root.join("stderr")).map_err(|e| e.to_string())?);
    Err(format!(
        "could not execute the candidate native entry: {}",
        command.exec()
    ))
}

pub fn lane_args(seconds: u64, allow_empty: bool) -> Vec<String> {
    let mut args = vec![
        "--timeout-secs".into(),
        seconds.to_string(),
        "--harness-timeout-secs".into(),
        "1".into(),
    ];
    if allow_empty {
        args.push("--allow-empty".into());
    }
    args
}

pub fn successful_block(qualified: &str) -> String {
    format!(
        "Checking harness {qualified}...\nCheck 1: {qualified}.cover.1\n\t - Status: SATISFIED\n\t - Description: \"synthetic cover\"\n\t - Location: fixture.rs:1:1 in function {qualified}\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.1s\n"
    )
}

pub fn expected_headers() -> &'static str {
    TOOL_HEADERS
}
