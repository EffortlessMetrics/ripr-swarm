//! Real stdio cancellation controls for #9895/#9979. This binary has one
//! Linux test so its subreaper owns only this fixture's descendants. Signals
//! use pinned pidfds; no cleanup signals a remembered numeric PID.
#![cfg(all(target_os = "linux", feature = "lang-rust"))]

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fd::{AsFd, OwnedFd};
use rustix::process::{
    Pid, PidfdFlags, Signal, WaitId, WaitIdOptions, pidfd_open, pidfd_send_signal, waitid,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const BUDGET: Duration = Duration::from_secs(20);
const REAP_BUDGET: Duration = Duration::from_secs(5);
const LOG_LIMIT: u64 = 1024 * 1024;

fn pause() {
    std::thread::sleep(Duration::from_millis(10));
}

struct PinnedProcess {
    pid: u32,
    descriptor: OwnedFd,
    label: &'static str,
    reaped: bool,
}

impl PinnedProcess {
    fn open(pid: u32, label: &'static str) -> Result<Self, String> {
        let raw_pid = pid;
        let pid = i32::try_from(pid)
            .ok()
            .and_then(Pid::from_raw)
            .ok_or_else(|| format!("invalid {label} fixture PID"))?;
        let descriptor = pidfd_open(pid, PidfdFlags::empty())
            .map_err(|error| format!("pin {label}: {error}"))?;
        Ok(Self {
            pid: raw_pid,
            descriptor,
            label,
            reaped: false,
        })
    }

    fn events(&self) -> Result<PollFlags, String> {
        let mut descriptors = [PollFd::new(&self.descriptor, PollFlags::IN)];
        let zero = Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        poll(&mut descriptors, Some(&zero))
            .map_err(|error| format!("poll {}: {error}", self.label))?;
        Ok(descriptors[0].revents())
    }

    fn signal(&self, signal: Signal) -> Result<(), String> {
        pidfd_send_signal(&self.descriptor, signal)
            .map_err(|error| format!("signal {}: {error}", self.label))
    }

    fn require_live(&self) -> Result<(), String> {
        if self.events()?.intersects(PollFlags::IN | PollFlags::HUP) {
            return Err(format!(
                "{} was already terminal before the stimulus",
                self.label
            ));
        }
        Ok(())
    }

    fn wait_terminal(&self, reaped: bool) -> Result<(), String> {
        let deadline = Instant::now() + REAP_BUDGET;
        loop {
            let events = self.events()?;
            // Linux pidfds report IN on exit and HUP when the process has
            // also been reaped. This observes the product's primary reap.
            if events.contains(if reaped {
                PollFlags::HUP
            } else {
                PollFlags::IN
            }) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "{} was not {} before harness cleanup",
                    self.label,
                    if reaped { "reaped" } else { "terminal" }
                ));
            }
            pause();
        }
    }

    fn reap_adopted(&mut self) -> Result<Option<i32>, String> {
        let deadline = Instant::now() + REAP_BUDGET;
        loop {
            match waitid(
                WaitId::PidFd(self.descriptor.as_fd()),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG,
            ) {
                Ok(Some(status)) => {
                    self.reaped = true;
                    return Ok(status.terminating_signal());
                }
                Ok(None) | Err(rustix::io::Errno::CHILD) if Instant::now() < deadline => pause(),
                Ok(None) => return Err(format!("{} did not exit for reaping", self.label)),
                Err(error) => return Err(format!("reap {}: {error}", self.label)),
            }
        }
    }

    fn stop(&self) -> Result<(), String> {
        if !self.events()?.intersects(PollFlags::IN | PollFlags::HUP) {
            self.signal(Signal::KILL)?;
        }
        Ok(())
    }
}

struct OwnedChild {
    child: Child,
    pinned: PinnedProcess,
}

impl OwnedChild {
    fn spawn(mut command: Command, label: &'static str) -> Result<Self, String> {
        let mut child = command
            .spawn()
            .map_err(|error| format!("spawn {label}: {error}"))?;
        match PinnedProcess::open(child.id(), label) {
            Ok(pinned) => Ok(Self { child, pinned }),
            Err(error) => {
                let kill = child.kill();
                let reap = bounded_wait(&mut child, REAP_BUDGET);
                Err(format!("{error}; direct-child cleanup: {kill:?}, {reap:?}"))
            }
        }
    }

