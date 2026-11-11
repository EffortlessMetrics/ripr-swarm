//! Source resolution of library unit-test `covered_by` selectors (#13705).
//!
//! Listing a `cargo test -p <package> --lib <module path>` selector needs the
//! package's library test target. For `ripr` that one build holds about
//! 13.8 GB, which does not fit a 15 GB container. Local `precommit` therefore
//! resolves such a selector against source: the named module must be declared
//! along the path from the library root and must hold at least one `#[test]`
//! function. That is weaker than a listing (it does not compile anything), so
//! its report row says so, and hosted qualification still lists the selector.
//!
//! Every shape this module does not understand returns `None`, and the caller
//! falls back to building and listing. A `None` is never a failure.

use std::path::{Path, PathBuf};

use ra_ap_syntax::ast::{self, HasAttrs, HasModuleItem, HasName};
use ra_ap_syntax::{AstNode, Edition, SourceFile};

/// `cargo test -p <package> --lib <module path>`, with optional lock flags and
/// nothing else.
#[derive(Debug, PartialEq, Eq)]
struct LibrarySelector {
    package: String,
    module_path: Vec<String>,
}

fn parse_library_selector(command: &str) -> Option<LibrarySelector> {
    let mut words = command.split_whitespace();
    if words.next() != Some("cargo") || words.next() != Some("test") {
        return None;
    }
    let mut package = None;
    let mut filter = None;
    let mut lib = false;
    while let Some(word) = words.next() {
        match word {
            "-p" | "--package" if package.is_none() => {
                package = Some(words.next().filter(|value| !value.starts_with('-'))?);
            }
            "--lib" if !lib => lib = true,
            "--locked" | "--offline" => {}
            positional if !positional.starts_with('-') && filter.is_none() => {
                filter = Some(positional);
            }
            _ => return None,
        }
    }
    if !lib {
        return None;
    }
    let module_path = filter?.split("::").map(str::to_string).collect::<Vec<_>>();
    let is_identifier = |segment: &String| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    };
    if !module_path.iter().all(is_identifier) {
        return None;
    }
    Some(LibrarySelector {
        package: package?.to_string(),
        module_path,
    })
}

/// The library root source file of workspace member `package`, read from the
/// manifests. Globbed members, a missing member or an unreadable manifest
/// give `None`.
fn library_root(root: &Path, package: &str) -> Option<PathBuf> {
    let workspace: toml::Table = std::fs::read_to_string(root.join("Cargo.toml"))
        .ok()?
        .parse()
        .ok()?;
    let members = workspace.get("workspace")?.get("members")?.as_array()?;
    for member in members {
        let member = member.as_str()?;
        if member.contains(['*', '?', '[']) {
            return None;
        }
        let directory = root.join(member);
        let manifest: toml::Table = std::fs::read_to_string(directory.join("Cargo.toml"))
            .ok()?
            .parse()
            .ok()?;
        if manifest.get("package")?.get("name")?.as_str()? != package {
            continue;
        }
        let relative = match manifest.get("lib").and_then(|lib| lib.get("path")) {
            Some(path) => path.as_str()?,
            None => "src/lib.rs",
        };
        let path = directory.join(relative);
        return path.is_file().then_some(path);
    }
    None
}

