//! Discriminators for optional `[[overrides]]` rows (#2671).

use super::*;
use crate::domain::ExposureClass;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_root(name: &str) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!("ripr-overrides-{name}-{stamp}"));
    fs::create_dir_all(&root).map_err(|err| format!("create temp root failed: {err}"))?;
    Ok(root)
}

fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create {} failed: {err}", parent.display()))?;
    }
    fs::write(path, text).map_err(|err| format!("write {} failed: {err}", path.display()))
}

fn write_package(root: &Path, dir: &str, name: &str) -> Result<(), String> {
    write_file(
        &root.join(dir).join("Cargo.toml"),
        &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n"),
    )
}

fn two_crate_workspace(name: &str) -> Result<PathBuf, String> {
    let root = temp_root(name)?;
    write_file(
        &root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/alpha\", \"crates/beta\"]\n",
    )?;
    write_package(&root, "crates/alpha", "alpha")?;
    write_package(&root, "crates/beta", "beta")?;
    Ok(root)
}

#[test]
fn omitted_overrides_keep_repo_wide_severity_byte_identical() -> Result<(), String> {
    let without = tests_only_parse("[severity.findings]\nweakly_exposed = \"warning\"\n")?;
    let empty_array =
        tests_only_parse("overrides = []\n\n[severity.findings]\nweakly_exposed = \"warning\"\n")?;
    let path = "crates/alpha/src/lib.rs";
    if without.severity_for(None, path) != empty_array.severity_for(None, path) {
        return Err("an empty overrides array must keep repo-wide severity".to_string());
    }
    if check_artifact_config_identity_hash(&without)
        != check_artifact_config_identity_hash(&empty_array)
    {
        return Err("an empty overrides array must keep the config identity".to_string());
    }
    if without.oracles_for(None, path) != empty_array.oracles_for(None, path) {
        return Err("an empty overrides array must keep repo-wide oracle policy".to_string());
    }
    Ok(())
}

#[test]
fn two_path_overrides_yield_different_severity() -> Result<(), String> {
    let config = tests_only_parse(
        "[severity.findings]\nweakly_exposed = \"warning\"\n\n\
         [[overrides]]\npath = \"crates/alpha/**\"\nseverity.findings.weakly_exposed = \"info\"\n\n\
         [[overrides]]\npath = \"crates/beta/**\"\nseverity.findings.weakly_exposed = \"note\"\n",
    )?;
    let alpha = config
        .severity_for(None, "crates/alpha/src/lib.rs")
        .for_exposure(&ExposureClass::WeaklyExposed);
    let beta = config
        .severity_for(None, "crates/beta/src/lib.rs")
        .for_exposure(&ExposureClass::WeaklyExposed);
    let other = config
        .severity_for(None, "crates/gamma/src/lib.rs")
        .for_exposure(&ExposureClass::WeaklyExposed);
    if alpha != ConfigSeverity::Info {
        return Err(format!("alpha expected info, got {}", alpha.as_str()));
    }
    if beta != ConfigSeverity::Note {
        return Err(format!("beta expected note, got {}", beta.as_str()));
    }
    if other != ConfigSeverity::Warning {
        return Err(format!(
            "unmatched crate expected repo-wide warning, got {}",
            other.as_str()
        ));
    }
    Ok(())
}

#[test]
fn unknown_crate_name_is_a_config_diagnostic() -> Result<(), String> {
    let root = two_crate_workspace("unknown-crate")?;
    write_file(
        &root.join("ripr.toml"),
        "[[overrides]]\ncrate = \"missing\"\nseverity.findings.weakly_exposed = \"info\"\n",
    )?;
    let error = match load_for_root(&root) {
        Ok(_) => return Err("unknown crate must fail closed".to_string()),
        Err(error) => error,
    };
    let _ = fs::remove_dir_all(&root);
    if !error.contains("[[overrides]] crate = \"missing\"") {
        return Err(format!("diagnostic must name the crate: {error}"));
    }
    if !error.contains("does not match a Cargo package") {
        return Err(format!(
            "diagnostic must fail closed, not silently: {error}"
        ));
    }
    if !(error.contains("alpha") && error.contains("beta")) {
        return Err(format!("diagnostic must name known packages: {error}"));
    }
    Ok(())
}

#[test]
fn crate_name_outranks_overlapping_path_glob() -> Result<(), String> {
    let root = two_crate_workspace("name-vs-path")?;
    write_file(
        &root.join("ripr.toml"),
        "[severity.findings]\nweakly_exposed = \"warning\"\n\n\
         [[overrides]]\npath = \"crates/**\"\nseverity.findings.weakly_exposed = \"note\"\n\n\
         [[overrides]]\ncrate = \"alpha\"\nseverity.findings.weakly_exposed = \"info\"\n",
    )?;
    let config = load_for_root(&root)?;
    let _ = fs::remove_dir_all(&root);
    let alpha = config
        .severity_for(Some("alpha"), "crates/alpha/src/lib.rs")
        .for_exposure(&ExposureClass::WeaklyExposed);
    let beta = config
        .severity_for(Some("beta"), "crates/beta/src/lib.rs")
        .for_exposure(&ExposureClass::WeaklyExposed);
    if alpha != ConfigSeverity::Info {
        return Err(format!(
            "crate name must outrank a broader path glob, got {}",
            alpha.as_str()
        ));
    }
    if beta != ConfigSeverity::Note {
        return Err(format!(
            "beta should keep the path glob, got {}",
            beta.as_str()
        ));
    }
    Ok(())
}

