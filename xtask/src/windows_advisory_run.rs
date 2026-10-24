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
use std::process::Command;
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
/// How long the primary is observed after a termination request.
const TERMINATION_WAIT: Duration = Duration::from_secs(10);
/// Chunks the job-log queue holds before the sink is declared lagging.
const SINK_QUEUE: usize = 4096;
/// How long the queued job-log output is given to flush at exit.
const SINK_FLUSH_WAIT: Duration = Duration::from_secs(5);

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
    /// Termination was requested but not confirmed.
    termination_error: Option<String>,
    /// The log file or the command pipe failed, so the log may be incomplete.
    output_faults: Vec<String>,
    /// The job-log copy was lossy; the log file is unaffected.
    sink_note: Option<String>,
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse(args)?;
    execute(&options, Box::new(std::io::stdout()))?;
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
                if timeout.is_some() {
                    return Err(format!("--timeout-minutes given twice; {USAGE}"));
                }
                let value = iter.next().ok_or(USAGE)?;
                let minutes: u64 = value.parse().map_err(|_parse| {
                    format!("--timeout-minutes must be a whole number: {value}")
                })?;
                if minutes == 0 {
                    return Err("--timeout-minutes must be at least 1".to_string());
                }
                let seconds = minutes
                    .checked_mul(60)
                    .ok_or_else(|| format!("--timeout-minutes is too large: {value}"))?;
                timeout = Some(Duration::from_secs(seconds));
            }
            "--log" if log.is_some() => return Err(format!("--log given twice; {USAGE}")),
            "--status" if status.is_some() => {
                return Err(format!("--status given twice; {USAGE}"));
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
            // libtest spells a finished should_panic test `NAME - should panic`.
            self.0
                .remove(name.strip_suffix(" - should panic").unwrap_or(name));
        }
    }
}

/// What the reader thread hands the supervisor.
enum Chunk {
    Line(Vec<u8>),
    /// The pipe failed before EOF. Without this the supervisor could not tell a
    /// lost suffix from a clean end of output.
    ReadFault(String),
}

fn spawn_reader<R: Read + Send + 'static>(stream: R, tx: mpsc::Sender<Chunk>) {
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        loop {
            let mut line = Vec::new();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) => return,
                Ok(_) => {
                    if tx.send(Chunk::Line(line)).is_err() {
                        return;
                    }
                }
                Err(err) => {
                    let _ = tx.send(Chunk::ReadFault(format!("read the command output: {err}")));
                    return;
                }
            }
        }
    });
}

/// Delivery to the job log runs on its own thread behind a bounded queue. The
/// supervisor polls the deadline and must never wait on a stalled sink, so a
/// full queue drops the chunk from the job log (the log file keeps it) and is
/// reported once the command is done.
struct Sink {
    tx: Option<mpsc::SyncSender<Vec<u8>>>,
    dropped: usize,
    failure: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    done: mpsc::Receiver<()>,
}

impl Sink {
    fn start(mut out: Box<dyn Write + Send>) -> Self {
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(SINK_QUEUE);
        let (done_tx, done) = mpsc::channel();
        let failure = std::sync::Arc::new(std::sync::Mutex::new(None));
        let thread_failure = std::sync::Arc::clone(&failure);
        thread::spawn(move || {
            let mut failed = false;
            for bytes in rx {
                if failed {
                    continue;
                }
                if let Err(err) = out.write_all(&bytes).and_then(|()| out.flush()) {
                    failed = true;
                    if let Ok(mut slot) = thread_failure.lock() {
                        *slot = Some(err.to_string());
                    }
                }
            }
            let _ = done_tx.send(());
        });
        Self {
            tx: Some(tx),
            dropped: 0,
            failure,
            done,
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        if let Some(tx) = &self.tx
            && let Err(mpsc::TrySendError::Full(_)) = tx.try_send(bytes.to_vec())
        {
            self.dropped += 1;
        }
    }

    /// Close the queue and wait a bounded time for what is queued to be written.
    /// A sink that is still blocked is left behind rather than awaited.
    fn finish(&mut self) -> Option<String> {
        self.tx = None;
        let drained = self.done.recv_timeout(SINK_FLUSH_WAIT).is_ok();
        let failure = self.failure.lock().ok().and_then(|slot| slot.clone());
        match (drained, failure) {
            (_, Some(err)) => Some(format!("job log write failed: {err}")),
            (false, None) => Some("job log delivery was still blocked at exit".to_string()),
            (true, None) if self.dropped > 0 => Some(format!(
                "job log delivery lagged and {} chunks were not streamed",
                self.dropped
            )),
            (true, None) => None,
        }
    }
}

/// Everything the supervisor writes goes through here: the log file the
/// summarizer reads, the job-log sink, and the evidence faults found on the way.
struct Output {
    log: File,
    sink: Sink,
    faults: Vec<String>,
}

impl Output {
    /// A fault in the log file or the command's pipe means the log may not be the
    /// whole output. The job-log sink is a convenience copy and is reported
    /// separately.
    fn emit(&mut self, bytes: &[u8]) {
        if let Err(err) = self.log.write_all(bytes).and_then(|()| self.log.flush()) {
            self.fault(format!("write the log file: {err}"));
        }
        self.sink.send(bytes);
    }

