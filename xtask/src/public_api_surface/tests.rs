use std::path::Path;

use super::public_api_surface;
use crate::tests::{temp_dir, write};

#[test]
fn records_a_pub_use_inside_an_allowlisted_module() -> Result<(), String> {
    // Discriminating fixture for #3052. The `pub use` sits in `domain/mod.rs`,
    // a file the previous line-prefix collector never opened.
    let dir = temp_dir("public-api-transitive");
    write(
        &dir.join("lib.rs"),
        "pub mod domain;\npub(crate) mod internal;\n",
    );
    write(
        &dir.join("domain/mod.rs"),
        "mod probe;\n\
         pub use probe::MISSING_DISCRIMINATOR_VALUE_PREFIX;\n\
         pub(crate) const CRATE_ONLY: &str = \"x\";\n\
         #[cfg(test)]\nmod tests {\n    pub const TEST_ONLY: &str = \"x\";\n}\n",
    );
    write(
        &dir.join("domain/probe.rs"),
        "pub const MISSING_DISCRIMINATOR_VALUE_PREFIX: &str = \"m\";\n",
    );
    write(
        &dir.join("internal.rs"),
        "pub const UNREACHABLE: &str = \"x\";\n",
    );

    let surface = public_api_surface(&dir.join("lib.rs"), "ripr")?;

    if !surface
        .iter()
        .any(|entry| entry == "pub use ripr::domain::MISSING_DISCRIMINATOR_VALUE_PREFIX")
    {
        return Err(format!("transitive re-export missing: {surface:?}"));
    }
    // Discriminate against a naive "grep every .rs file" widening: each of
    // these fails for a different wrong collector.
    for forbidden in ["CRATE_ONLY", "TEST_ONLY", "UNREACHABLE"] {
        if surface.iter().any(|entry| entry.contains(forbidden)) {
            return Err(format!(
                "non-public item recorded ({forbidden}): {surface:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn rejects_every_restricted_visibility() -> Result<(), String> {
    let dir = temp_dir("public-api-visibility");
    write(
        &dir.join("lib.rs"),
        "pub mod outer;\n\
         pub const ROOT_PUBLIC: u8 = 0;\n\
         pub(crate) const ROOT_CRATE: u8 = 0;\n",
    );
    write(
        &dir.join("outer/mod.rs"),
        "pub(crate) const A: u8 = 0;\n\
         pub(super) const B: u8 = 0;\n\
         pub(self) const C: u8 = 0;\n\
         pub(in crate::outer) const D: u8 = 0;\n\
         const E: u8 = 0;\n\
         pub const VISIBLE: u8 = 0;\n",
    );

    let surface = public_api_surface(&dir.join("lib.rs"), "ripr")?;
    let expected = vec![
        "pub const ripr::ROOT_PUBLIC".to_string(),
        "pub const ripr::outer::VISIBLE".to_string(),
        "pub mod ripr::outer".to_string(),
    ];
    if surface != expected {
        return Err(format!("expected {expected:?}, got {surface:?}"));
    }
    Ok(())
}

#[test]
fn records_every_module_level_item_kind() -> Result<(), String> {
    let dir = temp_dir("public-api-kinds");
    write(
        &dir.join("lib.rs"),
        "pub fn a() {}\n\
         pub struct B;\n\
         pub enum C { V }\n\
         pub trait D {}\n\
         pub type E = u8;\n\
         pub const F: u8 = 0;\n\
         pub static G: u8 = 0;\n\
         pub union H { x: u8 }\n\
         impl B { pub fn associated() {} }\n",
    );

    let surface = public_api_surface(&dir.join("lib.rs"), "ripr")?;
    let expected = vec![
        "pub const ripr::F".to_string(),
        "pub enum ripr::C".to_string(),
        "pub fn ripr::a".to_string(),
        "pub static ripr::G".to_string(),
        "pub struct ripr::B".to_string(),
        "pub trait ripr::D".to_string(),
        "pub type ripr::E".to_string(),
        "pub union ripr::H".to_string(),
    ];
    // `associated` is deliberately absent: associated items are outside this
    // collector's stated module-level scope.
    if surface != expected {
        return Err(format!("expected {expected:?}, got {surface:?}"));
    }
    Ok(())
}

#[test]
fn expands_grouped_and_renamed_use_trees() -> Result<(), String> {
    let dir = temp_dir("public-api-use-trees");
    write(
        &dir.join("lib.rs"),
        "mod app;\n\
         pub use app::{\n    CheckInput,\n    CheckOutput,\n    check_workspace,\n};\n\
         pub use app::Mode as RunMode;\n\
         pub use app::nested::{self, Leaf};\n\
         use app::PrivateImport;\n",
    );
    write(&dir.join("app.rs"), "pub mod nested { pub struct Leaf; }\n");

    let surface = public_api_surface(&dir.join("lib.rs"), "ripr")?;
    let expected = vec![
        "pub use ripr::CheckInput".to_string(),
        "pub use ripr::CheckOutput".to_string(),
        "pub use ripr::Leaf".to_string(),
        "pub use ripr::RunMode".to_string(),
        "pub use ripr::check_workspace".to_string(),
        "pub use ripr::nested".to_string(),
    ];
    // The group is expanded to one entry per bound name, so the old
    // `pub use app::{` truncation cannot recur; the non-`pub` import is absent.
    if surface != expected {
        return Err(format!("expected {expected:?}, got {surface:?}"));
    }
    Ok(())
}

#[test]
fn records_a_glob_reexport_rather_than_dropping_it() -> Result<(), String> {
    let dir = temp_dir("public-api-glob");
    write(&dir.join("lib.rs"), "mod app;\npub use app::*;\n");
    write(&dir.join("app.rs"), "pub struct Hidden;\n");

    let surface = public_api_surface(&dir.join("lib.rs"), "ripr")?;
    if surface != vec!["pub use ripr::*".to_string()] {
        return Err(format!("glob not recorded: {surface:?}"));
    }
    Ok(())
}

#[test]
fn keeps_feature_gated_and_cfg_not_test_items() -> Result<(), String> {
    let dir = temp_dir("public-api-cfg");
    write(
        &dir.join("lib.rs"),
        "#[cfg(feature = \"lang-typescript\")]\npub const FEATURE_GATED: u8 = 0;\n\
         #[cfg(not(test))]\npub const NON_TEST: u8 = 0;\n\
         #[cfg(any(test, feature = \"x\"))]\npub const TEST_OR_FEATURE: u8 = 0;\n",
    );

    let surface = public_api_surface(&dir.join("lib.rs"), "ripr")?;
    let expected = vec![
        "pub const ripr::FEATURE_GATED".to_string(),
        "pub const ripr::NON_TEST".to_string(),
    ];
    // The baseline is feature-independent, and `#[cfg(not(test))]` items are
    // present in a normal build. `#[cfg(any(test, ...))]` is test-conditional
    // and dropped.
    if surface != expected {
        return Err(format!("expected {expected:?}, got {surface:?}"));
    }
    Ok(())
}

#[test]
fn fails_closed_on_an_unresolvable_pub_module() -> Result<(), String> {
    let dir = temp_dir("public-api-unresolvable");
    write(&dir.join("lib.rs"), "pub mod missing;\n");

    match public_api_surface(&dir.join("lib.rs"), "ripr") {
        Ok(surface) => Err(format!("expected an error, got {surface:?}")),
        Err(message) => {
            if message.contains("pub mod missing;") {
                Ok(())
            } else {
                Err(format!("error does not name the module: {message}"))
            }
        }
    }
}

#[test]
fn follows_a_path_attribute_on_a_public_module() -> Result<(), String> {
    let dir = temp_dir("public-api-path-attr");
    write(
        &dir.join("lib.rs"),
        "#[path = \"relocated/elsewhere.rs\"]\npub mod moved;\n",
    );
    write(
        &dir.join("relocated/elsewhere.rs"),
        "pub const RELOCATED: u8 = 0;\n",
    );

    let surface = public_api_surface(&dir.join("lib.rs"), "ripr")?;
    let expected = vec![
        "pub const ripr::moved::RELOCATED".to_string(),
        "pub mod ripr::moved".to_string(),
    ];
    if surface != expected {
        return Err(format!("expected {expected:?}, got {surface:?}"));
    }
    Ok(())
}

#[test]
fn records_the_real_crate_surface_outside_the_root_file() -> Result<(), String> {
    // Binds the collector to the actual product crate: each of these is
    // publicly reachable today and invisible to the previous collector.
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest has no parent directory".to_string())?;
    let surface = public_api_surface(&repo_root.join("crates/ripr/src/lib.rs"), "ripr")?;

    for expected in [
        // Defect 1: an item in a file the old collector never opened.
        "pub const ripr::output::start_here_state::START_HERE_CLEAN",
        // Defect 2: a root-level `pub fn` matched by neither line prefix.
        "pub fn ripr::set_verbose",
        // Defect 3: a name lost inside a truncated `pub use app::{` group.
        "pub use ripr::CheckInput",
    ] {
        if !surface.iter().any(|entry| entry == expected) {
            return Err(format!("missing {expected} in {} entries", surface.len()));
        }
    }
    // A `pub(crate)` module in the real crate must not appear.
    if surface
        .iter()
        .any(|entry| entry.starts_with("pub mod ripr::agent"))
    {
        return Err("pub(crate) mod agent recorded as public".to_string());
    }
    Ok(())
}
