//! #13700: the `rust/field-change` repair-start case. A changed field of a
//! returned struct whose only test binds the owner result and never reads the
//! field starts `ripr repair --base HEAD~1` on that test. A test that already
//! reads the field strongly, or two candidate tests, get no repair. A weak
//! read through the same `use fx::owner` import starts the weak-read route;
//! a same-named rival in another module or crate never settles on the owner.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

const LIB_OLD: &str = "pub struct Config {\n    pub retries: u32,\n    pub verbose: bool,\n}\n\npub fn default_config() -> Config {\n    Config {\n        retries: 3,\n        verbose: false,\n    }\n}\n";

const UNREAD_TEST: &str = "use fx::default_config;\n\n#[test]\nfn default_config_is_quiet() {\n    let c = default_config();\n    assert!(!c.verbose);\n}\n";

struct Repo(PathBuf);

impl Repo {
    /// `LIB_OLD` + `test` committed, then `retries: 5` committed on top.
    fn create(test: &str) -> Result<Self, String> {
        Self::create_with(CARGO_TOML, LIB_OLD, test, &[])
    }

    /// `manifest`, `lib_old` (holding `retries: 3`), `test` and extra `files`
    /// committed, then `retries: 5` committed on top.
    fn create_with(
        manifest: &str,
        lib_old: &str,
        test: &str,
        files: &[(&str, &str)],
    ) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-unread-field-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
        let repo = Self(root);
        repo.write(".gitignore", "/target/\n")?;
        repo.write("Cargo.toml", manifest)?;
        repo.write("src/lib.rs", lib_old)?;
        repo.write("tests/t.rs", test)?;
        for (name, text) in files {
            if let Some(parent) = std::path::Path::new(name).parent() {
                std::fs::create_dir_all(repo.0.join(parent)).map_err(|e| e.to_string())?;
            }
            repo.write(name, text)?;
        }
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-qm", "base"])?;
        repo.write("src/lib.rs", &lib_old.replace("retries: 3", "retries: 5"))?;
        repo.git(&["commit", "-qam", "change"])?;
        Ok(repo)
    }

    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.0.join(name), text).map_err(|e| e.to_string())
    }

    fn git(&self, args: &[&str]) -> Result<(), String> {
        let output = Command::new("git")
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    fn ripr(&self, args: &[&str]) -> Result<Output, String> {
        Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())
    }

    /// `ripr check --base HEAD~1` text, then `ripr repair --base HEAD~1`
    /// (exit code, text).
    fn answers(&self) -> Result<(String, Option<i32>, String), String> {
        let check = text(&self.ripr(&["check", "--base", "HEAD~1"])?);
        let repair = self.ripr(&["repair", "--base", "HEAD~1"])?;
        Ok((check, repair.status.code(), text(&repair)))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn unread_field_of_a_bound_owner_result_starts_repair() -> Result<(), String> {
    let repo = Repo::create(UNREAD_TEST)?;
    let (check, code, repair) = repo.answers()?;
    if !check.contains("retries: 5") {
        return Err(format!("check must name the changed field:\n{check}"));
    }
    if code != Some(0)
        || !repair.contains("repair started")
        || !repair.contains("missing discriminator: retries: 5")
        || !repair.contains("Test to extend: tests/t.rs:4")
    {
        return Err(format!(
            "repair must start on the one binding test (exit {code:?}):\n{repair}"
        ));
    }
    // Check and repair agree: check recommends the same start, and the id it
    // prints starts the same repair on the same test.
    let id = check
        .lines()
        .find_map(|line| {
            let mut words = line.split_whitespace();
            (words.next() == Some("ripr") && words.next() == Some("repair"))
                .then(|| words.next().map(str::to_string))
                .flatten()
        })
        .ok_or_else(|| format!("check must print the `ripr repair <id>` start:\n{check}"))?;
    // `--base` only scopes an automatic start; a named item takes none.
    let by_id = repo.ripr(&["repair", &id])?;
    let by_id_text = text(&by_id);
    if by_id.status.code() != Some(0)
        || !by_id_text.contains("Ready to repair.")
        || !by_id_text.contains("Changed behavior: retries: 5")
        || !by_id_text.contains("Test to extend: tests/t.rs:4")
        || !by_id_text.contains(&format!("Repair ID: {id}"))
    {
        return Err(format!(
            "check's `ripr repair {id}` must start the same repair (exit {:?}):\n{by_id_text}",
            by_id.status.code()
        ));
    }
    Ok(())
}

#[test]
fn strong_read_of_the_changed_field_gets_no_repair() -> Result<(), String> {
    let repo = Repo::create(
        "use fx::default_config;\n\n#[test]\nfn default_config_retries() {\n    let c = default_config();\n    assert_eq!(c.retries, 5);\n}\n",
    )?;
    let (_, code, repair) = repo.answers()?;
    if code == Some(0) || repair.contains("repair started") {
        return Err(format!(
            "a strong field read must not start a repair (exit {code:?}):\n{repair}"
        ));
    }
    Ok(())
}

#[test]
fn two_binding_tests_get_no_repair() -> Result<(), String> {
    let repo = Repo::create(&format!(
        "{UNREAD_TEST}\n#[test]\nfn default_config_is_still_quiet() {{\n    let d = default_config();\n    assert!(!d.verbose);\n}}\n"
    ))?;
    let (_, code, repair) = repo.answers()?;
    if code == Some(0) || repair.contains("repair started") {
        return Err(format!(
            "two candidate tests must not start a repair (exit {code:?}):\n{repair}"
        ));
    }
    Ok(())
}

/// Neither `ripr repair` nor the check screen starts a repair.
fn assert_no_start(case: &str, repo: &Repo) -> Result<(), String> {
    let (check, code, repair) = repo.answers()?;
    if code == Some(0) || repair.contains("repair started") {
        return Err(format!(
            "{case}: repair must not start (exit {code:?}):\n{repair}"
        ));
    }
    if check
        .lines()
        .any(|line| line.trim_start().starts_with("ripr repair "))
    {
        return Err(format!(
            "{case}: check must not offer a repair start:\n{check}"
        ));
    }
    Ok(())
}

#[test]
fn weak_read_through_a_crate_name_import_starts_repair() -> Result<(), String> {
    // The field is read, so only the weak-read fact can start this route;
    // before #13700 the `use fx::default_config` import read as foreign.
    let repo = Repo::create(
        "use fx::default_config;\n\n#[test]\nfn default_config_retries() {\n    let c = default_config();\n    assert!(c.retries > 0);\n}\n",
    )?;
    let (_, code, repair) = repo.answers()?;
    if code != Some(0)
        || !repair.contains("repair started")
        || !repair.contains("missing discriminator: retries: 5")
        || !repair.contains("Test to extend: tests/t.rs:4")
    {
        return Err(format!(
            "a weak read through the crate-name import must start (exit {code:?}):\n{repair}"
        ));
    }
    Ok(())
}

const RIVAL_MODULE: &str = "\npub mod alt {\n    pub fn default_config() -> super::Config {\n        super::Config {\n            retries: 9,\n            verbose: false,\n        }\n    }\n}\n";

#[test]
fn rival_module_import_of_the_owner_name_gets_no_repair() -> Result<(), String> {
    for (label, body) in [
        ("unread", "assert!(!c.verbose);"),
        ("weak", "assert!(c.retries > 0);"),
    ] {
        let repo = Repo::create_with(
            CARGO_TOML,
            &format!("{LIB_OLD}{RIVAL_MODULE}"),
            &format!(
                "use fx::alt::default_config;\n\n#[test]\nfn default_config_is_quiet() {{\n    let c = default_config();\n    {body}\n}}\n"
            ),
            &[],
        )?;
        assert_no_start(&format!("rival module, {label}"), &repo)?;
    }
    Ok(())
}

#[test]
fn rival_crate_import_of_the_owner_name_gets_no_repair() -> Result<(), String> {
    let manifest = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dev-dependencies]\nother = { path = \"other\" }\n\n[workspace]\nmembers = [\".\", \"other\"]\n";
    let other_lib = "pub struct Config {\n    pub retries: u32,\n    pub verbose: bool,\n}\n\npub fn default_config() -> Config {\n    Config {\n        retries: 9,\n        verbose: false,\n    }\n}\n";
    for (label, body) in [
        ("unread", "assert!(!c.verbose);"),
        ("weak", "assert!(c.retries > 0);"),
    ] {
        let repo = Repo::create_with(
            manifest,
            LIB_OLD,
            &format!(
                "use other::default_config;\n\n#[test]\nfn default_config_is_quiet() {{\n    let c = default_config();\n    {body}\n}}\n"
            ),
            &[
                (
                    "other/Cargo.toml",
                    "[package]\nname = \"other\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                ),
                ("other/src/lib.rs", other_lib),
            ],
        )?;
        assert_no_start(&format!("rival crate, {label}"), &repo)?;
    }
    Ok(())
}
