//! #11400: a changed `const` value is its readers' new return value. When
//! every same-file reader returns the constant on one line, `ripr check`
//! names that returned expression as the change and `ripr repair` scopes to
//! the same code, so a weakly checked reader gets the same repair start from
//! both commands. Anything less certain keeps the constant an unknown with no
//! start.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

const GREET_OLD: &str = "pub const GREETING: &str = \"hello\";\n\npub fn greet(name: &str) -> String {\n    format!(\"{GREETING}, {name}\")\n}\n";

const WEAK_TEST: &str = "use fx::greet;\n\n#[test]\nfn greet_mentions_name() {\n    assert!(greet(\"bob\").contains(\"bob\"));\n}\n";

struct Repo(PathBuf);

impl Repo {
    /// `old_src` + `test` on `main`, then `new_src` on `feature`.
    fn create(old_src: &str, new_src: &str, test: &str) -> Result<Self, String> {
        Self::create_with_files(old_src, new_src, test, &[])
    }

    /// `create`, with extra test-side `files` committed in the base.
    fn create_with_files(
        old_src: &str,
        new_src: &str,
        test: &str,
        files: &[(&str, &str)],
    ) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-const-reader-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
        let repo = Self(root);
        repo.write(".gitignore", "/target/\n")?;
        repo.write("Cargo.toml", CARGO_TOML)?;
        repo.write("src/lib.rs", old_src)?;
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
        repo.git(&["branch", "-M", "main"])?;
        repo.git(&["checkout", "-qb", "feature"])?;
        repo.write("src/lib.rs", new_src)?;
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

    /// The `ripr check` screen and bare `ripr repair` (started, text).
    fn answers(&self) -> Result<(String, bool, String), String> {
        let check = text(&self.ripr(&["check", "--base", "main"])?);
        let repair = self.ripr(&["repair", "--root", "."])?;
        Ok((check, repair.status.success(), text(&repair)))
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

/// The id after `ripr repair ` on the check screen, if check offers a start.
fn check_start_id(check: &str) -> Option<&str> {
    check.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix("ripr repair ")?;
        let id = rest.split_whitespace().next()?;
        (id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit())).then_some(id)
    })
}

fn greet_case(new_value: &str, test: &str) -> Result<Repo, String> {
    Repo::create(GREET_OLD, &GREET_OLD.replace("\"hello\"", new_value), test)
}

/// Neither command starts a repair, and check offers no start line.
fn assert_no_start(case: &str, repo: &Repo) -> Result<(String, String), String> {
    let (check, started, repair) = repo.answers()?;
    if started {
        return Err(format!("{case}: repair must not start:\n{repair}"));
    }
    if let Some(id) = check_start_id(&check) {
        return Err(format!(
            "{case}: check must not offer `ripr repair {id}`:\n{check}"
        ));
    }
    Ok((check, repair))
}

#[test]
fn weakly_checked_reader_of_a_changed_constant_starts_in_both_commands() -> Result<(), String> {
    // The audit `string-const` case: `greet` returns the constant in its
    // `format!`, and the only test checks `contains("bob")`.
    let repo = greet_case("\"hi\"", WEAK_TEST)?;
    let (check, started, repair) = repo.answers()?;
    let Some(id) = check_start_id(&check) else {
        return Err(format!("check must offer a repair start:\n{check}"));
    };
    if !check.contains("format!(\"{GREETING}, {name}\")") {
        return Err(format!("check must name greet's returned value:\n{check}"));
    }
    if !started || !repair.contains(&format!("seam `{id}`")) {
        return Err(format!(
            "bare repair must start on the id check prints ({id}):\n{repair}"
        ));
    }
    Ok(())
}

#[test]
fn exactly_checked_reader_is_already_checked_and_does_not_start() -> Result<(), String> {
    let repo = greet_case(
        "\"hi\"",
        "use fx::greet;\n\n#[test]\nfn greet_is_exact() {\n    assert_eq!(greet(\"bob\"), \"hi, bob\");\n}\n",
    )?;
    let (check, _) = assert_no_start("exact pin", &repo)?;
    if !check.contains("Static exposure: exposed") {
        return Err(format!("exact pin: check must read exposed:\n{check}"));
    }
    Ok(())
}

