//! Bounded stdio observer. Every child has an owned process group on Linux.
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    thread::JoinHandle,
    time::{Duration, Instant},
};

const FRAME_LIMIT: u64 = 8 * 1024 * 1024;
const SAMPLE_INTERVAL: Duration = Duration::from_millis(20);
const RSS_LIMIT: u64 = 1024 * 1024 * 1024;
const FREE_RESERVE: u64 = 3 * 1024 * 1024 * 1024;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Wire {
    Lsp,
    Mcp,
}
impl Wire {
    pub(super) fn name(self) -> &'static str {
        if self == Self::Lsp { "lsp" } else { "mcp" }
    }
}

pub(super) struct Session {
    child: Child,
    input: Option<SyncSender<Vec<u8>>>,
    writes: Receiver<Result<(), String>>,
    writer: Option<JoinHandle<()>>,
    inbox: Receiver<Result<Value, String>>,
    reader: Option<JoinHandle<()>>,
    pub(super) wire: Wire,
    next_id: u64,
    deadline: Instant,
    started: Instant,
    samples: Vec<Value>,
    peak: u64,
    base: String,
    finished: bool,
}
impl Session {
    pub(super) fn start(
        binary: &Path,
        root: &Path,
        cache: &Path,
        wire: Wire,
        deadline: Instant,
    ) -> Result<Self, String> {
        let mut command = command(binary);
        command
            .args([wire.name(), "--stdio"])
            .current_dir(root)
            .env("RIPR_CACHE_DIR", cache)
            .env("RIPR_MCP_TOOLS_LIST", "full");
        if wire == Wire::Mcp {
            command.arg("--root").arg(root);
        }
        Self::spawn(command, wire, deadline)
    }
    pub(super) fn control(binary: &Path, deadline: Instant) -> Result<Self, String> {
        let mut command = command(binary);
        command.arg("--retention-control-child");
        Self::spawn(command, Wire::Mcp, deadline)
    }
    fn spawn(mut command: Command, wire: Wire, deadline: Instant) -> Result<Self, String> {
        if !cfg!(target_os = "linux") {
            return Err("owned observer currently requires Linux".into());
        }
        budget(deadline)?;
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let started = Instant::now();
        let mut child = command
            .spawn()
            .map_err(|e| format!("spawn observer subject: {e}"))?;
        let Some(mut input) = child.stdin.take() else {
            terminate(&mut child)?;
            return Err("child stdin missing".into());
        };
        let Some(output) = child.stdout.take() else {
            terminate(&mut child)?;
            return Err("child stdout missing".into());
        };
        // Queue bounds memory while the child writes notifications. try_send
        // refuses overflow rather than leaving a reader blocked during cleanup.
        let (send, inbox) = mpsc::sync_channel(128);
        let reader = std::thread::spawn(move || {
            let mut output = BufReader::new(output);
            loop {
                let read = read_message(&mut output, wire);
                let ended = read.is_err();
                if send.try_send(read).is_err() || ended {
                    break;
                }
            }
        });
        let (requests, pending) = mpsc::sync_channel::<Vec<u8>>(1);
        let (written, writes) = mpsc::sync_channel(1);
        let writer = std::thread::spawn(move || {
            while let Ok(bytes) = pending.recv() {
                let result = input
                    .write_all(&bytes)
                    .and_then(|()| input.flush())
                    .map_err(|e| e.to_string());
                let failed = result.is_err();
                if written.try_send(result).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input: Some(requests),
            writes,
            writer: Some(writer),
            inbox,
            reader: Some(reader),
            wire,
            next_id: 0,
            deadline,
            started,
            samples: Vec::new(),
            peak: 0,
            base: "HEAD".into(),
            finished: false,
        })
    }
    pub(super) fn id(&self) -> u32 {
        self.child.id()
    }
    pub(super) fn begin_observation(&mut self) {
        self.started = Instant::now();
        self.samples.clear();
        self.peak = 0;
    }
    fn sample(&mut self) -> Result<u64, String> {
        budget(self.deadline)?;
        let text = fs::read_to_string(format!("/proc/{}/status", self.id()))
            .map_err(|e| format!("read subject VmRSS: {e}"))?;
        let rss = status_bytes(&text, "VmRSS:").ok_or("missing or malformed subject VmRSS")?;
        if rss > RSS_LIMIT {
            return Err("subject exceeded fixed 1 GiB RSS envelope".into());
        }
        self.peak = self.peak.max(rss);
        self.samples
            .push(json!({"elapsed_us":self.started.elapsed().as_micros(),"rss_bytes":rss}));
        Ok(rss)
    }
    pub(super) fn settle(&mut self, duration: Duration) -> Result<Value, String> {
        let request_elapsed_ms = self.started.elapsed().as_secs_f64() * 1000.0;
        let until = Instant::now() + duration;
        while Instant::now() < until {
            self.sample()?;
            std::thread::sleep(
                SAMPLE_INTERVAL.min(until.saturating_duration_since(Instant::now())),
            );
        }
        let retained = self.sample()?;
        Ok(
            json!({"request_elapsed_ms":request_elapsed_ms,"observation_elapsed_ms":self.started.elapsed().as_secs_f64()*1000.0,"sampled_peak_rss_bytes":self.peak,"retained_rss_bytes":retained,"sample_count":self.samples.len(),"samples":self.samples}),
        )
    }
    fn send(&mut self, message: &Value) -> Result<(), String> {
        budget(self.deadline)?;
        let bytes = serde_json::to_vec(message).map_err(|e| e.to_string())?;
        if bytes.len() > 8192 {
            return Err("instrument request exceeded 8 KiB write budget".into());
        }
        let mut frame = Vec::new();
        if self.wire == Wire::Lsp {
            write!(frame, "Content-Length: {}\r\n\r\n", bytes.len()).map_err(|e| e.to_string())?;
        }
        frame.extend_from_slice(&bytes);
        if self.wire == Wire::Mcp {
            frame.push(b'\n');
        }
        self.input
            .as_ref()
            .ok_or("child input already closed")?
            .try_send(frame)
            .map_err(|e| format!("subject input queue: {e}"))?;
        loop {
            self.sample()?;
            match self.writes.recv_timeout(
                SAMPLE_INTERVAL.min(self.deadline.saturating_duration_since(Instant::now())),
            ) {
                Ok(result) => return result,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("subject input writer closed".into());
                }
            }
        }
    }

