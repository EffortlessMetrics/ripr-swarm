//! #9564: the owner-wide kill-on-close job only protects users if the shipped
//! binaries actually bind it. The unit tests in `process_owner` exercise
//! `bind_process_lifetime` directly, so deleting a call site would fail none of
//! them. These checks pin both call sites and their order.

use std::path::PathBuf;

fn read_from_crate_root(relative: &str) -> Result<String, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path).map_err(|err| format!("read {}: {err}", path.display()))
}

/// Byte offset of the first non-comment line containing `needle`.
fn first_code_offset(source: &str, needle: &str) -> Option<usize> {
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        if !line.trim_start().starts_with("//")
            && let Some(at) = line.find(needle)
        {
            return Some(offset + at);
        }
        offset += line.len();
    }
    None
}

#[test]
fn ripr_main_binds_the_process_lifetime_before_running() -> Result<(), String> {
    let main = read_from_crate_root("src/main.rs")?;
    let bind = first_code_offset(&main, "ripr::process_owner::bind_process_lifetime()")
        .ok_or("ripr main must call bind_process_lifetime")?;
    let run = first_code_offset(&main, "run_startup(startup::run)")
        .ok_or("ripr main must run the startup path")?;
    assert!(bind < run, "bind_process_lifetime must run before startup");
    Ok(())
}

#[test]
fn xtask_main_binds_the_process_lifetime_before_the_driver_bootstrap() -> Result<(), String> {
    let main = read_from_crate_root("../../xtask/src/main.rs")?;
    let bind = first_code_offset(&main, "ripr::process_owner::bind_process_lifetime()")
        .ok_or("xtask main must call bind_process_lifetime")?;
    let bootstrap = first_code_offset(&main, "driver::bootstrap()")
        .ok_or("xtask main must call driver::bootstrap")?;
    assert!(
        bind < bootstrap,
        "bind must precede the bootstrap so the relocated driver child joins the job"
    );
    Ok(())
}