    fn stop(&mut self) -> Result<(), String> {
        if self.pinned.reaped {
            return Ok(());
        }
        self.pinned.stop()?;
        let status = bounded_wait(&mut self.child, REAP_BUDGET)?;
        if status.is_none() {
            return Err(format!(
                "{} cleanup did not reap the child",
                self.pinned.label
            ));
        }
        self.pinned.reaped = true;
        Ok(())
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("owned fixture cleanup failed: {error}");
        }
    }
}

fn bounded_wait(child: &mut Child, budget: Duration) -> Result<Option<ExitStatus>, String> {
    let deadline = Instant::now() + budget;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        pause();
    }
}

/// Enumerate only children of this test process, across its own threads.
/// The one-test binary owns every process it creates; this is not a system
/// process scan, and a numeric entry grants no signalling authority by itself.
fn own_children() -> Result<BTreeSet<u32>, String> {
    let mut children = BTreeSet::new();
    for entry in std::fs::read_dir("/proc/self/task")
        .map_err(|error| format!("enumerate own tasks: {error}"))?
    {
        let path = entry
            .map_err(|error| format!("read own task entry: {error}"))?
            .path()
            .join("children");
        let file = match File::open(&path) {
            Ok(file) => file,
            // A reader thread may finish during enumeration. Its task entry
            // disappearing is not a reason to inspect a different process.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("read own child list: {error}")),
        };
        let mut bytes = Vec::new();
        file.take(64 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("read own child list: {error}"))?;
        if bytes.len() > 64 * 1024 {
            return Err("own child list exceeded its fixture bound".into());
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| format!("invalid own child list: {error}"))?;
        for value in text.split_whitespace() {
            let pid = value
                .parse::<u32>()
                .map_err(|error| format!("invalid own child PID: {error}"))?;
            children.insert(pid);
        }
    }
    Ok(children)
}

/// Best-effort `comm` plus truncated argument text for a failure receipt. A
/// reparented zombie still exposes `comm`; `cmdline` is typically empty by
/// then. Reads are bounded and their absence never blocks cleanup.
fn describe_unregistered_child(process: &PinnedProcess) -> String {
    let comm = std::fs::read_to_string(format!("/proc/{}/comm", process.pid))
        .map(|value| value.trim().to_owned())
        .unwrap_or_else(|_| "unknown".to_owned());
    let mut arguments = Vec::new();
    if let Ok(bytes) = std::fs::read(format!("/proc/{}/cmdline", process.pid)) {
        arguments = bytes[..bytes.len().min(512)]
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect();
    }
    if arguments.is_empty() {
        comm
    } else {
        format!("{comm} {}", arguments.join(" "))
    }
}

/// Recover a fixture descendant whose normal marker/pidfd handshake failed.
/// Known handles have already been stopped and their explicit waits finished.
/// Before signalling, PIDFD + successful non-reaping waitid prove the target
/// remains our child; an ECHILD result never permits a signal. The shared
/// deadline covers all rounds, including adoption after a stopped ancestor.
/// Discovering any child here always fails the test, even when cleanup works.
fn settle_unregistered_children() -> Result<(), String> {
    let deadline = Instant::now() + REAP_BUDGET;
    let mut receipts = Vec::new();
    loop {
        let children = own_children()?;
        if children.is_empty() {
            return if receipts.is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "unregistered fixture children required emergency cleanup: {}",
                    receipts.join("; ")
                ))
            };
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "unregistered fixture cleanup exceeded its deadline; retained children {children:?}; receipts: {}",
                receipts.join("; ")
            ));
        }
        for pid in children {
            let process = match PinnedProcess::open(pid, "unregistered fixture child") {
                Ok(process) => process,
                Err(error) => {
                    // No raw-PID fallback: retain the pin failure and retry
                    // only through a fresh own-child enumeration this round.
                    receipts.push(format!("child {pid}: {error}; no signal sent"));
                    continue;
                }
            };
            // Diagnostic identity for the failure receipt. The pidfd pins the
            // process, so this describes the discovered child and cannot race
            // a PID reuse; it never grants or gates signal authority.
            let identity = describe_unregistered_child(&process);
            let observed = match waitid(
                WaitId::PidFd(process.descriptor.as_fd()),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(observed) => observed,
                Err(error) => {
                    receipts.push(format!(
                        "child {pid} ({identity}): custody query {error}; no signal sent"
                    ));
                    continue;
                }
            };
            if observed.is_none()
                && let Err(error) = process.signal(Signal::KILL)
            {
                receipts.push(format!("child {pid} ({identity}): {error}"));
            }
            loop {
                match waitid(
                    WaitId::PidFd(process.descriptor.as_fd()),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG,
                ) {
                    Ok(Some(status)) => {
                        receipts.push(format!(
                            "child {pid} ({identity}) reaped: exit={:?}, signal={:?}",
                            status.exit_status(),
                            status.terminating_signal()
                        ));
                        break;
                    }
                    Ok(None) if Instant::now() < deadline => pause(),
                    Ok(None) => {
                        return Err(format!(
                            "child {pid} remains unreaped at the cleanup deadline; receipts: {}",
                            receipts.join("; ")
                        ));
                    }
                    Err(error) => {
                        receipts.push(format!("child {pid} reap failed: {error}"));
                        break;
                    }
                }
            }
        }
        // A terminated ancestor may have just released another child to this
        // subreaper. Re-enumerate this process rather than guessing its PID.
        pause();
    }
}

