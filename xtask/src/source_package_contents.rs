//! Development-test exclusion contract for the crates.io source packages
//! (#15216).
//!
//! Cargo selects package members by file, not by module. A test module
//! declared out of line (`#[cfg(test)] mod helpers;`) is excluded from the
//! production build, but its file is still packaged unless `include` leaves
//! it out. File-name globs (`tests.rs`, `*_tests.rs`) miss test modules with
//! other names (`src/testing/**`, `method_hop_rows.rs`) and every module of
//! `ripr-contract`, which had no exclusions at all.
//!
//! This check reads the actual `.crate` members and the module declarations
//! in its packaged sources. A member fails when it is:
//!
//! - an integration test or bench target (`tests/**`, `benches/**`); or
//! - the file of a module that no production build compiles: declared under
//!   a `cfg` that is false whenever `test` and `kani` are off (`test`,
//!   `all(test, not(windows))`, `kani`), or nested in such a module, or
//!   declared by no packaged module at all.
//!
//! Inline test modules inside production files are a separate dimension:
//! the publish stage (#14198) strips them and #15193 owns moving them out.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::Path;

use ra_ap_syntax::ast::{self, HasAttrs, HasModuleItem, HasName};
use ra_ap_syntax::{AstNode, Edition, SourceFile};

/// The `ripr` package still carries the portable help harness compiled from
/// the extracted package (`xtask/tests/common/ripr_help.rs`). #15216 item 5
/// moves those assertions onto the executable installed from the qualified
/// archive and removes these members; until then they are the one named
/// exception.
const RIPR_PACKAGED_HELP_HARNESS: [&str; 3] = [
    "tests/cli_help_hierarchy.rs",
    "tests/common/help_contract.rs",
    "tests/common/help_workspace.rs",
];

/// Reads every regular member of a `.crate` (gzip tar) below `top_dir/`,
/// keyed by package-relative path.
pub(crate) fn read_crate_members(
    krate: &Path,
    top_dir: &str,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let file =
        std::fs::File::open(krate).map_err(|e| format!("cannot open {}: {e}", krate.display()))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let prefix = format!("{top_dir}/");
    let mut members = BTreeMap::new();
    for entry in archive
        .entries()
        .map_err(|e| format!("cannot read {}: {e}", krate.display()))?
    {
        let mut entry = entry.map_err(|e| format!("bad entry in {}: {e}", krate.display()))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .map_err(|e| format!("bad entry path in {}: {e}", krate.display()))?
            .to_string_lossy()
            .into_owned();
        let Some(relative) = path.strip_prefix(&prefix) else {
            return Err(format!(
                "{} member {path} is outside {prefix}",
                krate.display()
            ));
        };
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| format!("cannot read {path} in {}: {e}", krate.display()))?;
        members.insert(relative.to_string(), bytes);
    }
    if members.is_empty() {
        return Err(format!("{} has no members", krate.display()));
    }
    Ok(members)
}

/// Checks a `.crate` file against the development-test exclusion contract.
pub(crate) fn check_crate_file(package: &str, krate: &Path, version: &str) -> Result<(), String> {
    let members = read_crate_members(krate, &format!("{package}-{version}"))?;
    check_source_package(package, &members)
        .map_err(|err| format!("packaged {} {err}", krate.display()))
}

/// Applies the contract to package-relative members.
pub(crate) fn check_source_package(
    package: &str,
    members: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let mut refused: BTreeMap<String, &'static str> = BTreeMap::new();
    for path in members.keys() {
        let exempt = package == "ripr" && RIPR_PACKAGED_HELP_HARNESS.contains(&path.as_str());
        if (path.starts_with("tests/") || path.starts_with("benches/")) && !exempt {
            refused.insert(path.clone(), "integration test or bench target");
        }
    }
    let graph = module_graph(members)?;
    for path in members
        .keys()
        .filter(|path| path.starts_with("src/") && !path.ends_with(".rs"))
    {
        if !graph.compiled.contains(path) {
            refused.insert(path.clone(), "data read by no production include");
        }
    }
    for path in members.keys().filter(|path| is_library_source(path)) {
        if graph.compiled.contains(path) {
            continue;
        }
        if graph.is_test_only(path) {
            refused.insert(path.clone(), "module compiled only in test or kani builds");
        } else {
            refused.insert(path.clone(), "declared by no packaged production module");
        }
    }
    if refused.is_empty() {
        return Ok(());
    }
    let shown: Vec<String> = refused
        .iter()
        .take(12)
        .map(|(path, why)| format!("{path} ({why})"))
        .collect();
    Err(format!(
        "carries {} development-test member(s); exclude them from the package (#15216): {}{}",
        refused.len(),
        shown.join(", "),
        if refused.len() > shown.len() {
            ", ..."
        } else {
            ""
        }
    ))
}