#[test]
fn reader_no_test_calls_does_not_start() -> Result<(), String> {
    let repo = Repo::create(
        &format!("{GREET_OLD}\npub fn other() -> u32 {{\n    1\n}}\n"),
        &format!("{GREET_OLD}\npub fn other() -> u32 {{\n    1\n}}\n")
            .replace("\"hello\"", "\"hi\""),
        "use fx::other;\n\n#[test]\nfn other_is_positive() {\n    assert!(other() > 0);\n}\n",
    )?;
    assert_no_start("no test calls greet", &repo).map(|_| ())
}

#[test]
fn constant_declared_twice_in_the_file_does_not_start() -> Result<(), String> {
    // An associated const of the same name cannot be told apart by spelling.
    let old = format!(
        "{GREET_OLD}\npub struct Other;\n\nimpl Other {{\n    pub const GREETING: &'static str = \"yo\";\n}}\n"
    );
    let repo = Repo::create(&old, &old.replacen("\"hello\"", "\"hi\"", 1), WEAK_TEST)?;
    assert_no_start("declared twice", &repo).map(|_| ())
}

#[test]
fn reader_that_shadows_the_name_with_its_own_constant_does_not_start() -> Result<(), String> {
    let old = "pub const GREETING: &str = \"hello\";\n\npub fn greet(name: &str) -> String {\n    const GREETING: &str = \"hey\";\n    format!(\"{GREETING}, {name}\")\n}\n";
    let repo = Repo::create(old, &old.replacen("\"hello\"", "\"hi\"", 1), WEAK_TEST)?;
    assert_no_start("inner const shadow", &repo).map(|_| ())
}

#[test]
fn reader_that_returns_the_constant_through_a_binding_does_not_start() -> Result<(), String> {
    // The returned line does not name the constant, so it is not tied to it.
    let old = "pub const GREETING: &str = \"hello\";\n\npub fn greet(name: &str) -> String {\n    let g = GREETING;\n    format!(\"{g}, {name}\")\n}\n";
    let repo = Repo::create(old, &old.replace("\"hello\"", "\"hi\""), WEAK_TEST)?;
    assert_no_start("through a binding", &repo).map(|_| ())
}

#[test]
fn a_second_reader_without_a_returned_read_keeps_the_constant_unknown() -> Result<(), String> {
    // All readers or none: `loud` reads the constant in a condition only.
    let old = format!(
        "{GREET_OLD}\npub fn loud(name: &str) -> bool {{\n    if name.starts_with(GREETING) {{\n        return true;\n    }}\n    false\n}}\n"
    );
    let repo = Repo::create(&old, &old.replace("\"hello\"", "\"hi\""), WEAK_TEST)?;
    assert_no_start("second reader", &repo).map(|_| ())
}

#[test]
fn changed_static_does_not_start() -> Result<(), String> {
    let old = GREET_OLD.replace("pub const GREETING", "pub static GREETING");
    let repo = Repo::create(&old, &old.replace("\"hello\"", "\"hi\""), WEAK_TEST)?;
    assert_no_start("static", &repo).map(|_| ())
}

#[test]
fn constant_the_value_evaluation_reads_keeps_its_evaluated_answer() -> Result<(), String> {
    // RIPR-SPEC-0048 evaluates `over(5)` with 10 and with 11: both return
    // `false`, so the test cannot notice the change. Moving the probe to
    // `x > LIMIT` would read the literal comparison as a check of it.
    let old = "pub const LIMIT: u32 = 10;\n\npub fn over(x: u32) -> bool {\n    x > LIMIT\n}\n";
    let repo = Repo::create(
        old,
        &old.replace("= 10;", "= 11;"),
        "use fx::over;\n\n#[test]\nfn five_is_not_over() {\n    assert_eq!(over(5), false);\n}\n",
    )?;
    let (check, _) = assert_no_start("evaluated constant", &repo)?;
    if check.contains("Static exposure: exposed") || check.contains("Changed behavior: x > LIMIT") {
        return Err(format!(
            "evaluated constant: check must keep the evaluated answer:\n{check}"
        ));
    }
    Ok(())
}