struct Session {
    process: OwnedChild,
    stdin: Option<ChildStdin>,
    output: File,
    stderr: PathBuf,
    pending: Vec<u8>,
    read_bytes: u64,
}

impl Session {
    fn spawn(root: &Path, control: &Path, logs: &Path) -> Result<Self, String> {
        let stdout_path = logs.join("stdout.jsonl");
        let stderr = logs.join("stderr.log");
        let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
        command
            .args(["mcp", "--stdio", "--root"])
            .arg(root)
            .env("RIPR_INTERRUPT_CONTROL", control)
            .env("PATH", fixture_path(control)?)
            .stdin(Stdio::piped())
            .stdout(File::create(&stdout_path).map_err(|error| error.to_string())?)
            .stderr(File::create(&stderr).map_err(|error| error.to_string())?);
        let mut process = OwnedChild::spawn(command, "MCP server")?;
        let stdin = process.child.stdin.take().ok_or("server stdin missing")?;
        let output = File::open(stdout_path).map_err(|error| error.to_string())?;
        let mut session = Self {
            process,
            stdin: Some(stdin),
            output,
            stderr,
            pending: Vec::new(),
            read_bytes: 0,
        };
        let init = session.exchange(
            "init",
            "initialize",
            json!({
                "protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": { "name": "ripr-interrupt-control", "version": "1" }
            }),
        )?;
        if init.get("result").is_none() || init.get("error").is_some() {
            return Err(format!("initialize failed: {init}"));
        }
        session.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
        Ok(session)
    }

    fn send(&mut self, request: Value) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        // One small request is outstanding at a time, well below pipe capacity.
        if bytes.len() > 2048 {
            return Err("fixture request exceeds its input bound".into());
        }
        self.stdin
            .as_mut()
            .ok_or("server stdin closed")?
            .write_all(&bytes)
            .map_err(|error| error.to_string())
    }

    fn receive(&mut self, id: &str) -> Result<Value, String> {
        let deadline = Instant::now() + BUDGET;
        loop {
            if self
                .output
                .metadata()
                .map_err(|error| error.to_string())?
                .len()
                > LOG_LIMIT
                || std::fs::metadata(&self.stderr)
                    .map_err(|error| error.to_string())?
                    .len()
                    > LOG_LIMIT
            {
                return Err("server output exceeded fixture log bound".into());
            }
            self.output
                .seek(SeekFrom::Start(self.read_bytes))
                .map_err(|error| error.to_string())?;
            let mut chunk = Vec::new();
            Read::by_ref(&mut self.output)
                .take(LOG_LIMIT + 1)
                .read_to_end(&mut chunk)
                .map_err(|error| error.to_string())?;
            self.read_bytes += chunk.len() as u64;
            self.pending.extend(chunk);
            while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
                let line: Vec<_> = self.pending.drain(..=end).collect();
                let reply: Value = serde_json::from_slice(&line)
                    .map_err(|error| format!("invalid server JSON: {error}"))?;
                if reply.get("id").and_then(Value::as_str) == Some(id) {
                    return Ok(reply);
                }
                if reply.get("id").is_some() {
                    return Err(format!("unexpected response while awaiting {id}: {reply}"));
                }
            }
            if let Some(status) = self
                .process
                .child
                .try_wait()
                .map_err(|error| error.to_string())?
            {
                return Err(format!("server exited {status} before {id}"));
            }
            if Instant::now() >= deadline {
                return Err(format!("bounded response deadline expired for {id}"));
            }
            pause();
        }
    }

    fn exchange(&mut self, id: &str, method: &str, params: Value) -> Result<Value, String> {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        self.receive(id)
    }

    fn begin_refresh(&mut self, id: &str, arguments: Value) -> Result<(), String> {
        self.send(
            json!({"jsonrpc":"2.0","id":id,"method":"tools/call", "params":{
                "name":"ripr_refresh","arguments":arguments
            }}),
        )
    }

    fn refresh(&mut self, id: &str, arguments: Value) -> Result<Value, String> {
        self.begin_refresh(id, arguments)?;
        completed_document(self.receive(id)?)
    }

    fn ping(&mut self, id: &str) -> Result<(), String> {
        let response = self.exchange(id, "ping", json!({}))?;
        if response.get("result") != Some(&json!({})) {
            return Err(format!("ping failed: {response}"));
        }
        Ok(())
    }

    fn close_eof(&mut self) -> Result<(), String> {
        self.stdin = None;
        let status = bounded_wait(&mut self.process.child, REAP_BUDGET)?
            .ok_or("server did not settle on stdin EOF")?;
        if !status.success() {
            return Err(format!("server EOF exit was {status}"));
        }
        self.process.pinned.reaped = true;
        Ok(())
    }
}

