//! Bounded identity authority for QuickCheck stimulus (#11545).
//!
//! The first slice admits conventional single-package crate roots only.
//! A trait name, nearest manifest, alias, or unexpanded macro alone is not
//! identity. Unsupported layouts retain their ordinary analysis; no role is
//! inferred from a function named `arbitrary`.

use super::{FunctionContainer, FunctionFact, FunctionSourceRole, RustIndex};
use ra_ap_syntax::{AstNode, ast};
use std::collections::BTreeSet;
use std::path::PathBuf;

pub(super) fn is_generator(function: &FunctionFact) -> bool {
    matches!(&function.item.container,
        FunctionContainer::TraitImpl { trait_path, .. }
        if matches!(trait_path.as_str(), "quickcheck::Arbitrary" | "::quickcheck::Arbitrary"))
}

/// Line coordinates cannot distinguish a generator from a production subject
/// sharing that line. Never grant the generator interpretation in that case.
pub(crate) fn is_generator_subject(index: &RustIndex, path: &std::path::Path, line: usize) -> bool {
    let Some(facts) = index.files.get(path) else {
        return false;
    };
    let mut found = false;
    for id in &facts.functions {
        let function = &index.function_facts[*id];
        if function.start_line <= line && line <= function.end_line {
            // Other item kinds have no FunctionFact. Code outside the method
            // can share either boundary line, so only its interior is known.
            if function.source_role != FunctionSourceRole::TestInputGenerator
                || line == function.start_line
                || line == function.end_line
            {
                return false;
            }
            found = true;
        }
    }
    found
}

fn sole_provider_dependency(manifest: &toml::Value) -> bool {
    // A differently keyed dependency can export a library named quickcheck.
    // This slice does not resolve other edges, so it cannot rule that out.
    if [
        "target",
        "dev-dependencies",
        "dev_dependencies",
        "build-dependencies",
        "build_dependencies",
    ]
    .iter()
    .any(|key| manifest.get(*key).is_some())
    {
        return false;
    }
    let Some(dependencies) = manifest.get("dependencies").and_then(toml::Value::as_table) else {
        return false;
    };
    dependencies.len() == 1
        && dependencies.get("quickcheck").is_some_and(|spec| {
            spec.get("optional")
                .is_none_or(|optional| optional.as_bool() == Some(false))
        })
}

pub(super) fn verified_files(index: &RustIndex) -> BTreeSet<PathBuf> {
    let mut verified = BTreeSet::new();
    let Some(root) = index.drop_in_manifests.root() else {
        return verified;
    };
    // Do not add manifest/configuration work to indexes without a candidate.
    if !index.function_facts.iter().any(is_generator) {
        return verified;
    }
    let Some(manifest) = std::fs::read_to_string(root.join("Cargo.toml"))
        .ok()
        .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
    else {
        return verified;
    };
    let package = manifest.get("package");
    let edition = package
        .and_then(|package| package.get("edition"))
        .and_then(toml::Value::as_str);
    if !matches!(edition, Some("2018" | "2021" | "2024"))
        || !sole_provider_dependency(&manifest)
        || manifest
            .get("workspace")
            .is_some_and(|workspace| !workspace.as_table().is_some_and(toml::Table::is_empty))
        || package
            .and_then(|package| package.get("workspace"))
            .is_some()
        || package
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
            == Some("quickcheck")
        || manifest.get("lib").is_some_and(|lib| {
            lib.get("path").is_some()
                || lib.get("name").and_then(toml::Value::as_str) == Some("quickcheck")
        })
        || ["bin", "test", "bench", "example"]
            .iter()
            .any(|key| manifest.get(*key).is_some())
        || crate::analysis::workspace::config_snapshot_identity(root).is_none()
        || !plain_compiler_configuration(root)
    {
        return verified;
    }
    for path in [PathBuf::from("src/lib.rs"), PathBuf::from("src/main.rs")] {
        let Some(facts) = index.files.get(&path) else {
            continue;
        };
        if facts.used_lexical_fallback
            || !facts
                .functions
                .iter()
                .any(|id| is_generator(&index.function_facts[*id]))
            || !index.member_crates.is_default_target_root(&path)
            || !index.drop_in_manifests.verified(&path, "quickcheck")
        {
            continue;
        }
        let Some(tree) = index
            .source_syntax
            .get(index, &path)
            .and_then(|snapshot| snapshot.tree())
        else {
            continue;
        };
        if namespace_is_unambiguous(&tree) {
            verified.insert(path);
        }
    }
    verified
}

