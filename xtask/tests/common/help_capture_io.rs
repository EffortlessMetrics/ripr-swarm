//! Operation-preserving I/O diagnostics for the bounded help-proof adapter.
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

// Concurrent tests in one process can read the same clock tick. Nested
// process-group cleanup also allocates a help-command scratch while another
// capture may be doing the same, so the name carries a process-wide sequence
// in addition to pid and clock nonce. Historical `ripr-{label}-{pid}-{nonce}`
// names failed exclusive create_dir with EEXIST (os error 17) on the real
// group-cleanup path (#7545).
static SCRATCH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn io_error(operation: &str, path: &Path, error: io::Error) -> String {
    format!(
        "{operation} at {path:?}: {error} (kind={:?}, raw_os_error={:?})",
        error.kind(),
        error.raw_os_error()
    )
}

pub fn ensure_parent(parent: &Path) -> Result<(), String> {
    std::fs::create_dir_all(parent).map_err(|e| io_error("create scratch parent", parent, e))
}

pub fn create_root(root: &Path) -> Result<(), String> {
    std::fs::create_dir(root).map_err(|e| io_error("create exclusive scratch root", root, e))
}

pub fn exclusive_scratch_root(label: &str, nonce: u128) -> Result<PathBuf, String> {
    let parent = std::env::temp_dir();
    ensure_parent(&parent)?;
    let sequence = SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = parent.join(format!(
        "ripr-{label}-{}-{nonce}-{sequence}",
        std::process::id()
    ));
    create_root(&root)?;
    Ok(root)
}

pub fn exclusive_capture(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| io_error("create exclusive capture", path, e))
}

pub fn read_capture(path: &Path) -> Result<Vec<u8>, String> {
    const CAP: u64 = 4 * 1024 * 1024;
    let file = File::open(path).map_err(|e| io_error("open capture for read", path, e))?;
    let mut bytes = Vec::new();
    file.take(CAP + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error("read capture", path, e))?;
    if bytes.len() as u64 > CAP {
        return Err(format!("{path:?} exceeded the {CAP}-byte capture cap"));
    }
    Ok(bytes)
}