fn completed_document(reply: Value) -> Result<Value, String> {
    // A failed attempt (a cancelled analysis included) keeps its document but
    // is `isError: true`; every other refresh is `isError: false` (#11571).
    let failed = reply.pointer("/result/structuredContent/attempt/state") == Some(&json!("failed"));
    if reply.pointer("/result/isError") != Some(&json!(failed)) {
        return Err(format!("refresh lost completed-document contract: {reply}"));
    }
    reply
        .pointer("/result/structuredContent")
        .cloned()
        .ok_or_else(|| format!("refresh lost structured content: {reply}"))
}

fn successful_snapshot(document: &Value) -> Result<String, String> {
    if document["attempt"]["state"] != "completed" {
        return Err(format!("refresh did not complete: {document}"));
    }
    document
        .pointer("/snapshot/snapshot_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("missing completed snapshot: {document}"))
}

fn cancelled_snapshot(document: &Value, previous: &str) -> Result<(), String> {
    if document["schema_version"] != "ripr-mcp-refresh-v1"
        || document["attempt"]["state"] != "failed"
        || document["attempt"]["failure"]["code"] != "analysis_failed"
        || document
            .pointer("/last_known_good/snapshot_id")
            .and_then(Value::as_str)
            != Some(previous)
        || document
            .pointer("/snapshot/snapshot_id")
            .and_then(Value::as_str)
            != Some(previous)
    {
        return Err(format!(
            "cancellation lost typed settlement or last-good snapshot: {document}"
        ));
    }
    Ok(())
}

fn fixture_path(control: &Path) -> Result<std::ffi::OsString, String> {
    let path = std::env::var_os("PATH").ok_or("PATH unavailable for fixture")?;
    std::env::join_paths(std::iter::once(control.to_path_buf()).chain(std::env::split_paths(&path)))
        .map_err(|error| error.to_string())
}

/// This test cannot import `common::fixture_git`: that module includes its
/// own native tests, allowing unrelated test children into the subreaper.
/// These fixed, local setup commands instead share the bounded owner above,
/// use regular-file output, and never retry a possibly completed commit.
fn fixture_git_ok(root: &Path, args: &[&str]) -> Result<(), String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT_LOG: AtomicUsize = AtomicUsize::new(0);
    let path = root.parent().ok_or("fixture parent missing")?.join(format!(
        "fixture-git-{}.log",
        NEXT_LOG.fetch_add(1, Ordering::Relaxed)
    ));
    let log = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| error.to_string())?;
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "core.fsmonitor=false",
            // This test is its descendant tree's only subreaper, so git must
            // not detach background helpers (observed: one clean-exit `git`
            // zombie per commit reparented to this process). Every helper is
            // suppressed here so the fixture owns each git it starts.
            "-c",
            "gc.auto=0",
            "-c",
            "gc.autoDetach=false",
            "-c",
            "maintenance.auto=false",
            "-c",
            "core.commitGraph=false",
        ])
        .args(args)
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|error| error.to_string())?)
        .stderr(log);
    let mut child = OwnedChild::spawn(command, "fixture git")?;
    let status = bounded_wait(&mut child.child, Duration::from_secs(30))?
        .ok_or_else(|| format!("fixture git {args:?} exceeded its deadline; no retry"))?;
    child.pinned.reaped = true;
    let mut output = String::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(LOG_LIMIT + 1)
        .read_to_string(&mut output)
        .map_err(|error| error.to_string())?;
    if !status.success() || output.len() as u64 > LOG_LIMIT {
        return Err(format!("fixture git {args:?}: {status}; {output}"));
    }
    Ok(())
}