#[test]
fn constant_read_outside_any_function_does_not_start() -> Result<(), String> {
    // A compile-time assertion reads the constant too; that read is not a
    // returned value, so the change is not described through `greet`.
    let old = format!("{GREET_OLD}\nconst _: () = assert!(!GREETING.is_empty());\n");
    let repo = Repo::create(&old, &old.replace("\"hello\"", "\"hi\""), WEAK_TEST)?;
    assert_no_start("item-level read", &repo).map(|_| ())
}

/// `pub fn n() -> u32 { N }` with `N` changed from `old` to `new`, tested by
/// `test_body`.
fn number_case(old: &str, new: &str, reader: &str, test_body: &str) -> Result<Repo, String> {
    let source = |value: &str| format!("pub const N: {value};\n\n{reader}\n");
    Repo::create(
        &source(old),
        &source(new),
        &format!("use fx::n;\n\n#[test]\nfn n_case() {{\n    {test_body}\n}}\n"),
    )
}

const N_READER: &str = "pub fn n() -> u32 {\n    N\n}";

#[test]
fn bare_name_return_is_left_to_the_value_evaluation_in_both_commands() -> Result<(), String> {
    // `n()` returns `N` itself: the declaration keeps its probe, so neither
    // command starts on `n`'s return while check reads the change as unknown.
    let repo = number_case("u32 = 10", "u32 = 11", N_READER, "assert!(n() > 0);")?;
    let (check, _) = assert_no_start("bare name return", &repo)?;
    if check.contains("Changed behavior: N\n") {
        return Err(format!(
            "bare name return: check must keep the declaration:\n{check}"
        ));
    }
    Ok(())
}

#[test]
fn value_flow_decided_bare_return_reads_exposed_and_does_not_start() -> Result<(), String> {
    // The pin holds on the old value and fails on the new one, so the
    // RIPR-SPEC-0048 evaluation decides the declaration: exposed.
    let repo = number_case("u32 = 10", "u32 = 11", N_READER, "assert_eq!(n(), 10);")?;
    let (check, _) = assert_no_start("exact whole return", &repo)?;
    if !check.contains("Static exposure: exposed") {
        return Err(format!(
            "exact whole return: check must read exposed:\n{check}"
        ));
    }
    Ok(())
}

#[test]
fn same_value_rewritten_does_not_start() -> Result<(), String> {
    let repo = number_case("u32 = 10", "u32 = 5 + 5", N_READER, "assert!(n() > 0);")?;
    assert_no_start("10 to 5 + 5", &repo).map(|_| ())
}

#[test]
fn type_only_change_does_not_start() -> Result<(), String> {
    let repo = number_case(
        "u32 = 10",
        "u64 = 10",
        "pub fn n() -> u64 {\n    N.into()\n}",
        "assert!(n() > 0);",
    )?;
    assert_no_start("type only", &repo).map(|_| ())
}

#[test]
fn visibility_only_change_does_not_start() -> Result<(), String> {
    let old = GREET_OLD.replace("pub const GREETING", "const GREETING");
    let repo = Repo::create(&old, GREET_OLD, WEAK_TEST)?;
    assert_no_start("const to pub const", &repo).map(|_| ())
}

#[test]
fn associated_constant_with_an_imported_free_reader_does_not_start() -> Result<(), String> {
    // (a) The free `greet` reads the imported `defaults::GREETING`, not the
    // edited associated constant.
    let old = "pub mod defaults {\n    pub const GREETING: &str = \"yo\";\n}\n\nuse crate::defaults::GREETING;\n\npub struct Greeter;\n\nimpl Greeter {\n    pub const GREETING: &'static str = \"hello\";\n}\n\npub fn greet(name: &str) -> String {\n    format!(\"{GREETING}, {name}\")\n}\n";
    let repo = Repo::create(old, &old.replace("\"hello\"", "\"hi\""), WEAK_TEST)?;
    assert_no_start("associated const", &repo).map(|_| ())
}

