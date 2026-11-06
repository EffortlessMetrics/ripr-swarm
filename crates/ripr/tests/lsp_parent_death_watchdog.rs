//! Parent-death watchdog for `ripr lsp --stdio` (#12575).
//!
//! Spawns the server under an intermediate launcher, kills the launcher without
//! closing stdio, and expects the server process to exit within a bounded time.

#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{RecvTimeoutError, sync_channel};
use std::thread;
use std::time::{Duration, Instant};

const PROBE_MS: &str = "50";
const EXIT_BUDGET: Duration = Duration::from_secs(10);

fn read_child_pid<R: Read + Send + 'static>(stdout: R) -> Result<(u32, BufReader<R>), String> {
    let (sender, receiver) = sync_channel(1);
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let result = reader
            .read_line(&mut line)
            .map_err(|err| format!("read child pid: {err}"))
            .and_then(|_| {
                line.trim()
                    .parse::<u32>()
                    .map_err(|err| format!("parse child pid from {line:?}: {err}"))
            })
            .map(|pid| (pid, reader));
        let _ = sender.send(result);
    });
    receiver
        .recv_timeout(Duration::from_secs(3))
        .map_err(|err| format!("waiting for server child pid: {err}"))?
}

fn process_exited(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat
            .rsplit_once(')')
            .is_some_and(|(_, tail)| tail.split_whitespace().next() == Some("Z")),
        Err(err) => err.kind() == std::io::ErrorKind::NotFound,
    }
}

struct OwnedTree {
    launcher: Child,
    server_pid: Option<u32>,
}

impl OwnedTree {
    fn terminate(&mut self) {
        if let Some(server_pid) = self.server_pid {
            let _ = Command::new("kill")
                .args(["-9", &server_pid.to_string()])
                .status();
        }
        let _ = self.launcher.kill();
        let _ = self.launcher.wait();
    }
}

impl Drop for OwnedTree {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn await_initialize_result(stdout: impl Read + Send + 'static) -> Result<(), String> {
    let (sender, receiver) = sync_channel(1);
    thread::spawn(move || {
        let mut stdout = stdout;
        let mut buffer = [0u8; 4096];
        let mut collected = Vec::new();
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    collected.extend_from_slice(&buffer[..n]);
                    if collected.windows(8).any(|window| window == b"\"result\"") {
                        let _ = sender.send(Ok(()));
                        return;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = sender.send(Err(format!(
            "initialize response missing result; saw {} byte(s)",
            collected.len()
        )));
    });
    match receiver.recv_timeout(Duration::from_secs(5)) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => {
            Err("timed out waiting for initialize result".to_string())
        }
        Err(RecvTimeoutError::Disconnected) => {
            Err("stdout reader exited before initialize result".to_string())
        }
    }
}

#[test]
fn server_exits_when_launcher_dies_without_closing_stdio() -> Result<(), String> {
    server_exits_after_launcher_death(PROBE_MS)
}

#[test]
fn zero_probe_interval_still_exits_after_launcher_death() -> Result<(), String> {
    server_exits_after_launcher_death("0")
}

fn server_exits_after_launcher_death(probe_ms: &str) -> Result<(), String> {
    let binary = env!("CARGO_BIN_EXE_ripr");
    let mut launcher = Command::new("bash")
        .arg("-c")
        .arg("\"$1\" lsp --stdio <&0 & child=$!; printf '%s\\n' \"$child\"; wait \"$child\"")
        .arg("ripr-watchdog-test")
        .arg(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("RIPR_LSP_PARENT_DEATH_PROBE_MS", probe_ms)
        .env("TOKIO_WORKER_THREADS", "2")
        .spawn()
        .map_err(|err| format!("spawn launcher: {err}"))?;
    let launcher_pid = launcher.id();
    let mut stdin = launcher
        .stdin
        .take()
        .ok_or_else(|| "launcher missing stdin".to_string())?;
    let stdout = launcher
        .stdout
        .take()
        .ok_or_else(|| "launcher missing stdout".to_string())?;

    let mut owned = OwnedTree {
        launcher,
        server_pid: None,
    };

    let (server_pid, stdout) = read_child_pid(stdout)?;
    owned.server_pid = Some(server_pid);

    let init = br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":null,"capabilities":{}}}"#;
    let frame = format!("Content-Length: {}\r\n\r\n", init.len());
    stdin
        .write_all(frame.as_bytes())
        .and_then(|()| stdin.write_all(init))
        .and_then(|()| stdin.flush())
        .map_err(|err| format!("write initialize: {err}"))?;

    await_initialize_result(stdout)?;

    if process_exited(server_pid) {
        return Err("server exited before launcher death; stdin may have closed".to_string());
    }

    let status = Command::new("kill")
        .args(["-9", &launcher_pid.to_string()])
        .status()
        .map_err(|err| format!("terminate launcher: {err}"))?;
    if !status.success() {
        return Err(format!("terminate launcher failed: {status}"));
    }

    let exit_deadline = Instant::now() + EXIT_BUDGET;
    while Instant::now() < exit_deadline {
        if process_exited(server_pid) {
            owned.server_pid = None;
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err(format!(
        "server {server_pid} still alive {EXIT_BUDGET:?} after launcher {launcher_pid} was terminated"
    ))
}