fn attribute_text(attr: &ast::Attr) -> String {
    attr.syntax()
        .text()
        .to_string()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

/// Whether an item's attributes leave it present in an ordinary test build.
/// `#[cfg(test)]` does. Any other `cfg`, a `cfg_attr` or a `#[path]` is not
/// understood here.
fn present_in_test_build<T: HasAttrs>(item: &T) -> bool {
    item.attrs().map(|attr| attribute_text(&attr)).all(|text| {
        text == "#[cfg(test)]" || !(text.starts_with("#[cfg") || text.starts_with("#[path"))
    })
}

fn is_test_function(function: &ast::Fn) -> bool {
    function
        .attrs()
        .any(|attr| attribute_text(&attr) == "#[test]")
}

fn parse_items(path: &Path) -> Option<Vec<ast::Item>> {
    let text = std::fs::read_to_string(path).ok()?;
    let parse = SourceFile::parse(&text, Edition::Edition2024);
    if !parse.errors().is_empty() {
        return None;
    }
    Some(parse.tree().items().collect())
}

/// Test names under `items`, relative to `prefix`. Nested inline modules are
/// followed. A nested file module (`mod deeper;`) is not read here, so its
/// tests would be missing from the list: the whole selector is refused
/// (`None`) and goes to the build-and-list path instead.
fn collect_tests(items: &[ast::Item], prefix: &str, tests: &mut Vec<String>) -> Option<()> {
    for item in items {
        match item {
            ast::Item::Fn(function)
                if is_test_function(function) && present_in_test_build(function) =>
            {
                if let Some(name) = function.name() {
                    tests.push(format!("{prefix}::{}", name.text()));
                }
            }
            ast::Item::Module(module) if present_in_test_build(module) => {
                let name = module.name()?;
                let nested = module.item_list()?.items().collect::<Vec<_>>();
                collect_tests(&nested, &format!("{prefix}::{}", name.text()), tests)?;
            }
            _ => {}
        }
    }
    Some(())
}

/// The tests a library unit-test selector names, read from source under
/// `root`. `None` means "not resolved here": the caller builds and lists.
pub(crate) fn resolve_in_source(root: &Path, command: &str) -> Option<Vec<String>> {
    let selector = parse_library_selector(command)?;
    let library = library_root(root, &selector.package)?;
    // Children of `lib.rs` live beside it.
    let mut child_directory = library.parent()?.to_path_buf();
    let mut items = parse_items(&library)?;
    for segment in &selector.module_path {
        let mut declared = items.iter().filter_map(|item| match item {
            ast::Item::Module(module)
                if module
                    .name()
                    .is_some_and(|name| name.text() == segment.as_str()) =>
            {
                Some(module.clone())
            }
            _ => None,
        });
        let module = declared.next()?;
        // Two declarations of one name are cfg alternatives; not understood.
        if declared.next().is_some() || !present_in_test_build(&module) {
            return None;
        }
        child_directory.push(segment);
        items = match module.item_list() {
            Some(list) => list.items().collect(),
            None => {
                let file = child_directory.with_extension("rs");
                let directory_module = child_directory.join("mod.rs");
                match (file.is_file(), directory_module.is_file()) {
                    (true, false) => parse_items(&file)?,
                    (false, true) => parse_items(&directory_module)?,
                    _ => return None,
                }
            }
        };
    }
    let mut tests = Vec::new();
    collect_tests(&items, &selector.module_path.join("::"), &mut tests)?;
    (!tests.is_empty()).then_some(tests)
}

#[cfg(test)]
mod tests {
    use super::{LibrarySelector, parse_library_selector, resolve_in_source};
    use std::path::{Path, PathBuf};

    struct Workspace(PathBuf);

    impl Workspace {
        fn new(label: &str) -> Result<Self, String> {
            let path = std::env::temp_dir().join(format!(
                "ripr-library-selector-{label}-{}",
                std::process::id()
            ));
            if path.exists() {
                std::fs::remove_dir_all(&path).map_err(|error| error.to_string())?;
            }
            let workspace = Self(path);
            workspace.write(
                "Cargo.toml",
                "[workspace]\nmembers = [\"crates/a\"]\nresolver = \"2\"\n",
            )?;
            workspace.write("crates/a/Cargo.toml", "[package]\nname = \"a\"\n")?;
            Ok(workspace)
        }

        fn write(&self, relative: &str, contents: &str) -> Result<(), String> {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(path, contents).map_err(|error| error.to_string())
        }

        fn root(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Workspace {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.0) {
                eprintln!("temporary workspace not removed: {error}");
            }
        }
    }

    const SELECTOR: &str = "cargo test -p a --lib outer::inner::tests";
    const INNER: &str = "pub fn f() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn first() {}\n    fn helper() {}\n    mod nested {\n        #[test]\n        fn second() {}\n    }\n}\n";

    fn layout(label: &str, inner: &str) -> Result<Workspace, String> {
        let workspace = Workspace::new(label)?;
        workspace.write("crates/a/src/lib.rs", "pub mod outer;\n")?;
        workspace.write("crates/a/src/outer/mod.rs", "pub(crate) mod inner;\n")?;
        workspace.write("crates/a/src/outer/inner.rs", inner)?;
        Ok(workspace)
    }

    fn require(condition: bool, message: impl Into<String>) -> Result<(), String> {
        if condition {
            Ok(())
        } else {
            Err(message.into())
        }
    }

    #[test]
    fn selector_grammar_accepts_only_one_package_lib_and_module_path() -> Result<(), String> {
        let expected = Some(LibrarySelector {
            package: "a".to_string(),
            module_path: vec!["outer".to_string(), "tests".to_string()],
        });
        for accepted in [
            "cargo test -p a --lib outer::tests",
            "cargo test --package a --locked --offline --lib outer::tests",
        ] {
            require(
                parse_library_selector(accepted) == expected,
                format!("not accepted: {accepted}"),
            )?;
        }
        for refused in [
            "cargo test -p a outer::tests",
            "cargo test -p a --lib",
            "cargo test --lib outer::tests",
            "cargo test -p a --lib outer::tests other",
            "cargo test -p a -p b --lib outer::tests",
            "cargo test -p a --lib --test t outer::tests",
            "cargo test -p a --lib --features x outer::tests",
            "cargo test -p a --lib outer::tests -- --ignored",
            "cargo test -p a --lib outer::",
            "cargo test -p a --lib outer::te-sts",
            "cargo nextest run -p a --lib outer::tests",
        ] {
            require(
                parse_library_selector(refused).is_none(),
                format!("accepted: {refused}"),
            )?;
        }
        Ok(())
    }

    #[test]
    fn a_declared_module_with_tests_resolves_to_their_names() -> Result<(), String> {
        let workspace = layout("resolves", INNER)?;
        let tests = resolve_in_source(workspace.root(), SELECTOR);
        require(
            tests
                == Some(vec![
                    "outer::inner::tests::first".to_string(),
                    "outer::inner::tests::nested::second".to_string(),
                ]),
            format!("wrong resolution: {tests:?}"),
        )
    }

    /// Each of these is a stale or not-understood pointer. None may resolve:
    /// the caller then builds and lists, which reports the real state.
    #[test]
    fn stale_or_unsupported_shapes_do_not_resolve() -> Result<(), String> {
        let no_tests = "#[cfg(test)]\nmod tests {\n    fn helper() {}\n}\n";
        let renamed = "#[cfg(test)]\nmod checks {\n    #[test]\n    fn first() {}\n}\n";
        let feature = "#[cfg(feature = \"x\")]\nmod tests {\n    #[test]\n    fn first() {}\n}\n";
        let pathed = "#[path = \"elsewhere.rs\"]\nmod tests;\n";
        let gated_test =
            "#[cfg(test)]\nmod tests {\n    #[cfg(windows)]\n    #[test]\n    fn first() {}\n}\n";
        let broken = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn first( {}\n";
        // Inline tests plus a nested file module: its tests would be left out
        // of the list, so the selector is not resolved at all.
        let nested_file =
            "#[cfg(test)]\nmod tests {\n    #[test]\n    fn first() {}\n    mod deeper;\n}\n";
        for (label, inner) in [
            ("no-tests", no_tests),
            ("renamed", renamed),
            ("feature", feature),
            ("pathed", pathed),
            ("gated-test", gated_test),
            ("broken", broken),
            ("nested-file", nested_file),
        ] {
            let workspace = layout(label, inner)?;
            let tests = resolve_in_source(workspace.root(), SELECTOR);
            require(tests.is_none(), format!("{label} resolved: {tests:?}"))?;
        }

        // The module file exists but the parent no longer declares it.
        let undeclared = layout("undeclared", INNER)?;
        undeclared.write("crates/a/src/outer/mod.rs", "// inner was removed\n")?;
        require(
            resolve_in_source(undeclared.root(), SELECTOR).is_none(),
            "an undeclared module resolved",
        )?;

        // Both `inner.rs` and `inner/mod.rs`: rustc refuses this, so do we.
        let ambiguous = layout("ambiguous", INNER)?;
        ambiguous.write("crates/a/src/outer/inner/mod.rs", INNER)?;
        require(
            resolve_in_source(ambiguous.root(), SELECTOR).is_none(),
            "an ambiguous module file resolved",
        )?;

        let other_package = layout("other-package", INNER)?;
        require(
            resolve_in_source(
                other_package.root(),
                "cargo test -p b --lib outer::inner::tests",
            )
            .is_none(),
            "an unknown package resolved",
        )
    }

    /// The shipped selector this exists for resolves in the real tree, so the
    /// local run does not build the `ripr` library test target for it.
    #[test]
    fn the_shipped_ripr_library_selector_resolves_in_this_repository() -> Result<(), String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("xtask has no parent directory")?;
        let policy = std::fs::read_to_string(root.join("policy/non-rust-allowlist.toml"))
            .map_err(|error| error.to_string())?;
        let selectors = policy
            .split('"')
            .filter(|text| text.starts_with("cargo test -p ripr ") && text.contains(" --lib "))
            .collect::<std::collections::BTreeSet<_>>();
        require(
            !selectors.is_empty(),
            "no ripr library selector is declared",
        )?;
        for selector in selectors {
            let tests = resolve_in_source(root, selector);
            require(
                tests.as_ref().is_some_and(|tests| !tests.is_empty()),
                format!("`{selector}` does not resolve in source: {tests:?}"),
            )?;
        }
        Ok(())
    }
}