#[test]
fn reader_in_another_module_importing_the_name_does_not_start() -> Result<(), String> {
    // (b) `fr::greet` reads `lang::GREETING` through a `use`.
    let old = "pub const GREETING: &str = \"hello\";\n\npub mod fr {\n    use crate::lang::GREETING;\n\n    pub fn greet(name: &str) -> String {\n        format!(\"{GREETING}, {name}\")\n    }\n}\n";
    let repo = Repo::create(
        old,
        &old.replace("\"hello\"", "\"hi\""),
        "use fx::fr::greet;\n\n#[test]\nfn greet_mentions_name() {\n    assert!(greet(\"bob\").contains(\"bob\"));\n}\n",
    )?;
    assert_no_start("other module", &repo).map(|_| ())
}

#[test]
fn trait_default_read_through_self_does_not_start() -> Result<(), String> {
    // (c) `Self::GREETING` is whatever the implementing type says.
    let old = "pub trait Greet {\n    const GREETING: &'static str = \"hello\";\n\n    fn greet(&self) -> String {\n        Self::GREETING.to_string()\n    }\n}\n\npub struct En;\n\nimpl Greet for En {}\n";
    let repo = Repo::create(
        old,
        &old.replace("\"hello\"", "\"hi\""),
        "use fx::{En, Greet};\n\n#[test]\nfn greet_is_not_empty() {\n    assert!(!En.greet().is_empty());\n}\n",
    )?;
    assert_no_start("trait default", &repo).map(|_| ())
}

#[test]
fn return_inside_a_closure_does_not_start() -> Result<(), String> {
    let old = "pub const GREETING: &str = \"hello\";\n\npub fn greet(name: &str) -> String {\n    [name].iter().map(|_| { return format!(\"{GREETING}\"); }).count().to_string()\n}\n";
    let repo = Repo::create(old, &old.replace("\"hello\"", "\"hi\""), WEAK_TEST)?;
    assert_no_start("closure return", &repo).map(|_| ())
}

#[test]
fn named_format_argument_that_shadows_the_name_does_not_start() -> Result<(), String> {
    let old = "pub const GREETING: &str = \"hello\";\n\npub fn greet(name: &str) -> String {\n    format!(\"{GREETING}\", GREETING = name)\n}\n";
    let repo = Repo::create(old, &old.replace("\"hello\"", "\"hi\""), WEAK_TEST)?;
    assert_no_start("named format argument", &repo).map(|_| ())
}

#[test]
fn conditional_return_of_the_constant_does_not_start() -> Result<(), String> {
    let old = "pub const GREETING: &str = \"hello\";\n\npub fn greet(flag: bool) -> String {\n    if flag {\n        return format!(\"{GREETING}\");\n    }\n    String::new()\n}\n";
    let repo = Repo::create(
        old,
        &old.replace("\"hello\"", "\"hi\""),
        "use fx::greet;\n\n#[test]\nfn greet_is_not_empty() {\n    assert!(!greet(true).is_empty());\n}\n",
    )?;
    assert_no_start("conditional return", &repo).map(|_| ())
}

/// `pub fn n() -> String { format!("{N}") }` with `N` changed from `old` to
/// `new`, loosely tested.
fn number_format_case(old: &str, new: &str) -> Result<Repo, String> {
    let source = |value: &str| {
        format!("pub const N: {value};\n\npub fn n() -> String {{\n    format!(\"{{N}}\")\n}}\n")
    };
    Repo::create(
        &source(old),
        &source(new),
        "use fx::n;\n\n#[test]\nfn n_is_not_empty() {\n    assert!(!n().is_empty());\n}\n",
    )
}