fn is_library_source(path: &str) -> bool {
    path.starts_with("src/") && path.ends_with(".rs")
}

#[derive(Default)]
struct ModuleGraph {
    /// Files a production build compiles, from the crate roots.
    compiled: BTreeSet<String>,
    /// Files and directory prefixes (ending in `/`) of test-only modules.
    test_only: BTreeSet<String>,
}

impl ModuleGraph {
    fn is_test_only(&self, path: &str) -> bool {
        self.test_only.iter().any(|gated| {
            if gated.ends_with('/') {
                path.starts_with(gated.as_str())
            } else {
                path == gated
            }
        })
    }
}

/// Walks module declarations from `src/lib.rs` and `src/main.rs` through the
/// packaged sources.
fn module_graph(members: &BTreeMap<String, Vec<u8>>) -> Result<ModuleGraph, String> {
    let mut graph = ModuleGraph::default();
    let mut pending: Vec<String> = ["src/lib.rs", "src/main.rs", "build.rs"]
        .into_iter()
        .filter(|root| members.contains_key(*root))
        .map(str::to_string)
        .collect();
    if !pending.iter().any(|root| root.starts_with("src/")) {
        return Err("has neither src/lib.rs nor src/main.rs".to_string());
    }
    let roots: BTreeSet<String> = pending.iter().cloned().collect();
    while let Some(file) = pending.pop() {
        if !graph.compiled.insert(file.clone()) {
            continue;
        }
        let Some(bytes) = members.get(&file) else {
            continue;
        };
        let text = std::str::from_utf8(bytes)
            .map_err(|err| format!("member {file} is not UTF-8: {err}"))?;
        let parse = SourceFile::parse(text, Edition::Edition2024);
        if let Some(error) = parse.errors().first() {
            return Err(format!("member {file} does not parse: {error}"));
        }
        let owns_directory = roots.contains(&file) || file.ends_with("/mod.rs");
        let file_dir = parent_dir(&file);
        // `include!`, `include_str!` and `include_bytes!` outside test-only
        // items read a file into the production build. An `include!`d
        // file's own `mod` declarations are not followed, so a module it
        // declares reads as undeclared (refused).
        for included in production_includes(parse.tree().syntax())? {
            graph
                .compiled
                .insert(normalize(&format!("{file_dir}{included}")));
        }
        let child_dir = if owns_directory {
            file_dir.clone()
        } else {
            format!(
                "{file_dir}{}/",
                file.trim_end_matches(".rs")
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
            )
        };
        let scope = Scope {
            file_dir: &file_dir,
            module_dir: child_dir,
            inline: false,
            test_only: false,
        };
        walk_items(parse.tree().items(), &scope, &mut graph, &mut pending)?;
    }
    Ok(graph)
}

struct Scope<'a> {
    file_dir: &'a str,
    module_dir: String,
    inline: bool,
    test_only: bool,
}

fn walk_items(
    items: impl Iterator<Item = ast::Item>,
    scope: &Scope<'_>,
    graph: &mut ModuleGraph,
    pending: &mut Vec<String>,
) -> Result<(), String> {
    for item in items {
        let ast::Item::Module(module) = item else {
            continue;
        };
        let Some(name) = module
            .name()
            .map(|name| name.text().trim_start_matches("r#").to_string())
        else {
            continue;
        };
        let test_only = scope.test_only || absent_from_production(&module)?;
        let path_attr = path_attribute(&module);
        if let Some(list) = module.item_list() {
            let module_dir = match &path_attr {
                Some(path) => format!("{}{}/", scope.module_dir, path.trim_end_matches('/')),
                None => format!("{}{name}/", scope.module_dir),
            };
            let inner = Scope {
                file_dir: scope.file_dir,
                module_dir,
                inline: true,
                test_only,
            };
            walk_items(list.items(), &inner, graph, pending)?;
            continue;
        }
        let (files, subtree) = match &path_attr {
            Some(path) => {
                let base = if scope.inline {
                    scope.module_dir.as_str()
                } else {
                    scope.file_dir
                };
                let file = normalize(&format!("{base}{path}"));
                let subtree = if file.ends_with("/mod.rs") {
                    parent_dir(&file)
                } else {
                    format!("{}/", file.trim_end_matches(".rs"))
                };
                (vec![file], subtree)
            }
            None => (
                vec![
                    format!("{}{name}.rs", scope.module_dir),
                    format!("{}{name}/mod.rs", scope.module_dir),
                ],
                format!("{}{name}/", scope.module_dir),
            ),
        };
        if test_only {
            graph.test_only.extend(files);
            graph.test_only.insert(subtree);
        } else {
            pending.extend(files);
        }
    }
    Ok(())
}

