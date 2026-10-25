//! Rust files that no Cargo target can compile (#8999).
//!
//! A corpus or benchmark haystack such as `benchmarks/haystacks/code/rust-library.rs`
//! is ordinary Rust text that lives outside `src/`, `tests/`, `benches/` and
//! `examples/`. Cargo only compiles a file that is a target root, a `mod`
//! child of one, or `include!`d into one. A `#[test]` in such a file never
//! runs, so crediting it as a related test is a wrong promotion, and a
//! parser refusal of the file is not a gap in any test evidence.
//!
//! This module only decides which indexed files are *positively* never
//! compiled. Every doubt leaves the file alone: a literal mention of its file
//! name anywhere in a Rust source or Cargo manifest (a `#[path]`,
//! `include!`, `[[bench]] path`, `build =`), a `mod <stem>` declaration
//! anywhere, a macro that builds `mod` or `include!` operands, an unreadable
//! manifest, or a package whose crate root is not indexed.

use super::model::RustIndex;
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

/// Directories Cargo autodiscovers or the module tree owns. A file below one
/// of them is never a stray: it is a target, a module child, or a helper whose
/// parent the module pass already resolves.
const TARGET_DIRS: &[&str] = &["src", "tests", "benches", "examples"];

/// True when `relative` (a path below its package directory) lies in a
/// directory Cargo never autodiscovers a target from. Root-level files stay
/// out: `build.rs` is the build-script root.
pub(super) fn is_stray_shape(relative: &Path) -> bool {
    let mut components = relative.components();
    let first = match components.next() {
        Some(Component::Normal(name)) => name.to_str(),
        _ => return false,
    };
    let Some(first) = first else {
        return false;
    };
    let rest = components.as_path();
    !TARGET_DIRS.contains(&first)
        && rest.components().count() >= 1
        && relative
            .extension()
            .is_some_and(|extension| extension == "rs")
}

/// The subset of `candidates` (each an unlinked, non-root file of the package
/// whose directory it is paired with) that no Cargo target can compile.
///
/// Returns nothing when any manifest cannot be read.
pub(super) fn never_compiled(
    index: &RustIndex,
    workspace_root: &Path,
    manifest_dirs: &BTreeSet<PathBuf>,
    candidates: &[PathBuf],
) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    if candidates.is_empty() {
        return out;
    }
    // Names compare ASCII-case-insensitively: a case-insensitive filesystem
    // resolves `#[path]` operands that differ from the file only in case.
    let candidate_names: BTreeSet<String> = candidates
        .iter()
        .filter_map(|file| file.file_name().and_then(|name| name.to_str()))
        .map(str::to_ascii_lowercase)
        .collect();
    let mut referenced_names: BTreeSet<String> = BTreeSet::new();
    let mut declared_modules: BTreeSet<String> = BTreeSet::new();
    for facts in index.files().values() {
        let source: &str = &facts.source;
        if builds_module_operands(source) {
            return out;
        }
        if spells_a_path_with_escapes(source) {
            return out;
        }
        collect_referenced_file_names(
            &source.to_ascii_lowercase(),
            &candidate_names,
            &mut referenced_names,
        );
        collect_declared_modules(source, &mut declared_modules);
    }
    let mut manifests: BTreeSet<PathBuf> = manifest_dirs
        .iter()
        .map(|dir| dir.join("Cargo.toml"))
        .collect();
    manifests.insert(workspace_root.join("Cargo.toml"));
    for manifest in &manifests {
        let path = if manifest.is_absolute() {
            manifest.clone()
        } else {
            workspace_root.join(manifest)
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                if spells_a_path_with_escapes(&text) {
                    return BTreeSet::new();
                }
                collect_referenced_file_names(
                    &text.to_ascii_lowercase(),
                    &candidate_names,
                    &mut referenced_names,
                );
            }
            // A workspace without a root manifest is fine; any other
            // unreadable manifest is doubt.
            Err(_) if path == workspace_root.join("Cargo.toml") => {}
            Err(_) => return BTreeSet::new(),
        }
    }
    // A package's build script is a crate root whose name the manifest may
    // not repeat; its text is already among the indexed sources.
    for file in candidates {
        let Some(name) = file.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        // A `mod.rs` is named by its directory's declaration, which a text scan
        // cannot tie to the file: keep it.
        if name == "mod.rs" {
            continue;
        }
        let stem = file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        if !referenced_names.contains(&name.to_ascii_lowercase())
            && !declared_modules.contains(&stem.to_ascii_lowercase())
        {
            out.insert(file.clone());
        }
    }
    out
}