fn fixture(root: &Path, control: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(control).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"interrupt-fixture\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn boundary(n: i32) -> bool { n >= 10 }\n",
    )
    .map_err(|error| error.to_string())?;
    fixture_git_ok(root, &["-c", "init.defaultBranch=main", "init", "-q"])?;
    fixture_git_ok(root, &["add", "."])?;
    fixture_git_ok(
        root,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@ripr.invalid",
            "commit",
            "-qm",
            "baseline",
        ],
    )?;
    fixture_git_ok(root, &["checkout", "-qb", "change"])?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn boundary(n: i32) -> bool { n > 10 }\n",
    )
    .map_err(|error| error.to_string())?;
    fixture_git_ok(root, &["add", "."])?;
    fixture_git_ok(
        root,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@ripr.invalid",
            "commit",
            "-qm",
            "change",
        ],
    )?;
    let path = std::env::var_os("PATH").ok_or("PATH unavailable")?;
    let git = std::env::split_paths(&path)
        .map(|directory| directory.join("git"))
        .find(|path| path.is_file())
        .ok_or("git fixture executable unavailable")?;
    let git = git
        .to_str()
        .filter(|path| !path.contains('\''))
        .ok_or("git executable cannot be quoted in fixture script")?;
    // The fixture blocks in shell builtins while handles are pinned. There
    // are no polling helper processes to become unrecorded descendants.
    let script = format!(
        r#"#!/bin/sh
for arg in "$@"; do
  if [ "$arg" = diff ] && [ -f "$RIPR_INTERRUPT_CONTROL/mode" ] && mkdir "$RIPR_INTERRUPT_CONTROL/claimed" 2>/dev/null; then
    trap '' INT TERM
    printf '%s\n' "$$" > "$RIPR_INTERRUPT_CONTROL/primary"
    sh -c 'trap "" INT TERM; printf "%s\n" "$$" > "$RIPR_INTERRUPT_CONTROL/descendant"; exec sleep 600' &
    IFS= read -r release < "$RIPR_INTERRUPT_CONTROL/release"
    IFS= read -r mode < "$RIPR_INTERRUPT_CONTROL/mode"
    if [ "$mode" = early ]; then exec '{git}' "$@"; fi
    exec sleep 600
  fi
done
exec '{git}' "$@"
"#
    );
    let shim = control.join("git");
    std::fs::write(&shim, script).map_err(|error| error.to_string())?;
    std::fs::set_permissions(shim, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())
}

fn arm(control: &Path, mode: &str) -> Result<(), String> {
    for path in ["primary", "descendant", "release", "mode"] {
        match std::fs::remove_file(control.join(path)) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.to_string()),
        }
    }
    match std::fs::remove_dir(control.join("claimed")) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.to_string()),
    }
    nix::unistd::mkfifo(
        &control.join("release"),
        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(control.join("mode"), format!("{mode}\n")).map_err(|error| error.to_string())
}

fn pin_marker(
    control: &Path,
    name: &'static str,
    parent: &PinnedProcess,
) -> Result<PinnedProcess, String> {
    let deadline = Instant::now() + BUDGET;
    loop {
        if Instant::now() >= deadline {
            return Err(format!("fixture {name} did not acknowledge its live gate"));
        }
        parent.require_live()?;
        if let Ok(text) = std::fs::read_to_string(control.join(name))
            && let Ok(pid) = text.trim().parse::<u32>()
        {
            let pinned = PinnedProcess::open(pid, name)?;
            // A pidfd pins identity only after open. A stale marker cannot
            // authorize an unrelated process that reused the number before
            // that open: require lineage to the live pinned fixture parent.
            let file = File::open(format!("/proc/{pid}/status"))
                .map_err(|error| format!("read {name} parent identity: {error}"))?;
            let mut status = String::new();
            file.take(64 * 1024 + 1)
                .read_to_string(&mut status)
                .map_err(|error| error.to_string())?;
            if status.len() > 64 * 1024 {
                return Err("fixture status exceeded its bound".into());
            }
            let observed_parent = status
                .lines()
                .find_map(|line| line.strip_prefix("PPid:"))
                .and_then(|value| value.trim().parse::<u32>().ok());
            if observed_parent != Some(parent.pid) {
                return Err(format!("fixture {name} marker has no owned parent lineage"));
            }
            // If either process died while /proc was read, its old handle
            // refuses even if that numeric /proc entry has since been reused.
            pinned.require_live()?;
            parent.require_live()?;
            if Instant::now() >= deadline {
                return Err("fixture identity acquisition exceeded its deadline".into());
            }
            return Ok(pinned);
        }
        pause();
    }
}