pub fn spawn_context(command: &Command, scratch: &Path) -> String {
    // Command's Debug representation can include environment values.
    format!(
        "spawn owned command program={:?}, args={:?}, cwd={:?}, scratch={scratch:?}",
        command.get_program(),
        command.get_args().collect::<Vec<_>>(),
        command.get_current_dir()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::PathBuf;
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Result<Self, String> {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let parent = std::env::temp_dir();
            ensure_parent(&parent)?;
            let root = parent.join(format!(
                "ripr-capture-diagnostics-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            create_root(&root)?;
            Ok(Self(root))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(e) = std::fs::remove_dir_all(&self.0) {
                eprintln!("diagnostic fixture cleanup {:?}: {e}", self.0);
            }
        }
    }

    fn diagnostic(
        error: String,
        operation: &str,
        path: &Path,
        actual: io::Error,
    ) -> Result<(), String> {
        for expected in [
            operation.to_string(),
            format!("{path:?}"),
            format!("kind={:?}", actual.kind()),
            format!("raw_os_error={:?}", actual.raw_os_error()),
            actual.to_string(),
        ] {
            if !error.contains(&expected) {
                return Err(format!("missing {expected:?} in diagnostic {error:?}"));
            }
        }
        Ok(())
    }

    #[test]
    fn occupied_parent_and_root_name_distinct_operations() -> Result<(), String> {
        let fixture = Fixture::new()?;
        let parent = fixture.0.join("parent-file");
        std::fs::write(&parent, b"owned").map_err(|e| e.to_string())?;
        let actual = std::fs::create_dir_all(&parent)
            .err()
            .ok_or("parent setup did not refuse")?;
        diagnostic(
            ensure_parent(&parent)
                .err()
                .ok_or("occupied parent accepted")?,
            "create scratch parent",
            &parent,
            actual,
        )?;
        let actual = std::fs::create_dir(&fixture.0)
            .err()
            .ok_or("root setup did not refuse")?;
        diagnostic(
            create_root(&fixture.0)
                .err()
                .ok_or("occupied root accepted")?,
            "create exclusive scratch root",
            &fixture.0,
            actual,
        )
    }

    #[test]
    fn occupied_capture_files_retain_path_and_contents() -> Result<(), String> {
        let fixture = Fixture::new()?;
        for role in ["stdout", "stderr"] {
            let path = fixture.0.join(role);
            std::fs::write(&path, b"retained").map_err(|e| e.to_string())?;
            let actual = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .err()
                .ok_or("capture setup did not refuse")?;
            diagnostic(
                exclusive_capture(&path)
                    .err()
                    .ok_or("occupied capture accepted")?,
                "create exclusive capture",
                &path,
                actual,
            )?;
            if std::fs::read(&path).map_err(|e| e.to_string())? != b"retained" {
                return Err(format!("occupied {role} was modified"));
            }
        }
        Ok(())
    }

    #[test]
    fn capture_reads_retain_missing_path_and_cap() -> Result<(), String> {
        let fixture = Fixture::new()?;
        let path = fixture.0.join("stdout");
        let actual = File::open(&path).err().ok_or("read setup did not refuse")?;
        diagnostic(
            read_capture(&path).err().ok_or("missing read accepted")?,
            "open capture for read",
            &path,
            actual,
        )?;
        let bytes = vec![b'x'; 4 * 1024 * 1024];
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        if read_capture(&path)? != bytes {
            return Err("exact capture cap changed bytes".into());
        }
        std::fs::write(&path, vec![b'x'; bytes.len() + 1]).map_err(|e| e.to_string())?;
        let error = read_capture(&path)
            .err()
            .ok_or("over-cap capture accepted")?;
        if !error.contains(&format!("{path:?}")) || !error.contains("4194304-byte capture cap") {
            return Err(error);
        }
        Ok(())
    }

    #[test]
    fn spawn_context_names_command_without_environment() -> Result<(), String> {
        let fixture = Fixture::new()?;
        let program = fixture.0.join("missing-program");
        let mut command = Command::new(&program);
        command
            .arg("--control")
            .current_dir(&fixture.0)
            .env("DIAGNOSTIC_SECRET", "must-not-appear");
        let context = spawn_context(&command, &fixture.0);
        for expected in [
            format!("{program:?}"),
            "--control".into(),
            format!("cwd=Some({:?})", fixture.0),
            format!("scratch={:?}", fixture.0),
        ] {
            if !context.contains(&expected) {
                return Err(format!("missing spawn identity {expected} in {context}"));
            }
        }
        if context.contains("DIAGNOSTIC_SECRET") || context.contains("must-not-appear") {
            return Err("spawn diagnostic exposed environment".into());
        }
        Ok(())
    }

    fn historical_pid_nonce_path(label: &str, nonce: u128) -> PathBuf {
        std::env::temp_dir().join(format!("ripr-{label}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn historical_pid_and_clock_name_collides_with_eexist() -> Result<(), String> {
        // Nonce is unique per run so an interrupted prior process that reused this
        // PID cannot fail the first exclusive create. The collision oracle is the
        // second create_root on that same pid+nonce path, not a globally fixed name.
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
            .saturating_add(NEXT.fetch_add(1, Ordering::Relaxed) as u128);
        let path = historical_pid_nonce_path("help-command", nonce);
        ensure_parent(path.parent().ok_or("historical parent missing")?)?;
        if path.exists() {
            std::fs::remove_dir_all(&path)
                .map_err(|error| format!("remove leftover historical path {path:?}: {error}"))?;
        }
        create_root(&path)?;
        let _guard = Fixture(path.clone());
        let actual = std::fs::create_dir(&path)
            .err()
            .ok_or("historical colliding name was accepted by create_dir")?;
        if actual.kind() != io::ErrorKind::AlreadyExists {
            return Err(format!(
                "historical collision kind {:?}, expected AlreadyExists",
                actual.kind()
            ));
        }
        let diagnostic = create_root(&path)
            .err()
            .ok_or("historical colliding name was accepted by create_root")?;
        for expected in [
            "create exclusive scratch root".to_string(),
            format!("{path:?}"),
            format!("kind={:?}", actual.kind()),
            format!("raw_os_error={:?}", actual.raw_os_error()),
            actual.to_string(),
        ] {
            if !diagnostic.contains(&expected) {
                return Err(format!("missing {expected:?} in {diagnostic:?}"));
            }
        }
        Ok(())
    }

    #[test]
    fn same_nonce_allocations_create_distinct_exclusive_roots() -> Result<(), String> {
        const NONCE: u128 = 11;
        let first = exclusive_scratch_root("help-command", NONCE)?;
        let _first = Fixture(first.clone());
        let second = exclusive_scratch_root("help-command", NONCE)?;
        let _second = Fixture(second.clone());
        if first == second {
            return Err(format!(
                "same-nonce allocations reused exclusive root {first:?}"
            ));
        }
        if !first.is_dir() || !second.is_dir() {
            return Err(format!(
                "same-nonce roots were not both directories: {first:?}, {second:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn concurrent_same_nonce_allocations_all_succeed() -> Result<(), String> {
        const THREADS: usize = 8;
        const NONCE: u128 = 0;
        let barrier = Barrier::new(THREADS);
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..THREADS)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        exclusive_scratch_root("help-command", NONCE)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| match handle.join() {
                    Ok(result) => result,
                    Err(_) => Err("same-nonce worker panicked".into()),
                })
                .collect::<Vec<_>>()
        });
        let mut guards = Vec::new();
        let mut paths = BTreeSet::new();
        let mut failures = Vec::new();
        for result in results {
            match result {
                Ok(path) => {
                    if !path.is_dir() {
                        failures.push(format!("same-nonce root is not a directory: {path:?}"));
                    }
                    if !paths.insert(path.clone()) {
                        failures.push(format!("duplicate exclusive root {path:?}"));
                    }
                    guards.push(Fixture(path));
                }
                Err(error) => failures.push(error),
            }
        }
        drop(guards);
        if paths.len() != THREADS {
            failures.push(format!(
                "expected {THREADS} distinct roots, got {}",
                paths.len()
            ));
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    }
}
