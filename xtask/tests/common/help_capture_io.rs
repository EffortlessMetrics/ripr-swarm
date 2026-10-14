//! Operation-preserving I/O diagnostics for the bounded help-proof adapter.
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;

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
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
}
