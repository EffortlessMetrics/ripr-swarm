//! Parent-death watchdog for `ripr lsp --stdio` (#12575).
//!
//! Spawns the server under an intermediate launcher, kills the launcher without
//! closing stdio, and expects the server process to exit within a bounded time.

#![cfg(target_os = "linux")]

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{RecvTimeoutError, sync_channel};
use std::thread;
use std::time::{Duration, Instant};

const PROBE_MS: &str = "50";
const EXIT_BUDGET: Duration = Duration::from_secs(10);

fn direct_child_pid(parent: u32) -> Result<u32, String> {
    let path = format!("/proc/{parent}/task/{parent}/children");
    let raw = std::fs::read_to_string(&path).map_err(|err| format!("read {path}: {err}"))?;
    raw.split_whitespace()
        .next()
        .ok_or_else(|| format!("launcher {parent} has no child yet (children={raw:?})"))?
        .parse::<u32>()
        .map_err(|err| format!("parse child pid from {raw:?}: {err}"))
}

fn process_gone(pid: u32) -> bool {
    !Path::new(&format!("/proc/{pid}")).exists()
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
    let binary = env!("CARGO_BIN_EXE_ripr");
    let script = format!("({binary} lsp --stdio)");
    let mut launcher = Command::new("bash")
        .arg("-c")
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("RIPR_LSP_PARENT_DEATH_PROBE_MS", PROBE_MS)
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

    let deadline = Instant::now() + Duration::from_secs(3);
    let server_pid = loop {
        if let Ok(pid) = direct_child_pid(launcher_pid) {
            break pid;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for server child of launcher {launcher_pid}"
            ));
        }
        thread::sleep(Duration::from_millis(25));
    };
    owned.server_pid = Some(server_pid);

    let init = br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":null,"capabilities":{}}}"#;
    let frame = format!("Content-Length: {}\r\n\r\n", init.len());
    stdin
        .write_all(frame.as_bytes())
        .and_then(|()| stdin.write_all(init))
        .and_then(|()| stdin.flush())
        .map_err(|err| format!("write initialize: {err}"))?;

    await_initialize_result(stdout)?;

    if process_gone(server_pid) {
        return Err("server exited before launcher death; stdin may have closed".to_string());
    }

    Command::new("kill")
        .args(["-9", &launcher_pid.to_string()])
        .status()
        .map_err(|err| format!("terminate launcher: {err}"))?;

    let exit_deadline = Instant::now() + EXIT_BUDGET;
    while Instant::now() < exit_deadline {
        if process_gone(server_pid) {
            owned.server_pid = None;
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err(format!(
        "server {server_pid} still alive {EXIT_BUDGET:?} after launcher {launcher_pid} was terminated"
    ))
}
