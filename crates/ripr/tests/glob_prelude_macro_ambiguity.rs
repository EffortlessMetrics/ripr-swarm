//! rustc backs the binding scan's list of trusted macro names a glob import
//! cannot rebind (#16060): a bare call whose name a glob and the `macro_use`
//! prelude both supply fails to compile with E0659, so a compiling crate's
//! bare call of such a name is the standard macro.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SCAN_SOURCE: &str = include_str!("../src/analysis/syntax/owner_pin.rs");

/// The names as the scan declares them, read from its source so the probe
/// follows every edit to the list.
fn unrebindable_names() -> Vec<&'static str> {
    let start = "const GLOB_UNREBINDABLE_MACRO_NAMES: &[&str] = &[";
    let Some(list) = SCAN_SOURCE
        .split_once(start)
        .and_then(|(_, rest)| rest.split_once("];"))
        .map(|(list, _)| list)
    else {
        return Vec::new();
    };
    list.split(',')
        .map(|item| item.trim().trim_matches('"'))
        .filter(|item| !item.is_empty())
        .collect()
}

/// A crate whose module defines and re-exports each name, glob-imported at
/// file level, with one bare call of each name.
fn probe(names: &[&str], no_std: bool) -> String {
    let mut source = String::new();
    if no_std {
        source.push_str("#![no_std]\n");
    }
    source.push_str("mod m {\n");
    for name in names {
        source.push_str(&format!(
            "    macro_rules! {name} {{ ($($t:tt)*) => {{ () }} }}\n    pub(crate) use {name};\n"
        ));
    }
    source.push_str("}\nuse m::*;\npub fn calls() {\n");
    for name in names {
        source.push_str(&format!("    let _ = {name}!(x);\n"));
    }
    source.push_str("}\n");
    source
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Result<Self, String> {
        let directory =
            std::env::temp_dir().join(format!("ripr-glob-prelude-{label}-{}", std::process::id()));
        std::fs::create_dir(&directory).map_err(|error| error.to_string())?;
        Ok(Self(directory))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            eprintln!("cleanup of {} failed: {error}", self.0.display());
        }
    }
}

/// Compiles `source` as a 2021 library; returns success and stderr.
fn compile(source: &str, label: &str) -> Result<(bool, String), String> {
    let scratch = Scratch::new(label)?;
    let file = scratch.0.join("lib.rs");
    std::fs::write(&file, source).map_err(|error| error.to_string())?;
    let stderr_path = scratch.0.join("stderr");
    let mut command = Command::new("rustc");
    command
        .args(["--edition=2021", "--crate-type=lib", "--emit=metadata"])
        .arg(&file)
        .arg("--out-dir")
        .arg(&scratch.0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&stderr_path).map_err(|error| error.to_string())?);
    let mut child =
        ripr::process_owner::OwnedProcess::spawn(command).map_err(|error| error.to_string())?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() >= Duration::from_mins(2) {
            child.terminate_tree()?;
            return Err(format!("rustc for {label} exceeded two minutes"));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stderr = read_capped(&stderr_path)?;
    Ok((status.success(), stderr))
}

fn read_capped(path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    Ok(text.chars().take(256 * 1024).collect())
}

#[test]
fn a_glob_cannot_rebind_a_bare_call_of_a_listed_name() -> Result<(), String> {
    let names = unrebindable_names();
    // Fixture admission: the list is read from the scan and holds the
    // assertions the scan credits.
    assert!(names.len() >= 20, "{names:?}");
    assert!(names.contains(&"assert_eq") && names.contains(&"assert"));
    for no_std in [false, true] {
        let (compiled, stderr) =
            compile(&probe(&names, no_std), if no_std { "core" } else { "std" })?;
        assert!(!compiled, "no_std={no_std}: the glob probe compiled");
        for name in &names {
            assert!(
                stderr.contains(&format!("`{name}` is ambiguous")),
                "no_std={no_std}: {name} is not refused as ambiguous:\n{stderr}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_glob_can_supply_a_std_only_name_under_no_std() -> Result<(), String> {
    // The scan keeps these names rebindable: without the std prelude a glob
    // supplies them and the crate compiles.
    let std_only = [
        "vec", "format", "dbg", "print", "println", "eprint", "eprintln",
    ];
    let listed = unrebindable_names();
    assert!(
        std_only.iter().all(|name| !listed.contains(name)),
        "{listed:?}"
    );
    let (compiled, stderr) = compile(&probe(&std_only, true), "alloc")?;
    assert!(compiled, "{stderr}");
    let (compiled, stderr) = compile(&probe(&std_only, false), "std-only")?;
    assert!(
        !compiled && stderr.contains("`vec` is ambiguous"),
        "{stderr}"
    );
    Ok(())
}