    pub(super) fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.send(&json!({"jsonrpc":"2.0","method":method,"params":params}))
    }
    pub(super) fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        if method == "initialize" && self.wire == Wire::Lsp {
            self.base = params
                .pointer("/initializationOptions/baseRef")
                .and_then(Value::as_str)
                .ok_or("missing fixture base")?
                .to_string();
        }
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        loop {
            self.sample()?;
            match self.inbox.recv_timeout(
                SAMPLE_INTERVAL.min(self.deadline.saturating_duration_since(Instant::now())),
            ) {
                Ok(message) => {
                    let message = message?;
                    if let (Some(request_id), Some(request_method)) = (
                        message.get("id"),
                        message.get("method").and_then(Value::as_str),
                    ) {
                        let result = if request_method == "workspace/configuration" {
                            let count = message
                                .pointer("/params/items")
                                .and_then(Value::as_array)
                                .map_or(1, Vec::len);
                            json!(vec![
                                json!({"baseRef":self.base,"checkMode":"instant","diagnosticProfile":"full"});
                                count
                            ])
                        } else {
                            Value::Null
                        };
                        self.send(&json!({"jsonrpc":"2.0","id":request_id,"result":result}))?;
                    } else if message.get("id") == Some(&json!(id)) {
                        if message.get("error").is_some() {
                            return Err(format!("{method} RPC failure: {}", message["error"]));
                        }
                        return message
                            .get("result")
                            .cloned()
                            .ok_or_else(|| format!("{method} response lacks result"));
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(
                        "subject closed stdout or exceeded bounded notification queue".into(),
                    );
                }
            }
        }
    }
    pub(super) fn tool(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        let result = self.call("tools/call", json!({"name":name,"arguments":arguments}))?;
        if result
            .get("isError")
            .is_some_and(|value| value != &json!(false))
        {
            return Err(format!("{name} tool refused: {result}"));
        }
        result
            .get("structuredContent")
            .cloned()
            .filter(|value| value.is_object())
            .ok_or_else(|| format!("{name} lacks structuredContent"))
    }
    pub(super) fn finish(&mut self) -> Result<(), String> {
        if self.finished {
            return Ok(());
        }
        self.input.take();
        terminate(&mut self.child)?;
        self.finished = true;
        if let Some(writer) = self.writer.take() {
            writer
                .join()
                .map_err(|panic| format!("subject input writer failed: {panic:?}"))?;
        }
        if let Some(reader) = self.reader.take() {
            reader
                .join()
                .map_err(|panic| format!("subject output reader failed: {panic:?}"))?;
        }
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    // No inherited Git redirection or RIPR test knob may retarget the fixture.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") || key.to_string_lossy().starts_with("RIPR_") {
            command.env_remove(key);
        }
    }
    command
}
fn terminate(child: &mut Child) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let pid =
            rustix::process::Pid::from_raw(i32::try_from(child.id()).map_err(|e| e.to_string())?)
                .ok_or("invalid child PID")?;
        match rustix::process::kill_process_group(pid, rustix::process::Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(error) => return Err(format!("terminate owned group: {error}")),
        }
    }
    #[cfg(not(target_os = "linux"))]
    child.kill().map_err(|e| e.to_string())?;
    child.wait().map_err(|e| format!("reap subject: {e}"))?;
    Ok(())
}
fn budget(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        return Err("fixed session deadline expired".into());
    }
    let memory = fs::read_to_string("/proc/meminfo").map_err(|e| e.to_string())?;
    if status_bytes(&memory, "MemAvailable:").ok_or("available-memory observation missing")?
        < FREE_RESERVE
    {
        return Err("fixed 3 GiB available-memory reserve reached".into());
    }
    Ok(())
}
fn status_bytes(text: &str, name: &str) -> Option<u64> {
    let line = text.lines().find(|line| line.starts_with(name))?;
    let mut parts = line[name.len()..].split_whitespace();
    let number = parts.next()?.parse::<u64>().ok()?;
    if parts.next() != Some("kB") || parts.next().is_some() {
        return None;
    }
    number.checked_mul(1024)
}
fn read_message(reader: &mut impl BufRead, wire: Wire) -> Result<Value, String> {
    let mut bytes = Vec::new();
    if wire == Wire::Mcp {
        reader
            .take(FRAME_LIMIT + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.is_empty() || bytes.len() as u64 > FRAME_LIMIT || bytes.last() != Some(&b'\n') {
            return Err("MCP frame missing, unterminated or oversized".into());
        }
    } else {
        let mut length = None;
        let mut total = 0usize;
        loop {
            let mut line = String::new();
            let count = reader
                .take(4097)
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            total += count;
            if count == 0 || total > 4096 {
                return Err("LSP headers missing or oversized".into());
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
            if let Some((key, value)) = line.split_once(':')
                && key.eq_ignore_ascii_case("content-length")
            {
                if length.is_some() {
                    return Err("duplicate Content-Length".into());
                }
                length = Some(value.trim().parse::<usize>().map_err(|e| e.to_string())?);
            }
        }
        let count = length
            .filter(|count| *count as u64 <= FRAME_LIMIT)
            .ok_or("missing or oversized Content-Length")?;
        bytes.resize(count, 0);
        reader.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("non-JSON subject frame: {e}"))
}
pub(super) fn file_uri(path: &Path) -> String {
    let mut uri = "file://".to_string();
    for byte in path.to_string_lossy().bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}
pub(super) fn git(root: &Path, args: &[&str], deadline: Instant) -> Result<String, String> {
    let stdout = fs::File::create(root.join("git.stdout")).map_err(|e| e.to_string())?;
    let stderr = fs::File::create(root.join("git.stderr")).map_err(|e| e.to_string())?;
    let mut child = command("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|e| e.to_string())?;
    loop {
        if let Err(error) = budget(deadline) {
            terminate(&mut child)?;
            return Err(error);
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err(format!(
                    "fixture git failed: {}",
                    fs::read_to_string(root.join("git.stderr")).unwrap_or_default()
                ));
            }
            return fs::read_to_string(root.join("git.stdout")).map_err(|e| e.to_string());
        }
        std::thread::sleep(SAMPLE_INTERVAL);
    }
}
// This is an instrument control, not a product descendant-containment claim.
// The grandchild inherits both pipes and the primary's process group. Its own
// five-second ceiling also bounds the deliberately wrong primary-only cleanup.
pub(super) fn cleanup_primary() -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = command(executable);
    command.arg("--cleanup-control-grandchild");
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(i32::try_from(std::process::id()).map_err(|e| e.to_string())?);
    }
    let mut descendant = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("spawn owned control grandchild: {e}"))?;
    let request = read_message(&mut BufReader::new(std::io::stdin()), Wire::Mcp)?;
    if request["method"] != "ready" {
        return Err("unknown cleanup control request".into());
    }
    println!(
        "{}",
        json!({"jsonrpc":"2.0","id":request["id"],"result":{"descendant_pid":descendant.id()}})
    );
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    descendant.wait().map_err(|e| e.to_string())?;
    Ok(())
}
pub(super) fn cleanup_grandchild() {
    std::thread::sleep(Duration::from_secs(5));
}

