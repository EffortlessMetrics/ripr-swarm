//! `cargo xtask windows-advisory-run` (#9272, #9660): the single-job Windows
//! advisory lane's step runner.
//!
//! The lane's cargo steps used to redirect all output to a file and replay it
//! with `cat` afterwards, because a `| tee` pipe is held open by any test that
//! leaks a child. That fixed the pipe hang but left two gaps: a step that never
//! ended published no output at all, so the job log could not say which test
//! was stuck, and when the hosted step timeout failed to kill the process tree
//! the job ran for hours (jobs 119992784688 and 120026188944).
//!
//! This runner owns the child instead. It
//!
//! - streams the child's stdout and stderr to the job log as they arrive and
//!   tees them into the log file the summarizer reads;
//! - enforces its own deadline and terminates the whole process tree (a Job
//!   Object on Windows) rather than trusting the step timeout;
//! - names every test libtest reported as running for over 60 seconds and not
//!   yet finished, so a timeout points at a test instead of at a step;
//! - bounds how long it waits for output after the child exits, and terminates
//!   descendants that still hold the pipe, so a leaked child cannot keep the
//!   step alive.
//!
//! It records the child's exit code in the status file, or 124 (the `timeout`
//! convention) when it enforced the deadline, and itself exits 0: test outcomes
//! stay advisory, and the summarizer is what refuses missing evidence.

use std::collections::{BTreeSet, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use ripr::process_owner::OwnedProcess;

/// Exit code recorded when the runner's own deadline ended the child.
const TIMEOUT_STATUS: i32 = 124;
/// Exit code recorded when the command could not be started (shell convention).
const SPAWN_FAILED_STATUS: i32 = 127;
/// How long output is awaited after the direct child has exited.
const DRAIN_GRACE: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(200);
const TAIL_LINES: usize = 8;

struct Options {
    timeout: Duration,
    log: PathBuf,
    status: PathBuf,
    program: String,
    args: Vec<String>,
    drain_grace: Duration,
}

#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    status: i32,
    timed_out: bool,
    /// Tests libtest reported as running for over 60 seconds with no result yet.
    still_running: Vec<String>,
    /// Output was still arriving after the child exited, so descendants held the
    /// pipe and were terminated.
    pipe_held_after_exit: bool,
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse(args)?;
    let stdout = std::io::stdout();
    execute(&options, &mut stdout.lock())?;
    Ok(())
}

fn parse(args: &[String]) -> Result<Options, String> {
    const USAGE: &str = "usage: cargo xtask windows-advisory-run --timeout-minutes <n> \
                         --log <path> --status <path> -- <program> [args...]";
    let mut timeout = None;
    let mut log = None;
    let mut status = None;
    let mut iter = args.iter();
    let mut command = Vec::new();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--timeout-minutes" => {
                let value = iter.next().ok_or(USAGE)?;
                let minutes: u64 = value.parse().map_err(|_parse| {
                    format!("--timeout-minutes must be a whole number: {value}")
                })?;
                if minutes == 0 {
                    return Err("--timeout-minutes must be at least 1".to_string());
                }
                timeout = Some(Duration::from_secs(minutes * 60));
            }
            "--log" => log = Some(PathBuf::from(iter.next().ok_or(USAGE)?)),
            "--status" => status = Some(PathBuf::from(iter.next().ok_or(USAGE)?)),
            "--" => {
                command.extend(iter.by_ref().cloned());
                break;
            }
            other => return Err(format!("unknown argument `{other}`; {USAGE}")),
        }
    }
    let (Some(timeout), Some(log), Some(status)) = (timeout, log, status) else {
        return Err(USAGE.to_string());
    };
    let Some((program, rest)) = command.split_first() else {
        return Err(USAGE.to_string());
    };
    Ok(Options {
        timeout,
        log,
        status,
        program: program.clone(),
        args: rest.to_vec(),
        drain_grace: DRAIN_GRACE,
    })
}