fn release_primary(control: &Path) -> Result<(), String> {
    let deadline = Instant::now() + BUDGET;
    loop {
        match OpenOptions::new()
            .write(true)
            .custom_flags(nix::fcntl::OFlag::O_NONBLOCK.bits())
            .open(control.join("release"))
        {
            Ok(mut writer) => {
                return writer
                    .write_all(b"continue\n")
                    .map_err(|error| error.to_string());
            }
            Err(error)
                if error.raw_os_error() == Some(nix::errno::Errno::ENXIO as i32)
                    && Instant::now() < deadline =>
            {
                pause()
            }
            Err(error) => return Err(format!("release fixture primary: {error}")),
        }
    }
}

struct FixtureTree {
    primary: PinnedProcess,
    descendant: Option<PinnedProcess>,
}

impl FixtureTree {
    fn require_product_settlement(&mut self) -> Result<(), String> {
        self.primary.wait_terminal(true)?;
        let descendant = self
            .descendant
            .as_mut()
            .ok_or("descendant custody missing")?;
        descendant.wait_terminal(false)?;
        if descendant.reap_adopted()? != Some(Signal::KILL.as_raw()) {
            return Err("descendant was not terminated by product tree settlement".into());
        }
        self.primary.reaped = true;
        Ok(())
    }
}

struct Harness {
    root: PathBuf,
    control: PathBuf,
    session: Option<Session>,
    sentinel: Option<OwnedChild>,
    trees: Vec<FixtureTree>,
    /// Scenario-keyed live-child snapshots and retained server logs, so a
    /// cleanup-contract failure can name where a stray child appeared.
    trace: Vec<String>,
    stderr_paths: Vec<PathBuf>,
}

impl Harness {
    /// Snapshot the live direct-child set (pid + comm) into the scenario
    /// trace. Observation only: it never signals or reaps anything, and a
    /// reparented zombie is still a child, so it appears here too.
    fn trace_children(trace: &mut Vec<String>, label: &str) {
        let describe = own_children()
            .map(|children| {
                let named: Vec<String> = children
                    .into_iter()
                    .map(|pid| {
                        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                            .map(|value| value.trim().to_owned())
                            .unwrap_or_else(|_| "unknown".to_owned());
                        format!("{pid}({comm})")
                    })
                    .collect();
                if named.is_empty() {
                    "none".to_owned()
                } else {
                    named.join(",")
                }
            })
            .unwrap_or_else(|error| format!("enumerate failed: {error}"));
        trace.push(format!("{label}: {describe}"));
    }