#[test]
fn bound_crate_name_matches_package_directory() -> Result<(), String> {
    let root = two_crate_workspace("bind-crate")?;
    write_file(
        &root.join("ripr.toml"),
        "[[overrides]]\ncrate = \"beta\"\nseverity.findings.weakly_exposed = \"note\"\n",
    )?;
    let config = load_for_root(&root)?;
    let _ = fs::remove_dir_all(&root);
    let via_path = config
        .severity_for_file(Path::new("crates/beta/src/lib.rs"))
        .for_exposure(&ExposureClass::WeaklyExposed);
    if via_path != ConfigSeverity::Note {
        return Err(format!(
            "bound crate selector must match package files, got {}",
            via_path.as_str()
        ));
    }
    Ok(())
}

#[test]
fn both_crate_and_path_on_one_row_is_rejected() -> Result<(), String> {
    let error = match tests_only_parse(
        "[[overrides]]\ncrate = \"alpha\"\npath = \"crates/alpha/**\"\nseverity.findings.exposed = \"info\"\n",
    ) {
        Ok(_) => return Err("crate and path together must fail".to_string()),
        Err(error) => error,
    };
    if !error.contains("exactly one of crate or path") {
        return Err(error);
    }
    Ok(())
}

#[test]
fn override_without_severity_or_oracles_is_rejected() -> Result<(), String> {
    let error = match tests_only_parse("[[overrides]]\npath = \"crates/alpha/**\"\n") {
        Ok(_) => return Err("empty override must fail".to_string()),
        Err(error) => error,
    };
    if !error.contains("must override severity or oracles") {
        return Err(error);
    }
    Ok(())
}

#[test]
fn two_path_overrides_yield_different_oracle_strength() -> Result<(), String> {
    let config = tests_only_parse(
        "[oracles]\nsnapshot_strength = \"medium\"\n\n\
         [[overrides]]\npath = \"crates/alpha/**\"\noracles.snapshot_strength = \"strong\"\n\n\
         [[overrides]]\npath = \"crates/beta/**\"\noracles.snapshot_strength = \"weak\"\n",
    )?;
    let alpha = config
        .oracles_for(None, "crates/alpha/src/lib.rs")
        .snapshot_strength()
        .clone();
    let beta = config
        .oracles_for(None, "crates/beta/src/lib.rs")
        .snapshot_strength()
        .clone();
    if alpha != crate::domain::OracleStrength::Strong {
        return Err(format!(
            "alpha snapshot expected strong, got {}",
            alpha.as_str()
        ));
    }
    if beta != crate::domain::OracleStrength::Weak {
        return Err(format!(
            "beta snapshot expected weak, got {}",
            beta.as_str()
        ));
    }
    Ok(())
}

#[test]
fn unknown_override_key_fails_closed() -> Result<(), String> {
    let error =
        match tests_only_parse("[[overrides]]\npath = \"crates/alpha/**\"\nmode = \"deep\"\n") {
            Ok(_) => return Err("unknown override key must fail".to_string()),
            Err(error) => error,
        };
    if !error.contains("unknown field") {
        return Err(error);
    }
    Ok(())
}

#[test]
fn reversed_equal_oracle_rows_change_identity() -> Result<(), String> {
    let first = tests_only_parse(
        "[[overrides]]\npath = \"src/**\"\noracles.snapshot_strength = \"strong\"\n\n\
         [[overrides]]\npath = \"src/**\"\noracles.snapshot_strength = \"weak\"\n",
    )?;
    let reversed = tests_only_parse(
        "[[overrides]]\npath = \"src/**\"\noracles.snapshot_strength = \"weak\"\n\n\
         [[overrides]]\npath = \"src/**\"\noracles.snapshot_strength = \"strong\"\n",
    )?;
    let path = "src/lib.rs";
    if first.oracles_for(None, path).snapshot_strength()
        == reversed.oracles_for(None, path).snapshot_strength()
    {
        return Err("later equally specific oracle row must win".to_string());
    }
    if check_artifact_config_identity_hash(&first) == check_artifact_config_identity_hash(&reversed)
    {
        return Err("reordering overlapping oracle rows must change identity".to_string());
    }
    Ok(())
}

#[test]
fn member_root_load_accepts_sibling_crate_override() -> Result<(), String> {
    let root = two_crate_workspace("member-root")?;
    write_file(
        &root.join("ripr.toml"),
        "[[overrides]]\ncrate = \"beta\"\nseverity.findings.weakly_exposed = \"note\"\n",
    )?;
    let config = load_for_root(&root.join("crates/alpha"))?;
    let _ = fs::remove_dir_all(&root);
    let beta = config
        .severity_for_file(Path::new("crates/beta/src/lib.rs"))
        .for_exposure(&ExposureClass::WeaklyExposed);
    if beta != ConfigSeverity::Note {
        return Err(format!(
            "sibling crate named from a member root must bind, got {}",
            beta.as_str()
        ));
    }
    Ok(())
}