    fn fault(&mut self, what: String) {
        if !self.faults.contains(&what) {
            self.faults.push(what);
        }
    }

    fn handle(&mut self, slow: &mut SlowTests, tail: &mut VecDeque<String>, chunk: Chunk) {
        match chunk {
            Chunk::Line(line) => {
                self.emit(&line);
                let text = String::from_utf8_lossy(&line);
                slow.observe(&text);
                tail.push_back(text.trim_end().to_string());
                if tail.len() > TAIL_LINES {
                    tail.pop_front();
                }
            }
            Chunk::ReadFault(what) => self.fault(what),
        }
    }
}

fn execute(options: &Options, out: Box<dyn Write + Send>) -> Result<Outcome, String> {
    if let Some(parent) = options
        .log
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    // A stale status from an earlier run must not stand in for this one: when
    // the output evidence is incomplete no status is written at all.
    match std::fs::remove_file(&options.status) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(format!("remove {}: {err}", options.status.display())),
    }
    let log = File::create(&options.log)
        .map_err(|err| format!("create {}: {err}", options.log.display()))?;
    let mut output = Output {
        log,
        sink: Sink::start(out),
        faults: Vec::new(),
    };
    let describe = std::iter::once(options.program.as_str())
        .chain(options.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");

    // One pipe for both streams, like `> log 2>&1`: cargo writes its `Running`
    // headers to stderr and libtest writes `test result:` to stdout, and the
    // summarizer attributes results to the most recent header, so the two must
    // keep the order the child wrote them in. Two pipes drained by two threads
    // would let a result overtake its header.
    let (reader, writer) =
        std::io::pipe().map_err(|err| format!("create the output pipe: {err}"))?;
    let stderr_writer = writer
        .try_clone()
        .map_err(|err| format!("clone the output pipe: {err}"))?;
    let mut command = Command::new(&options.program);
    command
        .args(&options.args)
        .stdout(writer)
        .stderr(stderr_writer);
    let spawned = OwnedProcess::spawn(command);
    let mut child = match spawned {
        Ok(child) => child,
        Err(err) => {
            let banner = format!("windows-advisory-run: could not start `{describe}`: {err}\n");
            output.emit(banner.as_bytes());
            let sink_note = output.sink.finish();
            write_status(options, SPAWN_FAILED_STATUS)?;
            return Ok(Outcome {
                status: SPAWN_FAILED_STATUS,
                timed_out: false,
                still_running: Vec::new(),
                pipe_held_after_exit: false,
                termination_error: None,
                output_faults: output.faults,
                sink_note,
            });
        }
    };
    let (tx, rx) = mpsc::channel();
    spawn_reader(reader, tx);

    let started = Instant::now();
    let mut slow = SlowTests::default();
    let mut tail: VecDeque<String> = VecDeque::new();
    let mut exited: Option<(Option<i32>, Instant)> = None;
    let mut timed_out = false;
    let mut readers_done = false;
    let mut pipe_held_after_exit = false;
    let mut termination_error: Option<String> = None;

    loop {
        match rx.recv_timeout(POLL) {
            Ok(chunk) => output.handle(&mut slow, &mut tail, chunk),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                readers_done = true;
                // A disconnected channel returns at once; wait out the poll
                // interval instead of spinning until the child exits.
                if exited.is_none() {
                    thread::sleep(POLL);
                }
            }
        }
        if exited.is_none() {
            if let Some(status) = child
                .try_wait()
                .map_err(|err| format!("poll `{describe}`: {err}"))?
            {
                exited = Some((status.code(), Instant::now()));
            } else if started.elapsed() >= options.timeout {
                timed_out = true;
                let (status, error) = terminate(&mut child, TERMINATION_WAIT);
                termination_error = error;
                exited = Some((status, Instant::now()));
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
        let (_, error) = terminate(&mut child, TERMINATION_WAIT);
        termination_error = termination_error.or(error);
        while let Ok(chunk) = rx.try_recv() {
            output.handle(&mut slow, &mut tail, chunk);
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
        let result = match &termination_error {
            None => "process tree terminated".to_string(),
            Some(err) => format!("process tree termination NOT confirmed: {err}"),
        };
        banner.push_str(&format!(
            "windows-advisory-run: TIMEOUT after {}s running `{describe}`; {result}\n",
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
        let result = match &termination_error {
            None => "termination of the process tree was requested".to_string(),
            Some(err) => format!("termination of the process tree was requested but failed: {err}"),
        };
        banner.push_str(&format!(
            "windows-advisory-run: output was still open {}s after the command exited; {result}\n",
            options.drain_grace.as_secs()
        ));
    }
    if !output.faults.is_empty() {
        banner.push_str(&format!(
            "windows-advisory-run: OUTPUT EVIDENCE INCOMPLETE ({}); command status {status} \
             is not recorded so the summarizer refuses this step\n",
            output.faults.join("; ")
        ));
    }
    if !banner.is_empty() {
        output.emit(banner.as_bytes());
    }
    let sink_note = output.sink.finish();
    if let Some(note) = &sink_note {
        // Not evidence loss: the log file above is the record.
        let line = format!("windows-advisory-run: {note}; the log file is the record\n");
        if let Err(err) = output.log.write_all(line.as_bytes()) {
            output.fault(format!("write the log file: {err}"));
        }
    }
    if output.faults.is_empty() {
        write_status(options, status)?;
    }
    Ok(Outcome {
        status,
        timed_out,
        still_running,
        pipe_held_after_exit,
        termination_error,
        output_faults: output.faults,
        sink_note,
    })
}

/// The two owner calls `terminate` needs, so a termination failure with the
/// primary still live can be injected in tests.
trait OwnedTree {
    fn terminate_tree(&mut self) -> Result<(), String>;
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>>;
}

impl OwnedTree for OwnedProcess {
    fn terminate_tree(&mut self) -> Result<(), String> {
        OwnedProcess::terminate_tree(self)
    }

    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        OwnedProcess::try_wait(self)
    }
}

/// End the owned tree and observe the primary for a bounded time. The owner
/// reaps the primary on success; on failure it can leave a live primary, so
/// this never enters its unbounded `wait`.
fn terminate(child: &mut impl OwnedTree, wait: Duration) -> (Option<i32>, Option<String>) {
    let error = child.terminate_tree().err();
    let deadline = Instant::now() + wait;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return (status.code(), error),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let note = format!(
                    "the primary was still running {}ms after the request",
                    wait.as_millis()
                );
                return (None, Some(join_errors(error, note)));
            }
            Err(err) => {
                return (
                    None,
                    Some(join_errors(error, format!("poll the primary: {err}"))),
                );
            }
        }
    }
}

fn join_errors(first: Option<String>, second: String) -> String {
    match first {
        Some(first) => format!("{first}; {second}"),
        None => second,
    }
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

    /// A sink the test can read back after `execute` hands it away.
    #[derive(Clone, Default)]
    struct Shared(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl Write for Shared {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .map_err(|_poisoned| std::io::Error::other("poisoned"))?
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Shared {
        fn text(&self) -> String {
            self.0
                .lock()
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_default()
        }
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
        let sink = Shared::default();
        let outcome = execute(&opts, Box::new(sink.clone()))?;
        assert_eq!(outcome.status, 3);
        assert!(!outcome.timed_out);
        let streamed = sink.text();
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
        let sink = Shared::default();
        let started = Instant::now();
        let outcome = execute(&opts, Box::new(sink.clone()))?;
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the deadline must end the command"
        );
        assert!(outcome.timed_out);
        assert_eq!(outcome.status, TIMEOUT_STATUS);
        assert_eq!(outcome.still_running, vec!["stuck::forever".to_string()]);
        let streamed = sink.text();
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
        let sink = Shared::default();
        let started = Instant::now();
        let outcome = execute(&opts, Box::new(sink.clone()))?;
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the grace must bound the wait"
        );
        assert_eq!(outcome.status, 0);
        assert!(!outcome.timed_out);
        assert!(outcome.pipe_held_after_exit);
        let streamed = sink.text();
        assert!(streamed.contains("finished"), "{streamed}");
        assert!(
            streamed.contains("termination of the process tree was requested"),
            "{streamed}"
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn stdout_and_stderr_keep_the_order_the_child_wrote_them() -> Result<(), String> {
        let dir = scratch("order")?;
        let script = "for i in 1 2 3 4 5 6 7 8 9 10; do echo \"o$i\"; echo \"e$i\" 1>&2; done";
        let opts = options(&dir, Duration::from_mins(1), script);
        let sink = Shared::default();
        execute(&opts, Box::new(sink.clone()))?;
        let log = std::fs::read_to_string(&opts.log).map_err(|err| err.to_string())?;
        let expected: String = (1..=10).map(|i| format!("o{i}\ne{i}\n")).collect();
        assert_eq!(log, expected);
        Ok(())
    }

    #[test]
    fn a_should_panic_test_that_finished_is_no_longer_named_as_running() {
        let mut slow = SlowTests::default();
        slow.observe("test a::p has been running for over 60 seconds\n");
        slow.observe("test a::p - should panic ... ok\n");
        assert!(slow.0.is_empty());
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
        let sink = Shared::default();
        let outcome = execute(&opts, Box::new(sink.clone()))?;
        assert_eq!(outcome.status, SPAWN_FAILED_STATUS);
        assert_eq!(
            std::fs::read_to_string(&opts.status).map_err(|err| err.to_string())?,
            "127\n"
        );
        Ok(())
    }

    struct StuckTree;

    impl OwnedTree for StuckTree {
        fn terminate_tree(&mut self) -> Result<(), String> {
            Err("job termination refused".to_string())
        }

        fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
            Ok(None)
        }
    }

    #[test]
    fn a_failed_termination_with_a_live_primary_returns_bounded_and_says_so() {
        let started = Instant::now();
        let (status, error) = terminate(&mut StuckTree, Duration::from_millis(100));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(status, None);
        let error = error.unwrap_or_default();
        assert!(error.contains("job termination refused"), "{error}");
        assert!(error.contains("still running"), "{error}");
    }

    #[test]
    fn duplicate_options_are_refused_instead_of_overwritten() {
        let args = |parts: &[&str]| parts.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for (dup, name) in [
            (["--timeout-minutes", "5"], "--timeout-minutes"),
            (["--log", "l2"], "--log"),
            (["--status", "s2"], "--status"),
        ] {
            let mut parts = vec!["--timeout-minutes", "5", "--log", "l", "--status", "s"];
            parts.extend(dup);
            parts.extend(["--", "x"]);
            let err = parse(&args(&parts)).err().unwrap_or_default();
            assert!(err.contains(&format!("{name} given twice")), "{err}");
        }
    }

    #[test]
    fn a_pipe_read_fault_is_kept_as_an_output_fault() -> Result<(), String> {
        let dir = scratch("readfault")?;
        let mut output = Output {
            log: File::create(dir.join("run.log")).map_err(|err| err.to_string())?,
            sink: Sink::start(Box::new(Shared::default())),
            faults: Vec::new(),
        };
        let mut slow = SlowTests::default();
        let mut tail = VecDeque::new();
        output.handle(&mut slow, &mut tail, Chunk::Line(b"ok\n".to_vec()));
        output.handle(&mut slow, &mut tail, Chunk::ReadFault("boom".to_string()));
        assert_eq!(output.faults, vec!["boom".to_string()]);
        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_failing_log_file_withholds_the_status_so_the_summarizer_refuses() -> Result<(), String> {
        let dir = scratch("logfault")?;
        let mut opts = options(&dir, Duration::from_mins(1), "echo hello; exit 0");
        // Writes to /dev/full always fail with ENOSPC.
        opts.log = PathBuf::from("/dev/full");
        let sink = Shared::default();
        let outcome = execute(&opts, Box::new(sink.clone()))?;
        assert_eq!(outcome.status, 0);
        assert!(!outcome.output_faults.is_empty());
        assert!(!opts.status.exists(), "no status may stand for lost output");
        let streamed = sink.text();
        assert!(
            streamed.contains("OUTPUT EVIDENCE INCOMPLETE"),
            "{streamed}"
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_stalled_job_log_does_not_delay_the_deadline() -> Result<(), String> {
        struct Blocked(std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>);
        impl Write for Blocked {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                let (lock, cv) = &*self.0;
                let mut released = lock
                    .lock()
                    .map_err(|_poisoned| std::io::Error::other("poisoned"))?;
                while !*released {
                    released = cv
                        .wait(released)
                        .map_err(|_poisoned| std::io::Error::other("poisoned"))?;
                }
                Ok(bytes.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let gate = std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
        let dir = scratch("stalled")?;
        let mut opts = options(
            &dir,
            Duration::from_secs(1),
            "echo 'test stuck::forever has been running for over 60 seconds'; sleep 120",
        );
        opts.timeout = Duration::from_secs(1);
        let started = Instant::now();
        let outcome = execute(&opts, Box::new(Blocked(std::sync::Arc::clone(&gate))))?;
        // The writer is still blocked: the deadline fired and finished anyway.
        assert!(started.elapsed() < Duration::from_secs(30));
        assert!(outcome.timed_out);
        assert_eq!(outcome.still_running, vec!["stuck::forever".to_string()]);
        assert!(
            outcome
                .sink_note
                .as_deref()
                .is_some_and(|note| note.contains("blocked")),
            "{:?}",
            outcome.sink_note
        );
        assert_eq!(
            std::fs::read_to_string(&opts.status).map_err(|err| err.to_string())?,
            "124\n"
        );
        let log = std::fs::read_to_string(&opts.log).map_err(|err| err.to_string())?;
        assert!(log.contains("still running: stuck::forever"), "{log}");
        let (lock, cv) = &*gate;
        if let Ok(mut released) = lock.lock() {
            *released = true;
            cv.notify_all();
        }
        Ok(())
    }

    /// Native Windows control for the forced-deadline path (#9272): the primary
    /// and a pipe-holding descendant outlive the deadline, so the Job Object has
    /// to end both, and an unrelated process outside the job must survive.
    #[cfg(windows)]
    #[test]
    fn windows_native_deadline_ends_the_descendant_and_spares_a_sentinel() -> Result<(), String> {
        fn pid_alive(pid: u32) -> Result<bool, String> {
            let out = Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}"), "/NH"])
                .output()
                .map_err(|err| err.to_string())?;
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            Ok(text.split_whitespace().any(|word| word == pid.to_string()))
        }

        let dir = scratch("native-deadline")?;
        let pid_file = dir.join("descendant.pid");
        let mut sentinel = Command::new("powershell")
            .args(["-NoProfile", "-Command", "Start-Sleep 600"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|err| format!("start the sentinel: {err}"))?;
        let script = format!(
            "$d = Start-Process -PassThru -NoNewWindow powershell \
             -ArgumentList '-NoProfile','-Command','Start-Sleep 600'; \
             Set-Content -Path '{}' -Value $d.Id; \
             Write-Output 'test stuck::forever has been running for over 60 seconds'; \
             Start-Sleep 600",
            pid_file.display()
        );
        let opts = Options {
            timeout: Duration::from_secs(20),
            log: dir.join("run.log"),
            status: dir.join("run.status"),
            program: "powershell".to_string(),
            args: vec!["-NoProfile".to_string(), "-Command".to_string(), script],
            drain_grace: Duration::from_secs(5),
        };
        let sink = Shared::default();
        let started = Instant::now();
        let outcome = execute(&opts, Box::new(sink.clone()))?;
        let elapsed = started.elapsed();
        let descendant: u32 = std::fs::read_to_string(&pid_file)
            .map_err(|err| format!("the descendant never recorded its pid: {err}"))?
            .trim()
            .parse()
            .map_err(|err| format!("descendant pid: {err}"))?;
        let descendant_alive = pid_alive(descendant)?;
        let sentinel_alive = sentinel
            .try_wait()
            .map_err(|err| err.to_string())?
            .is_none();
        sentinel.kill().map_err(|err| err.to_string())?;
        println!(
            "native deadline control: elapsed={elapsed:?} outcome={outcome:?} \
             descendant_pid={descendant} descendant_alive={descendant_alive} \
             sentinel_alive={sentinel_alive}"
        );
        println!("--- retained log ---\n{}", sink.text());
        assert!(outcome.timed_out);
        assert_eq!(outcome.status, TIMEOUT_STATUS);
        assert_eq!(outcome.termination_error, None);
        assert_eq!(outcome.still_running, vec!["stuck::forever".to_string()]);
        assert!(!descendant_alive, "the Job Object must end the descendant");
        assert!(sentinel_alive, "a process outside the job must survive");
        assert_eq!(
            std::fs::read_to_string(&opts.status).map_err(|err| err.to_string())?,
            "124\n"
        );
        Ok(())
    }
}