    fn run(&mut self) -> Result<(), String> {
        let mut sentinel_command = Command::new("sleep");
        sentinel_command
            .arg("600")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        self.sentinel = Some(OwnedChild::spawn(sentinel_command, "unrelated sentinel")?);
        self.stderr_paths.push(self.root.join("stderr.log"));
        self.session = Some(Session::spawn(
            &self.root.join("repo"),
            &self.control,
            &self.root,
        )?);
        Self::trace_children(&mut self.trace, "after-spawn");
        let session = self.session.as_mut().ok_or("session missing")?;
        let baseline = successful_snapshot(&session.refresh("baseline", json!({}))?)?;
        Self::trace_children(&mut self.trace, "after-baseline");
        for (name, signal) in [("interrupt", Signal::INT), ("terminate", Signal::TERM)] {
            arm(&self.control, "block")?;
            session.begin_refresh(name, json!({}))?;
            self.trees.push(FixtureTree {
                primary: pin_marker(&self.control, "primary", &session.process.pinned)?,
                descendant: None,
            });
            let tree = self.trees.last_mut().ok_or("tree missing")?;
            // The primary descriptor is retained before the descendant pin,
            // including every error return during the second handshake.
            tree.descendant = Some(pin_marker(&self.control, "descendant", &tree.primary)?);
            session.process.pinned.signal(signal)?;
            let document = completed_document(session.receive(name)?)?;
            cancelled_snapshot(&document, &baseline)?;
            tree.require_product_settlement()?;
            self.sentinel
                .as_ref()
                .ok_or("sentinel missing")?
                .pinned
                .require_live()?;
            session.ping(&format!("{name}-ping"))?;
            let status = session.exchange(
                &format!("{name}-status"),
                "tools/call",
                json!({
                    "name":"ripr_workspace_status", "arguments":{}
                }),
            )?;
            if status.get("error").is_some()
                || status.pointer("/result/structuredContent").is_none()
                || status.pointer("/result/isError") == Some(&json!(true))
            {
                return Err(format!("workspace status failed after signal: {status}"));
            }
            std::fs::remove_file(self.control.join("mode")).map_err(|error| error.to_string())?;
            for (index, arguments) in [json!({}), json!({}), json!({"base":"main"})]
                .into_iter()
                .enumerate()
            {
                successful_snapshot(
                    &session.refresh(&format!("{name}-recovery-{index}"), arguments)?,
                )?;
            }
            Self::trace_children(&mut self.trace, &format!("after-{name}-recoveries"));
        }
        arm(&self.control, "early")?;
        session.begin_refresh("early-primary", json!({}))?;
        self.trees.push(FixtureTree {
            primary: pin_marker(&self.control, "primary", &session.process.pinned)?,
            descendant: None,
        });
        let tree = self.trees.last_mut().ok_or("early tree missing")?;
        tree.descendant = Some(pin_marker(&self.control, "descendant", &tree.primary)?);
        release_primary(&self.control)?;
        successful_snapshot(&completed_document(session.receive("early-primary")?)?)?;
        tree.require_product_settlement()?;
        std::fs::remove_file(self.control.join("mode")).map_err(|error| error.to_string())?;
        let previous = successful_snapshot(&session.refresh("before-idle", json!({}))?)?;
        session.process.pinned.signal(Signal::TERM)?;
        session.ping("idle-signal-observed")?;
        cancelled_snapshot(&session.refresh("consume-idle", json!({}))?, &previous)?;
        successful_snapshot(&session.refresh("after-idle", json!({}))?)?;
        // A configuration refusal is also a terminal attempt. Its original
        // diagnostic wins, but it must consume the pending idle signal so
        // fixing the configuration restores this same session immediately.
        std::fs::write(self.root.join("repo/ripr.toml"), "invalid = [\n")
            .map_err(|error| error.to_string())?;
        session.process.pinned.signal(Signal::INT)?;
        session.ping("config-idle-observed")?;
        let invalid = session.refresh("invalid-config", json!({}))?;
        if invalid["attempt"]["state"] != "failed"
            || invalid["attempt"]["failure"]["code"] != "config_invalid"
            || invalid
                .pointer("/last_known_good/snapshot_id")
                .and_then(Value::as_str)
                != Some(previous.as_str())
        {
            return Err(format!(
                "config refusal lost its typed terminal state: {invalid}"
            ));
        }
        std::fs::remove_file(self.root.join("repo/ripr.toml"))
            .map_err(|error| error.to_string())?;
        successful_snapshot(&session.refresh("config-recovered", json!({}))?)?;
        Self::trace_children(&mut self.trace, "after-config-recovered");
        self.sentinel
            .as_ref()
            .ok_or("sentinel missing")?
            .pinned
            .require_live()?;
        session.close_eof()?;
        Self::trace_children(&mut self.trace, "after-close-eof");

        // Repeated native escape is exercised only with no refresh children.
        // First-signal survival has an actual same-session ping acknowledgment.
        let repeated_logs = self.root.join("repeated");
        std::fs::create_dir(&repeated_logs).map_err(|error| error.to_string())?;
        self.stderr_paths.push(repeated_logs.join("stderr.log"));
        self.session = Some(Session::spawn(
            &self.root.join("repo"),
            &self.control,
            &repeated_logs,
        )?);
        let session = self.session.as_mut().ok_or("repeat session missing")?;
        session.process.pinned.signal(Signal::INT)?;
        session.ping("first-signal-session-live")?;
        Self::trace_children(&mut self.trace, "after-escape-ping");
        session.process.pinned.signal(Signal::TERM)?;
        let status = bounded_wait(&mut session.process.child, REAP_BUDGET)?
            .ok_or("repeated signal did not terminate the server")?;
        if status.signal() != Some(Signal::TERM.as_raw()) {
            return Err(format!(
                "repeated signal lost native SIGTERM status: {status}"
            ));
        }
        session.process.pinned.reaped = true;
        Self::trace_children(&mut self.trace, "after-escape");
        self.sentinel
            .as_ref()
            .ok_or("sentinel missing")?
            .pinned
            .require_live()?;
        Ok(())
    }