#[test]
fn nested_member_glob_binds_crate_name() -> Result<(), String> {
    let root = temp_root("nested-glob")?;
    write_file(
        &root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*/plugins/*\"]\n",
    )?;
    write_package(&root, "crates/team/plugins/parser", "parser")?;
    write_file(
        &root.join("ripr.toml"),
        "[[overrides]]\ncrate = \"parser\"\nseverity.findings.weakly_exposed = \"info\"\n",
    )?;
    let config = load_for_root(&root)?;
    let _ = fs::remove_dir_all(&root);
    let severity = config
        .severity_for_file(Path::new("crates/team/plugins/parser/src/lib.rs"))
        .for_exposure(&ExposureClass::WeaklyExposed);
    if severity != ConfigSeverity::Info {
        return Err(format!(
            "nested member glob must bind parser, got {}",
            severity.as_str()
        ));
    }
    Ok(())
}

#[test]
fn nested_package_does_not_inherit_parent_crate_override() -> Result<(), String> {
    let root = temp_root("nested-package")?;
    write_file(
        &root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/alpha\", \"crates/alpha/plugin\"]\n",
    )?;
    write_package(&root, "crates/alpha", "alpha")?;
    write_package(&root, "crates/alpha/plugin", "plugin")?;
    write_file(
        &root.join("ripr.toml"),
        "[severity.findings]\nweakly_exposed = \"warning\"\n\n\
         [[overrides]]\ncrate = \"alpha\"\nseverity.findings.weakly_exposed = \"info\"\n",
    )?;
    let config = load_for_root(&root)?;
    let _ = fs::remove_dir_all(&root);
    let parent = config
        .severity_for_file(Path::new("crates/alpha/src/lib.rs"))
        .for_exposure(&ExposureClass::WeaklyExposed);
    let nested = config
        .severity_for_file(Path::new("crates/alpha/plugin/src/lib.rs"))
        .for_exposure(&ExposureClass::WeaklyExposed);
    if parent != ConfigSeverity::Info {
        return Err(format!(
            "parent crate expected info, got {}",
            parent.as_str()
        ));
    }
    if nested != ConfigSeverity::Warning {
        return Err(format!(
            "nested package must keep repo-wide warning, got {}",
            nested.as_str()
        ));
    }
    Ok(())
}

#[test]
fn listed_manifests_bind_candidate_crate_and_reject_unknown() -> Result<(), String> {
    let manifests = vec![
        (
            String::new(),
            "[workspace]\nmembers = [\"crates/alpha\"]\n".to_string(),
        ),
        (
            "crates/alpha".to_string(),
            "[package]\nname = \"alpha\"\nversion = \"0.1.0\"\n".to_string(),
        ),
    ];
    let crates = overrides::crates_from_listed_manifests(&manifests);
    let mut ok = tests_only_parse(
        "[[overrides]]\ncrate = \"alpha\"\noracles.snapshot_strength = \"strong\"\n",
    )?;
    ok.bind_workspace_crates(crates.clone())?;
    if ok
        .oracles_for(None, "crates/alpha/src/lib.rs")
        .snapshot_strength()
        .clone()
        != crate::domain::OracleStrength::Strong
    {
        return Err("candidate-listed crate selector must apply".to_string());
    }
    let mut missing = tests_only_parse(
        "[[overrides]]\ncrate = \"missing\"\noracles.snapshot_strength = \"strong\"\n",
    )?;
    let error = match missing.bind_workspace_crates(crates) {
        Ok(_) => return Err("unknown candidate crate must fail closed".to_string()),
        Err(error) => error,
    };
    if !error.contains("crate = \"missing\"") {
        return Err(format!("unknown candidate crate diagnostic: {error}"));
    }
    Ok(())
}

#[test]
fn oracle_only_row_does_not_shadow_a_less_specific_severity_row() -> Result<(), String> {
    let config = tests_only_parse(
        "[severity.findings]\nweakly_exposed = \"warning\"\n\n\
         [[overrides]]\npath = \"crates/**\"\nseverity.findings.weakly_exposed = \"note\"\n\n\
         [[overrides]]\npath = \"crates/alpha/**\"\noracles.snapshot_strength = \"weak\"\n",
    )?;
    let path = "crates/alpha/src/lib.rs";
    let direct = config
        .severity_for(None, path)
        .for_exposure(&ExposureClass::WeaklyExposed);
    let compiled = config
        .severity
        .for_exposure_in(&ExposureClass::WeaklyExposed, Path::new(path));
    if direct != ConfigSeverity::Note {
        return Err(format!(
            "severity_for must keep the crates/** severity row, got {}",
            direct.as_str()
        ));
    }
    if direct != compiled {
        return Err(format!(
            "severity_for ({}) and the compiled overlay path ({}) must agree",
            direct.as_str(),
            compiled.as_str()
        ));
    }
    Ok(())
}