/// Check offers `ripr repair <id>` and bare repair starts on that same id.
fn assert_same_start(case: &str, repo: &Repo) -> Result<(), String> {
    let (check, started, repair) = repo.answers()?;
    let Some(id) = check_start_id(&check) else {
        return Err(format!("{case}: check must offer a repair start:\n{check}"));
    };
    if !started || !repair.contains(&format!("seam `{id}`")) {
        return Err(format!("{case}: bare repair must start on {id}:\n{repair}"));
    }
    Ok(())
}

#[test]
fn changed_number_rendered_by_format_starts_in_both_commands() -> Result<(), String> {
    // Positive twin of the two negatives below.
    let repo = number_format_case("u32 = 10", "u32 = 11")?;
    assert_same_start("number format", &repo)
}

#[test]
fn same_value_rewritten_behind_a_format_reader_does_not_start() -> Result<(), String> {
    let repo = number_format_case("u32 = 10", "u32 = 5 + 5")?;
    assert_no_start("format reader, 10 to 5 + 5", &repo).map(|_| ())
}

#[test]
fn type_only_change_behind_a_format_reader_does_not_start() -> Result<(), String> {
    let repo = number_format_case("u32 = 10", "u64 = 10")?;
    assert_no_start("format reader, u32 to u64", &repo).map(|_| ())
}

#[test]
fn single_return_statement_starts_in_both_commands() -> Result<(), String> {
    let old = "pub const GREETING: &str = \"hello\";\n\npub fn greet(name: &str) -> String {\n    return format!(\"{GREETING}, {name}\");\n}\n";
    let repo = Repo::create(old, &old.replace("\"hello\"", "\"hi\""), WEAK_TEST)?;
    assert_same_start("single return", &repo)
}

#[test]
fn test_that_builds_its_expected_value_from_the_constant_does_not_start() -> Result<(), String> {
    // The verdict-corpus case `spec0048-flow-self-computed-expected`: the
    // expected side tracks the edit, so the pin cannot notice it.
    let old = "pub const BANNER: &str = \"v1\";\n\npub fn banner_line() -> String {\n    format!(\"[{BANNER}]\")\n}\n";
    let repo = Repo::create(
        old,
        &old.replace("\"v1\"", "\"v2\""),
        "use fx::{banner_line, BANNER};\n\n#[test]\nfn banner_line_wraps_banner() {\n    assert_eq!(banner_line(), format!(\"[{BANNER}]\"));\n}\n",
    )?;
    let (check, _) = assert_no_start("self-computed expected", &repo)?;
    if check.contains("Static exposure: exposed") {
        return Err(format!(
            "self-computed expected: check must not read exposed:\n{check}"
        ));
    }
    Ok(())
}

#[test]
fn workspace_format_macro_does_not_start() -> Result<(), String> {
    let old = format!(
        "macro_rules! format {{\n    ($($t:tt)*) => {{\n        String::new()\n    }};\n}}\n\n{GREET_OLD}"
    );
    let repo = Repo::create(&old, &old.replace("\"hello\"", "\"hi\""), WEAK_TEST)?;
    assert_no_start("format macro", &repo).map(|_| ())
}

#[test]
fn cfg_alternative_module_opened_twice_does_not_start() -> Result<(), String> {
    let old = "#[cfg(unix)]\npub mod a {\n    pub const GREETING: &str = \"hello\";\n\n    pub fn greet(name: &str) -> String {\n        format!(\"{GREETING}, {name}\")\n    }\n}\n\n#[cfg(not(unix))]\npub mod a {\n    pub fn greet(name: &str) -> String {\n        format!(\"{GREETING}, {name}\")\n    }\n}\n";
    let repo = Repo::create(
        old,
        &old.replace("\"hello\"", "\"hi\""),
        "use fx::a::greet;\n\n#[test]\nfn greet_mentions_name() {\n    assert!(greet(\"bob\").contains(\"bob\"));\n}\n",
    )?;
    assert_no_start("duplicate module", &repo).map(|_| ())
}

const BANNER_OLD: &str = "pub const BANNER: &str = \"v1\";\n\npub fn banner_line() -> String {\n    format!(\"[{BANNER}]\")\n}\n";