#[derive(Clone, Copy)]
struct ProcessIdentity {
    group: u32,
    started: u64,
    state: char,
}
fn process_identity(pid: u32) -> Result<Option<ProcessIdentity>, String> {
    let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("read owned control process: {error}")),
    };
    // comm can contain spaces and parentheses; numeric fields follow its final ')'.
    let fields: Vec<_> = stat
        .rsplit_once(')')
        .ok_or("malformed control process stat")?
        .1
        .split_whitespace()
        .collect();
    Ok(Some(ProcessIdentity {
        state: fields
            .first()
            .and_then(|s| s.chars().next())
            .ok_or("missing state")?,
        group: fields
            .get(2)
            .ok_or("missing process group")?
            .parse::<u32>()
            .map_err(|e| e.to_string())?,
        started: fields
            .get(19)
            .ok_or("missing start time")?
            .parse::<u64>()
            .map_err(|e| e.to_string())?,
    }))
}
fn descendant_terminal(pid: u32, identity: ProcessIdentity) -> Result<bool, String> {
    match process_identity(pid)? {
        None => Ok(true),
        Some(current) if current.started != identity.started || current.group != identity.group => {
            Err("owned control descendant identity changed".into())
        }
        Some(current) => Ok(matches!(current.state, 'Z' | 'X')),
    }
}
fn require_cleanup(
    primary_reaped: bool,
    descendant_terminal: bool,
    pipes_finished: bool,
) -> Result<(), String> {
    if primary_reaped && descendant_terminal && pipes_finished {
        Ok(())
    } else {
        Err("owned cleanup control left a live descendant or unfinished pipe thread".into())
    }
}
pub(super) fn cleanup_control(binary: &Path, deadline: Instant) -> Result<Value, String> {
    let mut observations = Vec::new();
    for primary_only in [false, true] {
        let mut command = command(binary);
        command.arg("--cleanup-control-primary");
        let mut session = Session::spawn(command, Wire::Mcp, deadline)?;
        let response = session.call("ready", json!({}))?;
        let pid = response["descendant_pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .ok_or("missing control descendant PID")?;
        let identity = process_identity(pid)?.ok_or("control descendant absent before cleanup")?;
        if identity.group != session.id() || descendant_terminal(pid, identity)? {
            return Err("control descendant must be live in the owned primary group".into());
        }
        let wrong = if primary_only {
            // Deliberately wrong cleanup leaves the inherited stdout open. Do not
            // join its reader: first apply the same predicate, then repair using
            // the real Session::finish path. Drop also repairs on an early error.
            session.child.kill().map_err(|e| e.to_string())?;
            session.child.wait().map_err(|e| e.to_string())?;
            session.input.take();
            std::thread::sleep(Duration::from_millis(100));
            let primary_reaped = session
                .child
                .try_wait()
                .map_err(|e| e.to_string())?
                .is_some();
            let terminal = descendant_terminal(pid, identity)?;
            let pipes_finished = session.reader.as_ref().is_some_and(JoinHandle::is_finished)
                && session.writer.as_ref().is_some_and(JoinHandle::is_finished);
            if require_cleanup(primary_reaped, terminal, pipes_finished).is_ok()
                || terminal
                || session.reader.as_ref().is_none_or(JoinHandle::is_finished)
            {
                return Err("primary-only cleanup negative control was not detected".into());
            }
            json!({"status":"rejected_as_expected","primary_reaped":primary_reaped,"descendant_terminal":terminal,"pipe_threads_finished":pipes_finished})
        } else {
            Value::Null
        };
        let started = Instant::now();
        session.finish()?;
        let until = deadline.min(started + Duration::from_secs(2));
        let terminal = loop {
            let terminal = descendant_terminal(pid, identity)?;
            if terminal || Instant::now() >= until {
                break terminal;
            }
            budget(deadline)?;
            std::thread::sleep(SAMPLE_INTERVAL);
        };
        let primary_reaped = session
            .child
            .try_wait()
            .map_err(|e| e.to_string())?
            .is_some();
        let pipes_finished =
            session.finished && session.reader.is_none() && session.writer.is_none();
        require_cleanup(primary_reaped, terminal, pipes_finished)?;
        if started.elapsed() >= Duration::from_secs(2) {
            return Err("owned control cleanup exceeded two seconds".into());
        }
        observations.push(json!({"mode":if primary_only {"primary_only_then_actual_finish"} else {"actual_finish"},"same_group_and_live_before_cleanup":true,"primary_reaped":primary_reaped,"descendant_terminal":terminal,"pipe_threads_joined":pipes_finished,"cleanup_elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"primary_only_negative_control":wrong}));
    }
    Ok(
        json!({"status":"observed","scope":"synthetic same-group grandchild inheriting stdin/stdout; terminal means absent or zombie, not descendant reaping; no escaped-descendant claim","observations":observations}),
    )
}

pub(super) fn retention_child() -> Result<(), String> {
    let mut held = Vec::new();
    let mut input = BufReader::new(std::io::stdin());
    loop {
        let message = match read_message(&mut input, Wire::Mcp) {
            Ok(message) => message,
            Err(_) => return Ok(()),
        };
        if message["method"] != "hold" {
            return Err("unknown retention control request".into());
        }
        let bytes = message
            .pointer("/params/bytes")
            .and_then(Value::as_u64)
            .filter(|n| *n <= 32 * 1024 * 1024)
            .ok_or("invalid control size")?;
        if !held.is_empty() {
            return Err("control retains only one allocation".into());
        }
        held.resize(usize::try_from(bytes).map_err(|e| e.to_string())?, 1u8);
        // Touch every page and keep the buffer live until process teardown.
        std::hint::black_box(&mut held);
        println!(
            "{}",
            json!({"jsonrpc":"2.0","id":message["id"],"result":{"held_bytes":held.len()}})
        );
        std::io::stdout().flush().map_err(|e| e.to_string())?;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_requires_descendant_and_pipe_completion() {
        assert!(require_cleanup(true, true, true).is_ok());
        assert!(require_cleanup(true, false, false).is_err());
        assert!(require_cleanup(true, false, true).is_err());
        assert!(require_cleanup(true, true, false).is_err());
        assert!(require_cleanup(false, true, true).is_err());
    }
    #[test]
    fn current_and_lifetime_peak_are_distinct() {
        let text = "VmHWM:\t9000 kB\nVmRSS:\t2000 kB\n";
        assert_eq!(status_bytes(text, "VmRSS:"), Some(2_048_000));
        assert_eq!(status_bytes("VmRSS: 2 MB", "VmRSS:"), None);
    }
    #[test]
    fn frame_reader_rejects_bad_length_and_preserves_both_protocols() {
        let value = json!({"id":1,"result":{}});
        let body = value.to_string();
        assert_eq!(
            read_message(&mut format!("{body}\n").as_bytes(), Wire::Mcp).ok(),
            Some(value.clone())
        );
        assert_eq!(
            read_message(
                &mut format!("Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes(),
                Wire::Lsp
            )
            .ok(),
            Some(value)
        );
        assert!(
            read_message(
                &mut b"Content-Length: 999999999\r\n\r\n".as_slice(),
                Wire::Lsp
            )
            .is_err()
        );
        assert!(
            read_message(
                &mut b"Content-Length: 1\r\nContent-Length: 1\r\n\r\nx".as_slice(),
                Wire::Lsp
            )
            .is_err()
        );
    }
}
