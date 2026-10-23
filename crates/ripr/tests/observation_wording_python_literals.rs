//! Real Python producer/render control for the human observation wording
//! (#9244, RIPR-SPEC-0122): source text the Python adapter embeds in its
//! `missing proof` summary must print exactly as written in the human Evidence
//! row, including a single (unpaired) backtick before the machine token. Only
//! the producer-appended marker is ever removed.
//!
//! The python_adversarial_error_path_untaken_branch fixture shape is reused
//! with the raised message replaced by the token. A build without
//! `lang-python` cannot observe this adapter path.

#![cfg(feature = "lang-python")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "def parse(text):
    if not text:
        raise KeyError(\"MESSAGE\")
    return int(text)
";

const DIFF: &str = "diff --git a/src/parseint.py b/src/parseint.py
index 0000000..1111111 100644
--- a/src/parseint.py
+++ b/src/parseint.py
@@ -1,4 +1,4 @@
 def parse(text):
     if not text:
-        raise ValueError(\"empty\")
+        raise KeyError(\"MESSAGE\")
     return int(text)
";

const TESTS: &str = "from src.parseint import parse


def test_parse_ok():
    assert parse(\"12\") == 12
";

fn render_human_full(message: &str) -> Result<String, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root: PathBuf = std::env::temp_dir().join(format!(
        "ripr-observation-wording-py-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let write = |path: &str, text: &str| -> Result<(), String> {
        let file = root.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("mkdir {path}: {error}"))?;
        }
        std::fs::write(&file, text).map_err(|error| format!("write {path}: {error}"))
    };
    write(
        "ripr.toml",
        "[languages]\nenabled = [\"rust\", \"python\"]\n",
    )?;
    write("src/parseint.py", &SOURCE.replace("MESSAGE", message))?;
    write("tests/test_parseint.py", TESTS)?;
    write("diff.patch", &DIFF.replace("MESSAGE", message))?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["check", "--root"])
        .arg(&root)
        .arg("--diff")
        .arg(root.join("diff.patch"))
        .args(["--mode", "fast", "--format", "human-full"])
        .output()
        .map_err(|error| format!("run ripr: {error}"));
    let result = output.map(|out| String::from_utf8_lossy(&out.stdout).into_owned());
    let _ = std::fs::remove_dir_all(&root);
    result
}

#[test]
fn single_backtick_before_the_token_prints_as_written() -> Result<(), String> {
    let text = render_human_full("`observation_unverified")?;
    assert!(
        text.contains("missing proof: `raises KeyError matching \"`observation_unverified\"`"),
        "producer missing-proof text did not carry the source verbatim:\n{text}"
    );
    assert!(
        !text.contains("the observation is unconfirmed"),
        "embedded source was rewritten:\n{text}"
    );
    Ok(())
}

#[test]
fn bare_token_message_prints_as_written() -> Result<(), String> {
    let text = render_human_full("observation_unverified")?;
    assert!(
        text.contains("missing proof: `raises KeyError matching \"observation_unverified\"`"),
        "{text}"
    );
    assert!(!text.contains("the observation is unconfirmed"), "{text}");
    Ok(())
}