fn banner_new(old: &str) -> String {
    old.replace("\"v1\"", "\"v2\"")
}

/// Neither command starts, and check keeps the declaration as the change
/// (so no exact check of a reader is read as a check of the constant).
fn assert_declaration_kept(case: &str, repo: &Repo) -> Result<(), String> {
    let (check, _) = assert_no_start(case, repo)?;
    if !check.contains("Changed behavior: pub const BANNER") {
        return Err(format!(
            "{case}: check must keep the declaration as the change:\n{check}"
        ));
    }
    Ok(())
}

#[test]
fn loosely_checked_banner_reader_starts_in_both_commands() -> Result<(), String> {
    // Positive twin of the four self-computed negatives below.
    let repo = Repo::create(
        BANNER_OLD,
        &banner_new(BANNER_OLD),
        "use fx::banner_line;\n\n#[test]\nfn banner_is_bracketed() {\n    assert!(banner_line().starts_with('['));\n}\n",
    )?;
    assert_same_start("banner positive", &repo)
}

#[test]
fn helper_in_a_test_file_that_builds_the_expected_value_keeps_the_declaration() -> Result<(), String>
{
    // (a) `expected()` is an ordinary fn in tests/common/mod.rs, not a test.
    let repo = Repo::create_with_files(
        BANNER_OLD,
        &banner_new(BANNER_OLD),
        "mod common;\n\nuse fx::banner_line;\n\n#[test]\nfn banner_line_matches() {\n    assert_eq!(banner_line(), common::expected());\n}\n",
        &[(
            "tests/common/mod.rs",
            "pub fn expected() -> String {\n    format!(\"[{}]\", fx::BANNER)\n}\n",
        )],
    )?;
    assert_declaration_kept("test helper", &repo)
}

#[test]
fn renamed_import_of_the_constant_in_a_test_keeps_the_declaration() -> Result<(), String> {
    // (b) The test reads the constant as `B`.
    let repo = Repo::create(
        BANNER_OLD,
        &banner_new(BANNER_OLD),
        "use fx::banner_line;\nuse fx::BANNER as B;\n\n#[test]\nfn banner_line_matches() {\n    assert_eq!(banner_line(), format!(\"[{B}]\"));\n}\n",
    )?;
    assert_declaration_kept("renamed import", &repo)
}

#[test]
fn test_file_constant_copied_from_the_constant_keeps_the_declaration() -> Result<(), String> {
    // (c) An item outside any function holds the constant's value.
    let repo = Repo::create(
        BANNER_OLD,
        &banner_new(BANNER_OLD),
        "use fx::banner_line;\n\nconst EXP: &str = fx::BANNER;\n\n#[test]\nfn banner_line_matches() {\n    assert_eq!(banner_line(), format!(\"[{EXP}]\"));\n}\n",
    )?;
    assert_declaration_kept("copied constant", &repo)
}

#[test]
fn second_reader_used_for_the_expected_value_keeps_the_declaration() -> Result<(), String> {
    // (d) `tag()` also returns the constant whole; the test builds its
    // expected value from it.
    let old = format!("{BANNER_OLD}\npub fn tag() -> String {{\n    format!(\"{{BANNER}}\")\n}}\n");
    let repo = Repo::create(
        &old,
        &banner_new(&old),
        "use fx::{banner_line, tag};\n\n#[test]\nfn banner_line_matches() {\n    assert_eq!(banner_line(), format!(\"[{}]\", tag()));\n}\n",
    )?;
    assert_declaration_kept("second reader", &repo)
}

#[test]
fn production_caller_of_the_reader_used_for_the_expected_value_keeps_the_declaration()
-> Result<(), String> {
    // `framed()` calls the reader, so it carries the constant's value too.
    let old = format!("{BANNER_OLD}\npub fn framed() -> String {{\n    banner_line()\n}}\n");
    let repo = Repo::create(
        &old,
        &banner_new(&old),
        "use fx::{banner_line, framed};\n\n#[test]\nfn banner_line_matches() {\n    assert_eq!(banner_line(), framed());\n}\n",
    )?;
    assert_declaration_kept("production caller", &repo)
}