fn namespace_is_unambiguous(tree: &ast::SourceFile) -> bool {
    // Refusal is deliberately file-wide. Macro expansions and imported
    // namespaces cannot be used as absence certificates for shadowing.
    !tree.syntax().descendants().any(|node| {
        if let Some(name) = ast::Name::cast(node.clone()) {
            return matches!(name.text(), "quickcheck" | "r#quickcheck");
        }
        if let Some(import) = ast::Use::cast(node.clone()) {
            // This slice admits only direct QuickCheck type imports. Other
            // imports could replace a derive or the provider namespace.
            let text = import
                .syntax()
                .text()
                .to_string()
                .split_whitespace()
                .collect::<String>();
            return !matches!(
                text.as_str(),
                "usequickcheck::Arbitrary;"
                    | "usequickcheck::Gen;"
                    | "usequickcheck::{Arbitrary,Gen};"
                    | "usequickcheck::{Gen,Arbitrary};"
            );
        }
        if let Some(attribute) = ast::Attr::cast(node.clone()) {
            let text = attribute
                .syntax()
                .text()
                .to_string()
                .split_whitespace()
                .collect::<String>();
            // These built-in derives cannot introduce a provider binding;
            // imported/redefined derive names are refused above/below.
            return !matches!(
                text.as_str(),
                "#[derive(Clone)]" | "#[derive(Clone,Debug)]" | "#[derive(Debug,Clone)]"
            );
        }
        ast::ExternCrate::can_cast(node.kind())
            || ast::MacroCall::can_cast(node.kind())
            || ast::MacroRules::can_cast(node.kind())
            || ast::MacroDef::can_cast(node.kind())
    })
}

fn plain_compiler_configuration(root: &std::path::Path) -> bool {
    if std::env::vars_os().any(|(key, value)| {
        let key = key.to_string_lossy().to_ascii_uppercase();
        !value.is_empty()
            && (matches!(
                key.as_str(),
                "RUSTFLAGS"
                    | "CARGO_ENCODED_RUSTFLAGS"
                    | "RUSTC"
                    | "RUSTC_WRAPPER"
                    | "RUSTC_WORKSPACE_WRAPPER"
            ) || key.starts_with("CARGO_")
                && (key.ends_with("RUSTFLAGS") || key.contains("RUSTC")))
    }) {
        return false;
    }
    let Ok(home) = home::env::cargo_home_with_cwd_env(&home::env::OS_ENV, root) else {
        return false;
    };
    let home = if home.is_absolute() {
        home
    } else {
        root.join(home)
    };
    let mut paths = vec![home.join("config"), home.join("config.toml")];
    for directory in root.ancestors() {
        paths.extend([
            directory.join(".cargo/config"),
            directory.join(".cargo/config.toml"),
        ]);
    }
    paths
        .iter()
        .all(|path| match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str::<toml::Value>(&text)
                .ok()
                .is_some_and(|config| {
                    let compiler_keys = [
                        "rustflags",
                        "rustc",
                        "rustc-wrapper",
                        "rustc-workspace-wrapper",
                    ];
                    let plain = |table: &toml::Value| {
                        compiler_keys.iter().all(|key| table.get(*key).is_none())
                    };
                    config.get("include").is_none()
                        && config.get("build").is_none_or(plain)
                        && config.get("target").is_none_or(|targets| {
                            targets
                                .as_table()
                                .is_some_and(|targets| targets.values().all(plain))
                        })
                        && config.get("env").is_none_or(|env| {
                            env.as_table().is_some_and(|env| {
                                env.keys()
                                    .all(|key| !key.to_ascii_uppercase().starts_with("RUST"))
                            })
                        })
                }),
            Err(error) => error.kind() == std::io::ErrorKind::NotFound,
        })
}