/// String-literal arguments of `include!`, `include_str!` and
/// `include_bytes!` calls under `node`, skipping items that no production
/// build compiles.
fn production_includes(node: &ra_ap_syntax::SyntaxNode) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    for child in node.children() {
        if let Some(item) = ast::Item::cast(child.clone())
            && item_absent_from_production(&item)?
        {
            continue;
        }
        if let Some(call) = ast::MacroCall::cast(child.clone()) {
            let name = call
                .path()
                .and_then(|path| path.segment())
                .and_then(|segment| segment.name_ref())
                .map(|name| name.text().to_string());
            if matches!(
                name.as_deref(),
                Some("include" | "include_str" | "include_bytes")
            ) && let Some(tree) = call.token_tree()
            {
                let text = tree.syntax().text().to_string();
                if let Some(literal) = text.trim_start_matches('(').trim_start().strip_prefix('"')
                    && let Some(end) = literal.find('"')
                {
                    files.push(literal[..end].to_string());
                }
            }
        }
        files.extend(production_includes(&child)?);
    }
    Ok(files)
}

fn parent_dir(path: &str) -> String {
    match path.rfind('/') {
        Some(at) => path[..=at].to_string(),
        None => String::new(),
    }
}

/// Resolves `.` and `..` segments in a package-relative path.
fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out.join("/")
}