/// `BANNER_OLD` plus `extra` appended to `src/lib.rs` on both sides.
fn banner_with(extra: &str) -> (String, String) {
    let old = format!("{BANNER_OLD}{extra}");
    let new = banner_new(&old);
    (old, new)
}

const BANNER_SELF_TEST: &str = "use fx::banner_line;\n\n#[test]\nfn banner_is_bracketed() {\n    assert!(banner_line().starts_with('['));\n}\n";

#[test]
fn out_of_line_test_module_aliasing_the_constant_keeps_the_declaration() -> Result<(), String> {
    // B1: `src/tests.rs` imports and copies the constant before any #[test].
    let (old, new) = banner_with("\n#[cfg(test)]\nmod tests;\n");
    let repo = Repo::create_with_files(
        &old,
        &new,
        BANNER_SELF_TEST,
        &[(
            "src/tests.rs",
            "use crate::BANNER as B;\nconst EXP: &str = crate::BANNER;\n\n#[test]\nfn banner_line_matches() {\n    assert_eq!(crate::banner_line(), format!(\"[{B}]\"));\n    assert!(!EXP.is_empty());\n}\n",
        )],
    )?;
    assert_declaration_kept("src/tests.rs alias", &repo)
}

#[test]
fn production_reexport_alias_keeps_the_declaration() -> Result<(), String> {
    // B2: `pub use crate::BANNER as LABEL;` in another file.
    let (old, new) = banner_with("\npub mod alias;\n");
    let repo = Repo::create_with_files(
        &old,
        &new,
        "use fx::{alias::LABEL, banner_line};\n\n#[test]\nfn banner_line_matches() {\n    assert_eq!(banner_line(), format!(\"[{LABEL}]\"));\n}\n",
        &[("src/alias.rs", "pub use crate::BANNER as LABEL;\n")],
    )?;
    assert_declaration_kept("pub use alias", &repo)
}

#[test]
fn production_constant_copied_by_path_keeps_the_declaration() -> Result<(), String> {
    // B2: `pub const LABEL: &str = crate::BANNER;` in the same file.
    let (old, new) = banner_with("\npub const LABEL: &str = crate::BANNER;\n");
    let repo = Repo::create(
        &old,
        &new,
        "use fx::{banner_line, LABEL};\n\n#[test]\nfn banner_line_matches() {\n    assert_eq!(banner_line(), format!(\"[{LABEL}]\"));\n}\n",
    )?;
    assert_declaration_kept("path-qualified copy", &repo)
}

#[test]
fn production_helper_reading_the_constant_by_path_keeps_the_declaration() -> Result<(), String> {
    // B2: `expected()` in another file reads `crate::BANNER`, which is not
    // counted as a reader by spelling.
    let (old, new) = banner_with("\npub mod other;\n");
    let repo = Repo::create_with_files(
        &old,
        &new,
        "use fx::{banner_line, other::expected};\n\n#[test]\nfn banner_line_matches() {\n    assert_eq!(banner_line(), expected());\n}\n",
        &[(
            "src/other.rs",
            "pub fn expected() -> String {\n    format!(\"[{}]\", crate::BANNER)\n}\n",
        )],
    )?;
    assert_declaration_kept("path-qualified helper", &repo)
}

#[test]
fn doctest_asserting_against_the_constant_keeps_the_declaration() -> Result<(), String> {
    // B3: the doc test builds its expected value from the constant.
    let old = "pub const BANNER: &str = \"v1\";\n\n/// ```\n/// assert_eq!(fx::banner_line(), format!(\"[{}]\", fx::BANNER));\n/// ```\npub fn banner_line() -> String {\n    format!(\"[{BANNER}]\")\n}\n";
    let repo = Repo::create(old, &banner_new(old), BANNER_SELF_TEST)?;
    assert_declaration_kept("doctest", &repo)
}
