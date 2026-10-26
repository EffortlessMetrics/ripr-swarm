//! Linux executable replacement reaches the real binding-record producer.

use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CHILD_MODE: &str = "RIPR_BINDING_IMAGE_TEST_CHILD";
const TEST: &str = "app::python_repair_binding::tests::driver_image::binding_retains_original_image_when_its_path_is_replaced";
const IMAGE_NAME: &str = "linked-driver-test";
const CHECKED_RECORD: &str = "checked-binding.json";

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn child_record(mode: &str) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    if exe != cwd.join(IMAGE_NAME) || !matches!(mode, "unlink" | "replace") {
        return Err("image test child is not the owned executable link".to_string());
    }
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
    let bytes = serde_json::to_vec(&record).map_err(|error| error.to_string())?;
    std::fs::write(cwd.join(CHECKED_RECORD), bytes).map_err(|error| error.to_string())
}

#[test]
fn binding_retains_original_image_when_its_path_is_replaced() -> Result<(), String> {
    if let Ok(mode) = std::env::var(CHILD_MODE) {
        return child_record(&mode);
    }
    let source = std::env::current_exe().map_err(|error| error.to_string())?;
    let parent = source.parent().ok_or("running test image has no parent")?;
    // Establish the oracle once, independently in the still-running parent.
    let expected =
        super::sha256_hex(&std::fs::read("/proc/self/exe").map_err(|error| error.to_string())?);
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
        let image = scratch.0.join(IMAGE_NAME);
        // This temporary link is protected by the parent's executing inode;
        // it is not a retained proof artifact. A copied executable can keep a
        // writable descriptor in a parallel fork and fail its first exec
        // (#10168 / #10211). Stay beside the image so linking uses one filesystem.
        std::fs::hard_link(&source, &image).map_err(|error| error.to_string())?;
        let running = std::fs::metadata("/proc/self/exe").map_err(|error| error.to_string())?;
        let linked = std::fs::metadata(&image).map_err(|error| error.to_string())?;
        if (running.dev(), running.ino()) != (linked.dev(), linked.ino()) {
            return Err("private link does not name the parent's executing image".to_string());
        }
        let mut command = std::process::Command::new(&image);
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
        // Exit zero with a misspelled exact filter runs zero tests. Require the
        // child's checked production record as evidence it reached the oracle.
        let record: serde_json::Value = serde_json::from_slice(
            &std::fs::read(scratch.0.join(CHECKED_RECORD))
                .map_err(|error| format!("{mode}: no checked binding record: {error}"))?,
        )
        .map_err(|error| error.to_string())?;
        if record["kind"] != super::BINDING_KIND {
            return Err(format!("{mode}: child did not produce a binding record"));
        }
        if record
            .pointer("/driver/binary_sha256")
            .and_then(serde_json::Value::as_str)
            != Some(expected.as_str())
        {
            return Err(format!(
                "{mode}: binding did not retain the original executing image digest"
            ));
        }
    }
    Ok(())
}