    fn cleanup(&mut self) -> Result<(), String> {
        let mut failures = Vec::new();
        for tree in &self.trees {
            for process in std::iter::once(&tree.primary).chain(tree.descendant.as_ref()) {
                if let Err(error) = process.stop() {
                    failures.push(error);
                }
            }
        }
        if let Some(session) = &mut self.session {
            session.stdin = None;
            if let Err(error) = session.process.stop() {
                failures.push(error);
            }
        }
        // Stopping the server releases any unreaped primary to this subreaper.
        for tree in &mut self.trees {
            for process in std::iter::once(&mut tree.primary).chain(tree.descendant.as_mut()) {
                if !process.reaped {
                    match process.events() {
                        Ok(events) if events.contains(PollFlags::HUP) => process.reaped = true,
                        _ => {
                            if let Err(error) = process.reap_adopted() {
                                failures.push(error);
                            }
                        }
                    }
                }
            }
        }
        if let Some(sentinel) = &mut self.sentinel
            && let Err(error) = sentinel.stop()
        {
            failures.push(error);
        }
        if let Err(error) = settle_unregistered_children() {
            failures.push(error);
        }
        // Emergency cleanup may have reaped an explicitly owned direct child
        // whose first stop returned an error. Retire its later Drop wait only
        // after kernel HUP establishes that reap; do not erase the error above.
        if let Some(session) = &mut self.session
            && session
                .process
                .pinned
                .events()
                .is_ok_and(|events| events.contains(PollFlags::HUP))
        {
            session.process.pinned.reaped = true;
        }
        if let Some(sentinel) = &mut self.sentinel
            && sentinel
                .pinned
                .events()
                .is_ok_and(|events| events.contains(PollFlags::HUP))
        {
            sentinel.pinned.reaped = true;
        }
        if failures.is_empty() {
            Ok(())
        } else {
            // Attach the scenario child trace and the retained server logs so
            // a stray-child failure names where it appeared, not just that it
            // existed. Tails are bounded; a missing log is reported as such.
            let mut context = Vec::new();
            for path in &self.stderr_paths {
                match std::fs::read(path) {
                    Ok(bytes) => {
                        let start = bytes.len().saturating_sub(2048);
                        context.push(format!(
                            "{} tail: {}",
                            path.display(),
                            String::from_utf8_lossy(&bytes[start..]).trim()
                        ));
                    }
                    Err(error) => context.push(format!("{} unreadable: {error}", path.display())),
                }
            }
            Err(format!(
                "{}; scenario child trace: {}; server logs: {}",
                failures.join("; "),
                self.trace.join(" | "),
                if context.is_empty() {
                    "none retained".to_owned()
                } else {
                    context.join(" | ")
                }
            ))
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            eprintln!("interrupt harness cleanup: {error}");
        }
    }
}

/// A signal must settle only its attempt, preserve the same session and its
/// last-good snapshot, and terminate/reap pipe descendants before cleanup.
#[test]
fn cooperative_interrupt_lifecycle() -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("ripr-mcp-interrupt-{}-{stamp}", std::process::id()));
    let control = root.join("control");
    let was_subreaper =
        nix::sys::prctl::get_child_subreaper().map_err(|error| error.to_string())?;
    nix::sys::prctl::set_child_subreaper(true).map_err(|error| error.to_string())?;
    let mut harness = Harness {
        root: root.clone(),
        control,
        session: None,
        sentinel: None,
        trees: Vec::new(),
        trace: Vec::new(),
        stderr_paths: Vec::new(),
    };
    let outcome = fixture(&root.join("repo"), &harness.control).and_then(|()| harness.run());
    let cleanup = harness.cleanup();
    let restored =
        nix::sys::prctl::set_child_subreaper(was_subreaper).map_err(|error| error.to_string());
    match (outcome, cleanup, restored) {
        (Ok(()), Ok(()), Ok(())) => {
            std::fs::remove_dir_all(root).map_err(|error| error.to_string())
        }
        (outcome, cleanup, restored) => Err(format!(
            "interrupt lifecycle: {outcome:?}; cleanup: {cleanup:?}; restore: {restored:?}; retained fixture {}",
            root.display()
        )),
    }
}