/// True when a source may assemble a `mod` or `include!` operand the text
/// cannot name: any `$` beside a `mod`/`include` word (macro metavariables,
/// whatever comments, strings or whitespace sit between them), or an
/// `include!` whose operand is not a plain string literal.
fn builds_module_operands(source: &str) -> bool {
    if source.contains('$') && (source.contains("mod") || source.contains("include")) {
        return true;
    }
    let compact: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();
    compact
        .match_indices("include!(")
        .any(|(at, matched)| !compact[at + matched.len()..].starts_with('"'))
}

/// `source` with every block and line comment replaced by one space; block
/// comments nest as in Rust. Strings are not tracked, so callers also scan the
/// raw text and a stripped scan only ever adds names.
fn without_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(at) = rest.find('/') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        if tail.starts_with("/*") {
            out.push(' ');
            let mut depth = 0usize;
            let mut end = tail.len();
            let bytes = tail.as_bytes();
            let mut i = 0;
            while i + 1 < bytes.len() {
                if &bytes[i..i + 2] == b"/*" {
                    depth += 1;
                    i += 2;
                } else if &bytes[i..i + 2] == b"*/" {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        end = i;
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            rest = &tail[end..];
        } else if tail.starts_with("//") {
            out.push(' ');
            rest = tail.find('\n').map_or("", |end| &tail[end..]);
        } else {
            out.push('/');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

/// Records each candidate file name that `text` mentions anywhere, including
/// inside a longer path, a string with spaces or punctuation, or a comment:
/// a bare substring match only ever adds doubt.
fn collect_referenced_file_names(
    text: &str,
    candidate_names: &BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    for name in candidate_names {
        if text.contains(name.as_str()) {
            out.insert(name.clone());
        }
    }
}

/// True when a `path` attribute or key, or an `include!`, is followed by a
/// backslash before its operand closes (an escape such as `\x2e`, `\u{72}`,
/// a line continuation or a Windows separator), so the operand could spell a
/// name the substring scan cannot match.
fn spells_a_path_with_escapes(text: &str) -> bool {
    ["path", "include!"].iter().any(|key| {
        text.match_indices(key).any(|(at, _)| {
            let window = &text[at..];
            let end = window
                .char_indices()
                .find(|&(index, ch)| index > 300 || ch == ']' || ch == ')')
                .map_or(window.len(), |(index, _)| index);
            window[..end].contains('\\')
        })
    })
}

/// Every identifier following a `mod` keyword, in any position. Comments and
/// strings count too: this only ever adds doubt.
fn collect_declared_modules(source: &str, out: &mut BTreeSet<String>) {
    collect_modules_in(source, out);
    collect_modules_in(&without_comments(source), out);
}

fn collect_modules_in(source: &str, out: &mut BTreeSet<String>) {
    for (at, _) in source.match_indices("mod") {
        let before = source[..at].chars().next_back();
        if before.is_some_and(|ch| ch.is_alphanumeric() || ch == '_') {
            continue;
        }
        if !source[at + 3..]
            .chars()
            .next()
            .is_some_and(|ch| ch.is_whitespace() || ch == '/')
        {
            continue;
        }
        let after = source[at + 3..].trim_start();
        let after = after.strip_prefix("r#").unwrap_or(after);
        let name: String = after
            .chars()
            .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
            .collect();
        if !name.is_empty() {
            out.insert(name.to_ascii_lowercase());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stray_shape_excludes_target_directories_and_root_files() {
        assert!(is_stray_shape(Path::new(
            "benchmarks/haystacks/code/rust-library.rs"
        )));
        assert!(is_stray_shape(Path::new("scripts/tool.rs")));
        for kept in [
            "src/lib.rs",
            "src/deep/mod.rs",
            "tests/it.rs",
            "benches/b.rs",
            "examples/e/main.rs",
            "build.rs",
            "benchmarks/readme.md",
        ] {
            assert!(!is_stray_shape(Path::new(kept)), "{kept}");
        }
    }

    #[test]
    fn referenced_names_match_inside_any_literal_or_path() {
        let candidates: BTreeSet<String> =
            ["x.rs", "my hay.rs", "a(1).rs", "it's.rs", "bench-main.rs"]
                .map(String::from)
                .into();
        let mut names = BTreeSet::new();
        collect_referenced_file_names(
            &"#[path = \"../benchmarks/x.rs\"] mod a; \
              #[path = \"../my hay.rs\"] mod b; #[path=\"a(1).rs\"] mod c; \
              #[path = \"it's.rs\"] mod d; [[bench]]\npath = \"z/Bench-Main.rs\""
                .to_ascii_lowercase(),
            &candidates,
            &mut names,
        );
        assert_eq!(names.len(), candidates.len(), "{names:?}");
    }

    #[test]
    fn escaped_path_operands_are_doubt() {
        for doubt in [
            "#[path = \"\\u{72}ust.rs\"] mod m;",
            "path = \"benchmarks\\hay.rs\"",
            "#[path = \"../rust-\\\n  library.rs\"] mod m;",
            "#[path = \"../rust-library\\x2ers\"] mod m;",
            "#[path = \"../rust-library\\u{2e}rs\"] mod m;",
            "include!(\"gen\\x2ers\");",
        ] {
            assert!(spells_a_path_with_escapes(doubt), "{doubt}");
        }
        assert!(!spells_a_path_with_escapes(
            "#[path = \"../hay.rs\"] mod m; // \\n"
        ));
        assert!(!spells_a_path_with_escapes(
            "let s = \"a\\n\";\nmod m; // x.rs"
        ));
    }

    #[test]
    fn declared_modules_ignore_longer_identifiers() {
        let mut modules = BTreeSet::new();
        collect_declared_modules(
            "mod real;\nfn submod x() {}\npub mod other {}\n",
            &mut modules,
        );
        assert!(modules.contains("real"));
        assert!(modules.contains("other"));
        assert!(!modules.contains("x"));
    }

    #[test]
    fn module_tokens_survive_newlines_and_comments() {
        let mut modules = BTreeSet::new();
        collect_declared_modules("mod\nnl;\nmod/*c*/cm;\nmodule x;\n", &mut modules);
        assert!(modules.contains("nl"), "{modules:?}");
        assert!(modules.contains("cm"), "{modules:?}");
        assert!(!modules.contains("ule"));
        collect_declared_modules("mod /* /* */ */ nested;\nmod r#raw;\n", &mut modules);
        assert!(modules.contains("nested"), "{modules:?}");
        assert!(modules.contains("raw"), "{modules:?}");
    }

    #[test]
    fn metavariable_module_operands_survive_trivia() {
        for source in [
            "m!{ ($n:ident) => { mod $n; } }",
            "m!{ ($n:ident) => { mod\n$n; } }",
            "m!{ ($n:ident) => { mod/*c*/$n; } }",
            "m!{ ($n:ident) => { include!\n($n); } }",
        ] {
            assert!(builds_module_operands(source), "{source}");
        }
        for source in [
            "m!{ ($n:ident) => { mod /* /* */ */ $n; } }",
            "const A: &str = \"/*\";\nm!{ ($n:ident) => { mod $n; } }",
            "include!(concat!(\"hay\", \".rs\"));",
            "include!(env!(\"X\"));",
        ] {
            assert!(builds_module_operands(source), "{source}");
        }
        assert!(!builds_module_operands("mod plain;\n// mod\nfn f() {}"));
        assert!(!builds_module_operands("include!(\"gen.rs\");"));
    }
}

#[cfg(test)]
mod index_tests {
    use super::super::model::RustIndex;
    use std::fs;
    use std::path::{Path, PathBuf};

    const HAY: &str = "benchmarks/haystacks/code/rust-library.rs";
    const HAY_SOURCE: &str = "#[test]\nfn corpus_case() { assert_eq!(1 + 1, 2); }\n";
    const LIB: &str = "pub fn discount(q: u32) -> u32 { q }\n";
    const MANIFEST: &str = "[package]\nname='demo'\nversion='0.1.0'\nedition='2021'\n";

    fn fixture(name: &str, extra: &[(&str, &str)]) -> Result<(PathBuf, Vec<PathBuf>), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("ripr-stray-{name}-{stamp}"));
        let mut files = Vec::new();
        let mut manifest = MANIFEST.to_string();
        let mut sources: Vec<(&str, &str)> = vec![
            ("src/lib.rs", LIB),
            ("tests/it.rs", "#[test]\nfn it() { assert_eq!(1, 1); }\n"),
        ];
        for (path, text) in extra {
            if *path == "Cargo.toml" {
                manifest.push_str(text);
            } else {
                sources.push((path, text));
            }
        }
        // A later entry replaces the default of the same path.
        let sources: std::collections::BTreeMap<&str, &str> = sources.into_iter().collect();
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        fs::write(root.join("Cargo.toml"), manifest).map_err(|error| error.to_string())?;
        for (path, text) in sources {
            let full = root.join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::write(&full, text).map_err(|error| error.to_string())?;
            if path.ends_with(".rs") {
                files.push(PathBuf::from(path));
            }
        }
        Ok((root, files))
    }

    fn outside(index: &RustIndex, file: &str) -> bool {
        index
            .files()
            .get(Path::new(file))
            .is_some_and(|facts| facts.role_provenance.outside_module_tree)
    }

    fn test_files(index: &RustIndex) -> Vec<String> {
        index
            .tests()
            .iter()
            .map(|test| test.file.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    fn build(root: &Path, files: &[PathBuf]) -> Result<RustIndex, String> {
        crate::analysis::facts::build_index(root, files)
    }

    fn run(name: &str, extra: &[(&str, &str)]) -> Result<RustIndex, String> {
        let (root, files) = fixture(name, extra)?;
        let index = build(&root, &files);
        let _ = fs::remove_dir_all(&root);
        index
    }

    #[test]
    fn stray_haystack_tests_are_not_related_test_evidence() -> Result<(), String> {
        let index = run("positive", &[(HAY, HAY_SOURCE)])?;
        assert!(outside(&index, HAY), "haystack is outside every target");
        assert_eq!(test_files(&index), vec!["tests/it.rs".to_string()]);
        Ok(())
    }

    #[test]
    fn stray_file_the_parser_refuses_is_still_outside_every_target() -> Result<(), String> {
        let index = run(
            "fallback",
            &[(HAY, "#[test]\nfn corpus_case() {\nfn broken( {\n")],
        )?;
        let facts = index
            .files()
            .get(Path::new(HAY))
            .ok_or("haystack indexed")?;
        assert!(facts.used_lexical_fallback, "premise: parser refusal");
        assert!(outside(&index, HAY));
        assert_eq!(test_files(&index), vec!["tests/it.rs".to_string()]);
        Ok(())
    }

    #[test]
    fn inline_module_path_with_punctuation_or_escape_keeps_the_file() -> Result<(), String> {
        for (name, file, lib) in [
            (
                "space",
                "benchmarks/my hay.rs",
                "mod o { #[path = \"../../benchmarks/my hay.rs\"] mod h; }\n",
            ),
            (
                "paren",
                "benchmarks/a(1).rs",
                "mod o { #[path = \"../../benchmarks/a(1).rs\"] mod h; }\n",
            ),
            (
                "escape",
                "benchmarks/rust.rs",
                "mod o { #[path = \"../../benchmarks/\\u{72}ust.rs\"] mod h; }\n",
            ),
            (
                "hex",
                "benchmarks/rust.rs",
                "mod o { #[path = \"../../benchmarks/rust\\x2ers\"] mod h; }\n",
            ),
            (
                "continuation",
                "benchmarks/rust-library.rs",
                "mod o { #[path = \"../../benchmarks/rust-\\\n   library.rs\"] mod h; }\n",
            ),
            (
                "case",
                "benchmarks/Rust-Library.rs",
                "mod o { #[path = \"../../benchmarks/rust-library.rs\"] mod h; }\n",
            ),
        ] {
            let lib_source = format!("{LIB}{lib}");
            let index = run(name, &[("src/lib.rs", &lib_source), (file, HAY_SOURCE)])?;
            assert!(!outside(&index, file), "{name}");
        }
        Ok(())
    }

    #[test]
    fn manifest_bench_path_keeps_the_file_a_target() -> Result<(), String> {
        let manifest = format!("[[bench]]\nname='hay'\npath='{HAY}'\nharness=false\n");
        let index = run("manifest", &[("Cargo.toml", &manifest), (HAY, HAY_SOURCE)])?;
        assert!(!outside(&index, HAY));
        assert!(test_files(&index).contains(&HAY.to_string()));
        Ok(())
    }

    #[test]
    fn another_packages_manifest_naming_the_file_keeps_it_a_target() -> Result<(), String> {
        let manifest = format!(
            "[package]\nname='other'\nversion='0.1.0'\nedition='2021'\n\n[[bench]]\nname='hay'\npath='../{HAY}'\nharness=false\n"
        );
        let other = [
            ("other/Cargo.toml", manifest.as_str()),
            ("other/src/lib.rs", LIB),
        ];
        let named = run("other-manifest", &[(HAY, HAY_SOURCE), other[0], other[1]])?;
        assert!(!outside(&named, HAY), "the sibling manifest names the file");
        let unnamed = run(
            "other-control",
            &[(HAY, HAY_SOURCE), ("other/src/lib.rs", LIB)],
        )?;
        assert!(
            outside(&unnamed, HAY),
            "control: no manifest names the file"
        );
        Ok(())
    }

    #[test]
    fn path_attribute_into_the_file_keeps_it_compiled() -> Result<(), String> {
        let lib =
            "#[cfg(test)]\n#[path = \"../benchmarks/haystacks/code/rust-library.rs\"]\nmod hay;\n";
        let index = run("path-attr", &[("src/lib.rs", lib), (HAY, HAY_SOURCE)])?;
        assert!(!outside(&index, HAY));
        Ok(())
    }

    #[test]
    fn nested_module_or_include_mention_is_doubt() -> Result<(), String> {
        let nested =
            "mod outer { #[path = \"../benchmarks/haystacks/code/rust-library.rs\"] mod hay; }\n";
        let index = run("nested", &[("src/lib.rs", nested), (HAY, HAY_SOURCE)])?;
        assert!(!outside(&index, HAY), "inline-module #[path] is doubt");
        let include = "include!(\"../benchmarks/haystacks/code/rust-library.rs\");\n";
        let index = run("include", &[("src/lib.rs", include), (HAY, HAY_SOURCE)])?;
        assert!(!outside(&index, HAY), "include! is not an orphan");
        let stem = "mod shared { mod deep; }\n";
        let shared = "benchmarks/shared.rs";
        let index = run("stem", &[("src/lib.rs", stem), (shared, HAY_SOURCE)])?;
        assert!(
            !outside(&index, shared),
            "a same-stem mod anywhere is doubt"
        );
        let index = run("stem-control", &[("src/lib.rs", LIB), (shared, HAY_SOURCE)])?;
        assert!(outside(&index, shared), "control: no `mod shared` anywhere");
        Ok(())
    }

    #[test]
    fn macro_built_module_operands_are_doubt() -> Result<(), String> {
        let lib = "macro_rules! m { ($n:ident) => { mod $n; } }\n";
        let index = run("macro", &[("src/lib.rs", lib), (HAY, HAY_SOURCE)])?;
        assert!(!outside(&index, HAY));
        Ok(())
    }

    #[test]
    fn linked_unparsed_file_is_workspace_doubt_but_unlinked_one_is_not() -> Result<(), String> {
        // A lexical-fallback crate root emits no `mod` declarations, so it may
        // compile the haystack.
        let broken_root = "mod a; fn broken( {\n";
        let index = run(
            "broken-root",
            &[("src/lib.rs", broken_root), (HAY, HAY_SOURCE)],
        )?;
        assert!(!outside(&index, HAY));
        Ok(())
    }

    #[test]
    fn package_without_an_indexed_crate_root_gives_no_evidence() -> Result<(), String> {
        let (root, _) = fixture("no-root", &[(HAY, HAY_SOURCE)])?;
        let only = vec![PathBuf::from(HAY)];
        let index = build(&root, &only);
        let _ = fs::remove_dir_all(&root);
        assert!(!outside(&index?, HAY));
        Ok(())
    }

    #[test]
    fn leaf_addition_that_links_a_withheld_file_rebuilds_like_fresh() -> Result<(), String> {
        let manifest = "[[bench]]\nname='entry'\npath='benchmarks/entry.rs'\nharness=false\n";
        let (root, files) = fixture("reuse", &[("Cargo.toml", manifest), (HAY, HAY_SOURCE)])?;
        let load = |paths: &[PathBuf]| -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
            paths
                .iter()
                .map(|path| {
                    fs::read(root.join(path))
                        .map(|bytes| (path.clone(), bytes))
                        .map_err(|error| error.to_string())
                })
                .collect()
        };
        let core = load(&files)?;
        let cached =
            crate::analysis::facts::build_analysis_index_from_loaded_files(&root, &core, &[])?;
        assert!(
            outside(&cached.index, HAY),
            "premise: withheld before the addition"
        );
        let entry = "#[path = \"haystacks/code/rust-library.rs\"]\nmod hay;\nfn main() {}\n";
        fs::write(root.join("benchmarks/entry.rs"), entry).map_err(|error| error.to_string())?;
        let mut all = files.clone();
        all.push(PathBuf::from("benchmarks/entry.rs"));
        let target = load(&all)?;
        let reused = crate::analysis::facts::reuse_cached_index_with_loaded_files(
            cached,
            &root,
            &target,
            &[],
        )?;
        let fresh =
            crate::analysis::facts::build_analysis_index_from_loaded_files(&root, &target, &[])?;
        let _ = fs::remove_dir_all(&root);
        assert!(
            !outside(&fresh.index, HAY),
            "premise: the new entry links the file"
        );
        assert!(test_files(&fresh.index).contains(&HAY.to_string()));
        assert_eq!(test_files(&reused.index), test_files(&fresh.index));
        Ok(())
    }
}