fn attribute_text(attr: &ast::Attr) -> String {
    attr.syntax()
        .text()
        .to_string()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

fn item_absent_from_production(item: &ast::Item) -> Result<bool, String> {
    match item {
        ast::Item::Const(it) => absent_from_production(it),
        ast::Item::Static(it) => absent_from_production(it),
        ast::Item::Fn(it) => absent_from_production(it),
        ast::Item::Module(it) => absent_from_production(it),
        ast::Item::Impl(it) => absent_from_production(it),
        ast::Item::MacroCall(it) => absent_from_production(it),
        ast::Item::Struct(it) => absent_from_production(it),
        ast::Item::Enum(it) => absent_from_production(it),
        ast::Item::Trait(it) => absent_from_production(it),
        ast::Item::Use(it) => absent_from_production(it),
        _ => Ok(false),
    }
}

fn path_attribute(module: &ast::Module) -> Option<String> {
    module.attrs().find_map(|attr| {
        let text = attribute_text(&attr);
        text.strip_prefix("#[path=\"")
            .and_then(|rest| rest.strip_suffix("\"]"))
            .map(str::to_string)
    })
}

/// Whether some `cfg` on the module is false in every build with `test` and
/// `kani` off. A `cfg_attr` that sets `path` is refused rather than guessed;
/// other `cfg_attr`s (lint levels) do not decide presence.
fn absent_from_production(module: &impl HasAttrs) -> Result<bool, String> {
    let mut absent = false;
    for text in module.attrs().map(|attr| attribute_text(&attr)) {
        if text.starts_with("#[cfg_attr") && text.contains("path=") {
            return Err(format!(
                "module attribute {text} is not understood by the source-package check"
            ));
        }
        if let Some(predicate) = text
            .strip_prefix("#[cfg(")
            .and_then(|rest| rest.strip_suffix(")]"))
        {
            absent |= production_value(predicate)? == Some(false);
        }
    }
    Ok(absent)
}

/// The value of a whitespace-free `cfg` predicate when `test` and `kani` are
/// off: `Some(false)` when it is false whatever every other option is,
/// `Some(true)` when it is true whatever they are, `None` otherwise.
fn production_value(predicate: &str) -> Result<Option<bool>, String> {
    for (combinator, all) in [("all(", true), ("any(", false)] {
        if let Some(inner) = predicate
            .strip_prefix(combinator)
            .and_then(|rest| rest.strip_suffix(')'))
        {
            let values = split_top_level(inner)?
                .into_iter()
                .map(production_value)
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(if all {
                if values.contains(&Some(false)) {
                    Some(false)
                } else if values.iter().all(|value| *value == Some(true)) {
                    Some(true)
                } else {
                    None
                }
            } else if values.contains(&Some(true)) {
                Some(true)
            } else if values.iter().all(|value| *value == Some(false)) {
                Some(false)
            } else {
                None
            });
        }
    }
    if let Some(inner) = predicate
        .strip_prefix("not(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return Ok(production_value(inner)?.map(|value| !value));
    }
    Ok(match predicate {
        "test" | "kani" => Some(false),
        _ => None,
    })
}

fn split_top_level(list: &str) -> Result<Vec<&str>, String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut quoted = false;
    let mut start = 0;
    for (at, ch) in list.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| format!("unbalanced cfg predicate {list}"))?;
            }
            ',' if !quoted && depth == 0 => {
                parts.push(&list[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    let last = &list[start..];
    if !last.is_empty() {
        parts.push(last);
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn members(files: &[(&str, &str)]) -> BTreeMap<String, Vec<u8>> {
        files
            .iter()
            .map(|(path, text)| ((*path).to_string(), text.as_bytes().to_vec()))
            .collect()
    }

    fn refusal(package: &str, files: &[(&str, &str)]) -> Result<String, String> {
        match check_source_package(package, &members(files)) {
            Ok(()) => Err(format!("{files:?} must be refused")),
            Err(error) => Ok(error),
        }
    }

    #[test]
    fn production_modules_pass_including_test_named_ones() -> Result<(), String> {
        check_source_package(
            "ripr-contract",
            &members(&[
                ("Cargo.toml", ""),
                ("README.md", ""),
                (
                    "src/lib.rs",
                    "pub mod a;\nmod related_tests;\n#[cfg(not(test))]\nmod live;\n#[cfg(unix)]\nmod unix;\nmod inline { mod nested; }\n#[cfg(test)]\nmod tests;\n",
                ),
                ("src/a.rs", "mod b;\n"),
                ("src/a/b.rs", ""),
                ("src/related_tests.rs", ""),
                ("src/live.rs", ""),
                ("src/unix.rs", ""),
                ("src/inline/nested.rs", ""),
            ]),
        )
    }

    #[test]
    fn test_only_modules_are_refused_whatever_their_name() -> Result<(), String> {
        let error = refusal(
            "ripr",
            &[
                (
                    "src/lib.rs",
                    "#[cfg(test)]\npub(crate) mod testing;\nmod a;\n#[cfg(kani)]\n#[path = \"proofs.rs\"]\nmod proofs;\n",
                ),
                ("src/testing/mod.rs", "mod fixture_git;\n"),
                ("src/testing/fixture_git.rs", ""),
                (
                    "src/a.rs",
                    "#[cfg(all(test, not(windows)))]\nmod hover_rows;\n#[cfg(test)]\nmod rows {\n    mod deep;\n}\n",
                ),
                ("src/a/hover_rows.rs", ""),
                ("src/a/hover_rows/inner.rs", ""),
                ("src/a/rows/deep.rs", ""),
                ("src/proofs.rs", ""),
            ],
        )?;
        for member in [
            "src/testing/mod.rs",
            "src/testing/fixture_git.rs",
            "src/a/hover_rows.rs",
            "src/a/hover_rows/inner.rs",
            "src/a/rows/deep.rs",
            "src/proofs.rs",
        ] {
            assert!(
                error.contains(&format!(
                    "{member} (module compiled only in test or kani builds)"
                )),
                "{member}: {error}"
            );
        }
        assert!(
            error.contains("carries 6 development-test member(s)"),
            "{error}"
        );
        Ok(())
    }

    #[test]
    fn build_script_modules_and_included_files_are_compiled() -> Result<(), String> {
        check_source_package(
            "ripr",
            &members(&[
                ("build.rs", "#[path = \"src/record.rs\"]\nmod record;\n"),
                (
                    "src/lib.rs",
                    "// Shared with build.rs; the crate only unit-tests it.\n#[cfg(test)]\nmod record;\nmod swarm;\n",
                ),
                ("src/record.rs", ""),
                (
                    "src/swarm/mod.rs",
                    "#[path = \"queue_live.rs\"]\nmod queue;\n",
                ),
                ("src/swarm/queue_live.rs", "include!(\"queue.rs\");\n"),
                ("src/swarm/queue.rs", "pub fn run() {}\n"),
            ]),
        )?;
        // Without the build script, the shared module is test-only.
        let error = refusal(
            "ripr",
            &[
                ("src/lib.rs", "#[cfg(test)]\nmod record;\n"),
                ("src/record.rs", ""),
            ],
        )?;
        assert!(
            error.contains("src/record.rs (module compiled only"),
            "{error}"
        );
        Ok(())
    }

    #[test]
    fn source_data_must_be_read_by_production_code() -> Result<(), String> {
        let lib = "const SCHEMA: &str = include_str!(\"schema.json\");\n#[cfg(test)]\nconst FIXTURE: &str = include_str!(\"fixtures/a.json\");\n#[cfg(test)]\nmod tests {\n    const B: &[u8] = include_bytes!(\"fixtures/b.bin\");\n}\n";
        let error = refusal(
            "ripr",
            &[
                ("src/lib.rs", lib),
                ("src/schema.json", "{}"),
                ("src/fixtures/a.json", "{}"),
                ("src/fixtures/b.bin", ""),
            ],
        )?;
        assert!(
            error.contains("carries 2 development-test member(s)"),
            "{error}"
        );
        assert!(
            error.contains("src/fixtures/a.json (data read by no production include)"),
            "{error}"
        );
        assert!(
            error.contains("src/fixtures/b.bin (data read by no production include)"),
            "{error}"
        );
        assert!(!error.contains("schema.json"), "{error}");
        Ok(())
    }

    #[test]
    fn orphan_sources_and_integration_tests_are_refused() -> Result<(), String> {
        let error = refusal(
            "ripr-contract",
            &[
                ("src/lib.rs", ""),
                ("src/orphan.rs", ""),
                ("tests/cli_help_hierarchy.rs", ""),
                ("benches/speed.rs", ""),
            ],
        )?;
        assert!(
            error.contains("src/orphan.rs (declared by no packaged production module)"),
            "{error}"
        );
        assert!(
            error.contains("tests/cli_help_hierarchy.rs (integration test"),
            "{error}"
        );
        assert!(
            error.contains("benches/speed.rs (integration test"),
            "{error}"
        );
        // The named help-harness exception is `ripr`'s only, and covers
        // exactly its three files.
        check_source_package(
            "ripr",
            &members(&[
                ("src/lib.rs", ""),
                ("tests/cli_help_hierarchy.rs", ""),
                ("tests/common/help_contract.rs", ""),
                ("tests/common/help_workspace.rs", ""),
            ]),
        )?;
        let error = refusal("ripr", &[("src/lib.rs", ""), ("tests/other.rs", "")])?;
        assert!(error.contains("tests/other.rs"), "{error}");
        Ok(())
    }

    #[test]
    fn cfg_predicates_read_production_absence_conservatively() -> Result<(), String> {
        for (predicate, expected) in [
            ("test", Some(false)),
            ("kani", Some(false)),
            ("not(test)", Some(true)),
            ("all(test,not(windows))", Some(false)),
            ("any(test,kani)", Some(false)),
            ("any(test,unix)", None),
            ("all(unix,feature=\"a,b\")", None),
            ("feature=\"test\"", None),
        ] {
            assert_eq!(production_value(predicate)?, expected, "{predicate}");
        }
        Ok(())
    }

    #[test]
    fn unparseable_sources_and_cfg_attr_fail_closed() -> Result<(), String> {
        let error = refusal("ripr", &[("src/lib.rs", "mod a")])?;
        assert!(error.contains("does not parse"), "{error}");
        let error = refusal(
            "ripr",
            &[
                ("src/lib.rs", "#[cfg_attr(test, path = \"t.rs\")]\nmod a;\n"),
                ("src/a.rs", ""),
            ],
        )?;
        assert!(error.contains("not understood"), "{error}");
        Ok(())
    }
}