/// Tests libtest has announced as slow and not yet finished.
#[derive(Default)]
struct SlowTests(BTreeSet<String>);

impl SlowTests {
    fn observe(&mut self, line: &str) {
        let Some(rest) = line.trim_end().strip_prefix("test ") else {
            return;
        };
        if let Some((name, _)) = rest.split_once(" has been running for over ") {
            self.0.insert(name.to_string());
        } else if let Some((name, _)) = rest.split_once(" ... ") {
            self.0.remove(name);
        }
    }
}

fn spawn_reader<R: Read + Send + 'static>(stream: R, tx: mpsc::Sender<Vec<u8>>) {
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        loop {
            let mut line = Vec::new();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => return,
                Ok(_) => {
                    if tx.send(line).is_err() {
                        return;
                    }
                }
            }
        }
    });
}

fn execute(options: &Options, out: &mut dyn Write) -> Result<Outcome, String> {
    if let Some(parent) = options
        .log
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    let mut log = File::create(&options.log)
        .map_err(|err| format!("create {}: {err}", options.log.display()))?;
    let describe = std::iter::once(options.program.as_str())
        .chain(options.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");

    let mut command = Command::new(&options.program);
    command
        .args(&options.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match OwnedProcess::spawn(command) {
        Ok(child) => child,
        Err(err) => {
            let banner = format!("windows-advisory-run: could not start `{describe}`: {err}\n");
            emit(&mut log, out, banner.as_bytes());
            write_status(options, SPAWN_FAILED_STATUS)?;
            return Ok(Outcome {
                status: SPAWN_FAILED_STATUS,
                timed_out: false,
                still_running: Vec::new(),
                pipe_held_after_exit: false,
            });
        }
    };
    let (tx, rx) = mpsc::channel();
    if let Some(stdout) = child.stdout_pipe().take() {
        spawn_reader(stdout, tx.clone());
    }
    if let Some(stderr) = child.stderr_pipe().take() {
        spawn_reader(stderr, tx.clone());
    }
    drop(tx);

    let started = Instant::now();
    let mut slow = SlowTests::default();
    let mut tail: VecDeque<String> = VecDeque::new();
    let mut exited: Option<(Option<i32>, Instant)> = None;
    let mut timed_out = false;
    let mut readers_done = false;
    let mut pipe_held_after_exit = false;

    loop {
        match rx.recv_timeout(POLL) {
            Ok(line) => handle_line(&mut log, out, &mut slow, &mut tail, &line),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => readers_done = true,
        }
        if exited.is_none() {
            if let Some(status) = child
                .try_wait()
                .map_err(|err| format!("poll `{describe}`: {err}"))?
            {
                exited = Some((status.code(), Instant::now()));
            } else if started.elapsed() >= options.timeout {
                timed_out = true;
                let ended = child.terminate_tree();
                let status = child.wait().ok().and_then(|status| status.code());
                exited = Some((status, Instant::now()));
                if let Err(err) = ended {
                    let banner = format!("windows-advisory-run: tree termination: {err}\n");
                    emit(&mut log, out, banner.as_bytes());
                }
            }
        }
        if let Some((_, at)) = exited {
            if readers_done {
                break;
            }
            if at.elapsed() >= options.drain_grace {
                pipe_held_after_exit = true;
                break;
            }
        }
    }
    if pipe_held_after_exit {
        // A descendant still holds the output pipe: end it so the step does not
        // outlive its command, then take whatever was already queued.
        let _ = child.terminate_tree();
        while let Ok(line) = rx.try_recv() {
            handle_line(&mut log, out, &mut slow, &mut tail, &line);
        }
    }

    let (code, _) = exited.unwrap_or((None, Instant::now()));
    let status = if timed_out {
        TIMEOUT_STATUS
    } else {
        code.unwrap_or(1)
    };
    let still_running: Vec<String> = slow.0.iter().cloned().collect();
    let mut banner = String::new();
    if timed_out {
        banner.push_str(&format!(
            "windows-advisory-run: TIMEOUT after {}s running `{describe}`; process tree terminated\n",
            options.timeout.as_secs()
        ));
        if still_running.is_empty() {
            banner.push_str(
                "windows-advisory-run: no test was reported running for over 60 seconds; \
                 last output:\n",
            );
            for line in &tail {
                banner.push_str(&format!("  {line}\n"));
            }
        } else {
            for name in &still_running {
                banner.push_str(&format!("windows-advisory-run: still running: {name}\n"));
            }
        }
    }
    if pipe_held_after_exit {
        banner.push_str(&format!(
            "windows-advisory-run: output was still open {}s after the command exited; \
             descendants holding the pipe were terminated\n",
            options.drain_grace.as_secs()
        ));
    }
    if !banner.is_empty() {
        emit(&mut log, out, banner.as_bytes());
    }
    write_status(options, status)?;
    Ok(Outcome {
        status,
        timed_out,
        still_running,
        pipe_held_after_exit,
    })
}

fn handle_line(
    log: &mut File,
    out: &mut dyn Write,
    slow: &mut SlowTests,
    tail: &mut VecDeque<String>,
    line: &[u8],
) {
    emit(log, out, line);
    let text = String::from_utf8_lossy(line);
    slow.observe(&text);
    tail.push_back(text.trim_end().to_string());
    if tail.len() > TAIL_LINES {
        tail.pop_front();
    }
}

/// Write to the log file and the job log. A failed write is not a reason to
/// stop the child: the status file and the other sink still carry the evidence.
fn emit(log: &mut File, out: &mut dyn Write, bytes: &[u8]) {
    let _ = log.write_all(bytes);
    let _ = log.flush();
    let _ = out.write_all(bytes);
    let _ = out.flush();
}

fn write_status(options: &Options, status: i32) -> Result<(), String> {
    std::fs::write(&options.status, format!("{status}\n"))
        .map_err(|err| format!("write {}: {err}", options.status.display()))
}

#[cfg(test)]
mod windows_advisory_run_tests {
    use super::*;

    fn scratch(name: &str) -> Result<PathBuf, String> {
        let dir =
            std::env::temp_dir().join(format!("ripr-advisory-run-{}-{name}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|err| err.to_string())?;
        }
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        Ok(dir)
    }

    #[cfg(unix)]
    fn options(dir: &std::path::Path, timeout: Duration, script: &str) -> Options {
        Options {
            timeout,
            log: dir.join("run.log"),
            status: dir.join("run.status"),
            program: "sh".to_string(),
            args: vec!["-c".to_string(), script.to_string()],
            drain_grace: Duration::from_secs(2),
        }
    }

    #[test]
    fn libtest_slow_markers_are_tracked_until_the_test_finishes() {
        let mut slow = SlowTests::default();
        slow.observe("test a::b has been running for over 60 seconds\n");
        slow.observe("test c::d has been running for over 60 seconds\n");
        slow.observe("test a::b ... ok\n");
        assert_eq!(slow.0.into_iter().collect::<Vec<_>>(), vec!["c::d"]);
    }

    #[test]
    fn unrelated_lines_do_not_change_the_slow_set() {
        let mut slow = SlowTests::default();
        slow.observe("running 3 tests\n");
        slow.observe("   Compiling ripr v0.1.0\n");
        assert!(slow.0.is_empty());
    }

    #[test]
    fn arguments_require_all_options_and_a_command() {
        let args = |parts: &[&str]| parts.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(parse(&args(&["--log", "l", "--status", "s", "--", "x"])).is_err());
        assert!(
            parse(&args(&[
                "--timeout-minutes",
                "5",
                "--log",
                "l",
                "--status",
                "s"
            ]))
            .is_err()
        );
        assert!(
            parse(&args(&[
                "--timeout-minutes",
                "0",
                "--log",
                "l",
                "--status",
                "s",
                "--",
                "x"
            ]))
            .is_err()
        );
        let parsed = parse(&args(&[
            "--timeout-minutes",
            "5",
            "--log",
            "l",
            "--status",
            "s",
            "--",
            "cargo",
            "test",
            "--workspace",
        ]));
        assert!(matches!(
            parsed,
            Ok(Options { ref program, ref args, .. })
                if program == "cargo" && args == &["test", "--workspace"]
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_normal_exit_streams_output_and_records_the_child_status() -> Result<(), String> {
        let dir = scratch("normal")?;
        let opts = options(
            &dir,
            Duration::from_secs(30),
            "echo out; echo err 1>&2; exit 3",
        );
        let mut sink = Vec::new();
        let outcome = execute(&opts, &mut sink)?;
        assert_eq!(outcome.status, 3);
        assert!(!outcome.timed_out);
        let streamed = String::from_utf8_lossy(&sink).into_owned();
        assert!(
            streamed.contains("out") && streamed.contains("err"),
            "{streamed}"
        );
        let log = std::fs::read_to_string(&opts.log).map_err(|err| err.to_string())?;
        assert_eq!(
            log, streamed,
            "the log file carries exactly the streamed output"
        );
        assert_eq!(
            std::fs::read_to_string(&opts.status).map_err(|err| err.to_string())?,
            "3\n"
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_command_is_killed_at_the_deadline_and_the_slow_test_is_named() -> Result<(), String> {
        let dir = scratch("timeout")?;
        // Prints a libtest slow marker, then hangs far past the deadline.
        let mut opts = options(
            &dir,
            Duration::from_mins(1),
            "echo 'test stuck::forever has been running for over 60 seconds'; sleep 120",
        );
        opts.timeout = Duration::from_secs(1);
        let mut sink = Vec::new();
        let started = Instant::now();
        let outcome = execute(&opts, &mut sink)?;
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the deadline must end the command"
        );
        assert!(outcome.timed_out);
        assert_eq!(outcome.status, TIMEOUT_STATUS);
        assert_eq!(outcome.still_running, vec!["stuck::forever".to_string()]);
        let streamed = String::from_utf8_lossy(&sink).into_owned();
        assert!(streamed.contains("TIMEOUT after 1s"), "{streamed}");
        assert!(
            streamed.contains("still running: stuck::forever"),
            "{streamed}"
        );
        assert_eq!(
            std::fs::read_to_string(&opts.status).map_err(|err| err.to_string())?,
            "124\n"
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_leaked_descendant_holding_the_pipe_does_not_keep_the_step_alive() -> Result<(), String> {
        let dir = scratch("leak")?;
        // The shell exits at once; the background sleep inherits the pipes.
        let opts = options(&dir, Duration::from_mins(1), "sleep 120 & echo finished");
        let mut sink = Vec::new();
        let started = Instant::now();
        let outcome = execute(&opts, &mut sink)?;
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the grace must bound the wait"
        );
        assert_eq!(outcome.status, 0);
        assert!(!outcome.timed_out);
        assert!(outcome.pipe_held_after_exit);
        let streamed = String::from_utf8_lossy(&sink).into_owned();
        assert!(streamed.contains("finished"), "{streamed}");
        assert!(
            streamed.contains("descendants holding the pipe were terminated"),
            "{streamed}"
        );
        Ok(())
    }

    #[test]
    fn a_missing_program_records_a_spawn_failure_instead_of_erroring() -> Result<(), String> {
        let dir = scratch("spawn")?;
        let opts = Options {
            timeout: Duration::from_secs(5),
            log: dir.join("run.log"),
            status: dir.join("run.status"),
            program: "ripr-definitely-not-a-program".to_string(),
            args: Vec::new(),
            drain_grace: Duration::from_secs(1),
        };
        let mut sink = Vec::new();
        let outcome = execute(&opts, &mut sink)?;
        assert_eq!(outcome.status, SPAWN_FAILED_STATUS);
        assert_eq!(
            std::fs::read_to_string(&opts.status).map_err(|err| err.to_string())?,
            "127\n"
        );
        Ok(())
    }
}
