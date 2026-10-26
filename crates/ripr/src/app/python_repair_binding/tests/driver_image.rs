//! Linux executable replacement reaches the real binding-record producer.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CHILD_MODE: &str = "RIPR_BINDING_IMAGE_TEST_CHILD";
const TEST: &str = "app::python_repair_binding::tests::driver_image::binding_retains_original_image_when_its_path_is_replaced";
const COPY_NAME: &str = "copied-driver-test";

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn child_record(mode: &str) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    if exe != cwd.join(COPY_NAME) || !matches!(mode, "unlink" | "replace") {
        return Err("image test child is not the owned executable copy".to_string());
    }
    let original = std::fs::read(&exe).map_err(|error| error.to_string())?;
    let expected = super::sha256_hex(&original);
    std::fs::remove_file(&exe).map_err(|error| error.to_string())?;
    if mode == "replace" {
        // A fallback that strips `(deleted)` would hash these unrelated bytes.
        std::fs::write(&exe, b"replacement, not the executing image")
            .map_err(|error| error.to_string())?;
    }
    let reported = std::env::current_exe().map_err(|error| error.to_string())?;
    if reported.exists() {
        return Err("replacement did not remove the running image's pathname".to_string());
    }
    let record = super::sample_record()?;
    if record
        .pointer("/driver/binary_sha256")
        .and_then(serde_json::Value::as_str)
        != Some(expected.as_str())
    {
        return Err(format!(
            "{mode}: binding did not retain the original executing image digest"
        ));
    }
    Ok(())
}

#[test]
fn binding_retains_original_image_when_its_path_is_replaced() -> Result<(), String> {
    if let Ok(mode) = std::env::var(CHILD_MODE) {
        return child_record(&mode);
    }
    let parent = std::env::temp_dir();
    std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    for mode in ["unlink", "replace"] {
        let root = parent.join(format!(
            "ripr-binding-image-{}-{nonce}-{mode}",
            std::process::id()
        ));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let scratch = Scratch(root);
        let copy = scratch.0.join(COPY_NAME);
        std::fs::copy("/proc/self/exe", &copy).map_err(|error| error.to_string())?;
        let mut command = std::process::Command::new(&copy);
        command
            .args(["--exact", TEST, "--nocapture"])
            .current_dir(&scratch.0)
            .env(CHILD_MODE, mode)
            .stdin(std::process::Stdio::null());
        let mut child = crate::process_owner::OwnedProcess::spawn(command)
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                break status;
            }
            if Instant::now() >= deadline {
                child.terminate_tree()?;
                return Err(format!(
                    "{mode}: owned image test child exceeded its deadline"
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        if !status.success() {
            return Err(format!("{mode}: owned image test child failed: {status}"));
        }
    }
    Ok(())
}
