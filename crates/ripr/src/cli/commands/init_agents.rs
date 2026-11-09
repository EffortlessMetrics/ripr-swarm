//! `ripr init --agents`: the user-facing agent instruction block.
//!
//! The block is a short find-gap, write-test, verify loop with an explicit
//! done rule and stop rule. It is written into the instruction file each agent
//! already reads, inside a marked section, so a rerun is idempotent and the
//! user's own content outside the markers is never touched.

use super::init::{InitAction, InitTarget, path_is_occupied};
use crate::cli::commands_options::InitAgent;
use crate::cli::suggest::unknown_value;
use crate::domain::LanguageId;
use crate::output;
use std::path::{Path, PathBuf};

/// Project MCP file Cursor reads; same `mcpServers` shape as `.mcp.json`.
pub(super) const CURSOR_MCP_FILE: &str = ".cursor/mcp.json";

const CURSOR_RULE_FILE: &str = ".cursor/rules/ripr.mdc";

const BEGIN_PREFIX: &str = "<!-- ripr:agents:begin";
const BEGIN_LINE: &str = "<!-- ripr:agents:begin (written by `ripr init --agents`; `--force` refreshes this section, keep your own notes outside the markers) -->";
const END_LINE: &str = "<!-- ripr:agents:end -->";

pub(super) const FOLLOW_UP: &str = "Commit the agent instruction file(s) so every session sees them. Register the MCP server only if your agent prefers tools over the CLI: see docs/AGENT_QUICKSTART.md.";

/// The instruction block. `docs/AGENT_QUICKSTART.md` carries the identical
/// text; a unit test keeps the two equal.
pub(super) const AGENT_BLOCK: &str = r#"## ripr: static test-gap finder (Rust). Run it after you change behavior.
ripr reads your diff and says which changed lines no test would notice. It never runs
tests or mutants; "exposed" is static evidence only. It does not edit files.

Before you start: commit the source change; `/target/` must be in .gitignore.
Only edit tests. Never change production code to make a gap go away.
Keep the same --base and source options for every check below.

Loop (max 3 rounds per gap):
1. FIND    `ripr check --quiet --format agent` (add `--base <branch>` if no origin/main|master).
           Read gap, test and next lines. Exit 0 can still mean gaps; 2/3 = stop (read stderr).
2. DETAIL  Follow the printed `ripr explain ...` with its selector and scope. Add `--format agent`
           only if no `--format` is present; for `--from` plus `--suppression-policy`, keep it as printed.
           Follow `ripr context ...` as-is (JSON). Optional: `ripr agent stub --at FILE:LINE --kind KIND [--write]`.
3. WRITE   One focused test next to the related test. Use assert_eq! on exact changed
           values: every changed field, branch and boundary (below, equal, above). Not
           `.is_some()`, `.is_empty()`, `<=`, or only whole-struct equality.
4. VERIFY  `cargo test <your_test_name>` must pass. Then run `ripr check --quiet --format agent`
           once more on the same root, base and saved source (new files need `git add -N PATH`).
           Check its exit status before reading it; do not change inputs before DONE.
5. DONE    Use that same report; require exit 0 and its last line `verify=complete|...`.
           `verify=refused`, a missing `verify=` line, or a cut-off report mean STOP.
           `done=` or an empty gap list alone is never enough.
           If gaps remain, return to FIND; if inputs change, repeat VERIFY before DONE.
           Report tests and static result; this does not establish runtime bug detection.

STOP and report instead of looping when:
- the outcome names empty scope, no changed behavior, partial analysis, unsupported input or failed analysis;
- the top finding says "Analyzer limit" / "Tests cannot close this static-analysis limitation";
- the same gap remains after 3 exact-assert tests (it may be an equivalent change, e.g. `>` vs
  `>=` where both branches return the same value); or `ripr check` exits 2 or 3.

Optional guided repair (Rust seams ripr can bind to one test file):
  `ripr repair <seam-id-or-probe-id> --root .` -> edit only the named test file -> `ripr continue`
  If it refuses (exit 2 or 3), fall back to steps 3-5. `ripr status --root .` resumes an attempt.

Do not read target/ripr/pilot/*.json (2-4 MB) or docs/OUTPUT_SCHEMA.md (1 MB).
MCP is optional and read-only; the CLI above is cheaper. Setup: ripr init --mcp
"#;

/// Languages the written instructions cover. A Rust-only (or undetected) root
/// keeps the Rust text byte for byte; Python and TypeScript/JavaScript get the
/// verify command and assertion advice that fit them.
///
/// A preview language counts only when it is both detected and enabled for
/// `ripr check`; a detected language that is not enabled is named in a note
/// instead, so the instructions never promise analysis that will not run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AgentLanguages {
    pub(super) rust: bool,
    pub(super) python: bool,
    pub(super) typescript: bool,
    /// Python detected but not enabled.
    pub(super) python_not_enabled: bool,
    /// TypeScript/JavaScript detected but not enabled.
    pub(super) typescript_not_enabled: bool,
}

impl AgentLanguages {
    #[cfg(test)]
    pub(super) const RUST: Self = Self {
        rust: true,
        python: false,
        typescript: false,
        python_not_enabled: false,
        typescript_not_enabled: false,
    };

    /// Detect from the file markers `ripr check` uses, keeping the preview
    /// languages `enabled` (the effective `[languages].enabled`) turns on.
    /// Nothing covered means Rust, the primary language.
    pub(super) fn detect(root: &Path, enabled: &[LanguageId]) -> Self {
        let found = crate::config::language_detection::detect_languages(root);
        Self::from_languages(&found, enabled)
    }

    fn from_languages(found: &[LanguageId], enabled: &[LanguageId]) -> Self {
        let python_found = found.contains(&LanguageId::Python);
        let typescript_found =
            found.contains(&LanguageId::TypeScript) || found.contains(&LanguageId::JavaScript);
        // The config's `javascript` alias parses to `TypeScript`.
        let python = python_found && enabled.contains(&LanguageId::Python);
        let typescript = typescript_found && enabled.contains(&LanguageId::TypeScript);
        let rust = found.contains(&LanguageId::Rust) || !(python || typescript);
        Self {
            rust,
            python,
            typescript,
            python_not_enabled: python_found && !python,
            typescript_not_enabled: typescript_found && !typescript,
        }
    }

    /// Note for detected languages `ripr check` will not analyze; empty when
    /// every detected language is enabled.
    fn not_enabled_note(self) -> String {
        let (label, entries) = match (self.python_not_enabled, self.typescript_not_enabled) {
            (false, false) => return String::new(),
            (true, true) => (
                "Python and TypeScript/JavaScript",
                "`python` and `typescript`",
            ),
            (true, false) => ("Python", "`python`"),
            (false, true) => ("TypeScript/JavaScript", "`typescript`"),
        };
        format!(
            "\n{label} files here are NOT analyzed: ripr.toml `[languages].enabled` lacks {entries}\n(or this ripr build has no support for them), so `ripr check` reports nothing for them.\nRun `ripr doctor` for the fix; do not treat them as covered.\n"
        )
    }

    fn preview_label(self) -> &'static str {
        match (self.python, self.typescript) {
            (true, true) => "Python and TypeScript/JavaScript",
            (true, false) => "Python",
            _ => "TypeScript/JavaScript",
        }
    }

    fn verify_command(self) -> &'static str {
        match (self.python, self.typescript) {
            (true, true) => {
                "your test runner on the new test (Python: `pytest -k <name>` or `python -m unittest <module.Class.test_name>`; TypeScript: `npx jest -t \"<name>\"` or `npx vitest run -t \"<name>\"`)"
            }
            (true, false) => {
                "`pytest -k <your_test_name>` (or `python -m unittest <module.Class.test_name>`)"
            }
            _ => "`npx jest -t \"<name>\"` (or `npx vitest run -t \"<name>\"`)",
        }
    }

    fn assertion_advice(self) -> &'static str {
        match (self.python, self.typescript) {
            (true, true) => {
                "Python: `assert result == expected` or `self.assertEqual`, not bare\n           `assert result`, `is not None` or `len(x) > 0`. TypeScript: `toBe` / `toEqual` on\n           exact values, not `toBeTruthy()` or `toBeDefined()`."
            }
            (true, false) => {
                "Use `assert result == expected` (or `self.assertEqual`)\n           on exact values, not bare `assert result`, `is not None` or `len(x) > 0`."
            }
            _ => {
                "Use `toBe` / `toEqual` on exact values, not `toBeTruthy()`,\n           `toBeDefined()` or `not.toThrow()` alone."
            }
        }
    }

    /// The instruction text written between the markers.
    pub(super) fn block(self) -> String {
        format!("{}{}", self.language_block(), self.not_enabled_note())
    }

    fn language_block(self) -> String {
        let preview = self.python || self.typescript;
        if !preview {
            return AGENT_BLOCK.to_string();
        }
        if self.rust {
            return format!(
                "{AGENT_BLOCK}\nPython/TypeScript files here are a preview and their results are advisory: steps 1, 2\nand 5 are the same, but `ripr repair` and `ripr agent stub` are Rust only. Step 5 still\nneeds `summary.findings` = `summary.exposed` for those files too. Write the test by\nhand next to the related test with exact values, and verify it with {}.\n",
                self.verify_command()
            );
        }
        format!(
            r#"## ripr: static test-gap finder ({label}, preview). Run it after you change behavior.
ripr reads your diff and says which changed lines no test would notice. It never runs
tests; "exposed" is static evidence only. It does not edit files. {label} support is a
preview, so results are advisory.

Before you start: commit the source change (new files need `git add -N PATH`).
Only edit tests. Never change production code to make a gap go away.
Keep the same --base and source options for every check below.

Loop (max 3 rounds per gap):
1. FIND    `ripr check --quiet --format agent` (add `--base <branch>` if no origin/main|master).
           Read gap, test and next lines. Exit 0 can still mean gaps; 2/3 = stop (read stderr).
           An `open=`, `stop=` or `done=` line saying preview evidence is advisory is expected.
2. DETAIL  Follow the printed `ripr explain ...` command as printed, then the `ripr context ...`
           it names (JSON).
3. WRITE   One focused test next to the related test. Assert exact changed values: every
           changed field, branch and boundary (below, equal, above). {advice}
4. VERIFY  Run {verify}; it must pass.
           Then run `ripr check --quiet --format json-compact` once on the same root, base and
           saved source (new files need `git add -N PATH`). The new test must be in the
           analyzed source: commit it, or pass `--worktree` to both FIND and VERIFY. Check that
           run's exit status before reading the JSON; do not change inputs before DONE.
5. DONE    Use that same report; require exit 0 and these fields:
           `analysis_outcome.outcome.kind`: `complete_no_findings` or `complete_with_findings`;
           `analysis_outcome.analysis_complete`: true; `summary.findings` = `summary.exposed`.
           Missing required fields, `analysis_outcome.document_complete: false`, or
           `unanalyzed_working_tree: true` mean STOP.
           `gaps=0` in the agent header is not enough: `no_static_path`, `reachable_unrevealed`
           and `*_unknown` findings are not exposed. If findings remain, return to FIND.
           Report tests and static result; this does not establish runtime bug detection.

STOP and report instead of looping when:
- the outcome names empty scope, no changed behavior, partial analysis, unsupported input or failed analysis;
- the top finding says "Analyzer limit" / "Tests cannot close this static-analysis limitation";
- the same gap remains after 3 exact-assert tests (it may be an equivalent change, e.g. `>` vs
  `>=` where both branches return the same value); or `ripr check` exits 2 or 3.

`ripr repair` and `--seam-id` are Rust only. If `ripr check` prints a
`ripr agent repair --gap-id ...` start, run that exact command; otherwise use the manual loop.

Do not read target/ripr/pilot/*.json (2-4 MB) or docs/OUTPUT_SCHEMA.md (1 MB).
MCP is optional and read-only; the CLI above is cheaper. Setup: ripr init --mcp
"#,
            label = self.preview_label(),
            advice = self.assertion_advice(),
            verify = self.verify_command(),
        )
    }

    /// Cursor rule front matter: globs and description follow the languages.
    fn cursor_frontmatter(self) -> String {
        let mut globs: Vec<&str> = Vec::new();
        let mut names: Vec<&str> = Vec::new();
        if self.rust {
            globs.push("\"**/*.rs\"");
            names.push("Rust");
        }
        if self.python {
            globs.push("\"**/*.py\"");
            names.push("Python");
        }
        if self.typescript {
            globs.extend([
                "\"**/*.ts\"",
                "\"**/*.tsx\"",
                "\"**/*.mts\"",
                "\"**/*.cts\"",
                "\"**/*.js\"",
                "\"**/*.jsx\"",
                "\"**/*.mjs\"",
                "\"**/*.cjs\"",
            ]);
            names.push("TypeScript");
        }
        format!(
            "---\ndescription: ripr static test-gap finder; run it after changing {} behavior\nalwaysApply: false\nglobs: [{}]\n---\n",
            names.join("/"),
            globs.join(", ")
        )
    }

    /// Byte length of front matter at the start of `text` that ripr itself
    /// wrote for some language set, `None` otherwise. Only an exact match of a
    /// generated shape counts, so user-authored front matter is never touched.
    fn generated_cursor_frontmatter_len(text: &str) -> Option<usize> {
        (1u8..8)
            .map(|bits| Self {
                rust: bits & 1 != 0,
                python: bits & 2 != 0,
                typescript: bits & 4 != 0,
                python_not_enabled: false,
                typescript_not_enabled: false,
            })
            .map(Self::cursor_frontmatter)
            .find(|front| text.starts_with(front.as_str()))
            .map(|front| front.len())
    }
}

/// File each agent reads, relative to the init root.
pub(super) fn instruction_file(agent: InitAgent) -> &'static str {
    match agent {
        InitAgent::Claude => "CLAUDE.md",
        InitAgent::Codex => "AGENTS.md",
        InitAgent::Cursor => CURSOR_RULE_FILE,
    }
}

pub(super) fn wants_cursor(agents: &[InitAgent]) -> bool {
    agents.contains(&InitAgent::Cursor)
}

/// True for a path `--agents` may have written, used for the follow-up note.
pub(super) fn is_instruction_file(path: &Path) -> bool {
    [InitAgent::Claude, InitAgent::Codex, InitAgent::Cursor]
        .iter()
        .any(|agent| path.ends_with(instruction_file(*agent)))
}

/// Parse one `--agents` value; `all` expands to every target. Repeats merge
/// without duplicates, keeping request order.
pub(super) fn parse_agents(value: &str, agents: &mut Vec<InitAgent>) -> Result<(), String> {
    let requested: &[InitAgent] = match value {
        "claude" => &[InitAgent::Claude],
        "codex" => &[InitAgent::Codex],
        "cursor" => &[InitAgent::Cursor],
        "all" => &[InitAgent::Claude, InitAgent::Codex, InitAgent::Cursor],
        _ => {
            return Err(unknown_value(
                "init --agents target",
                value,
                &["claude", "codex", "cursor", "all"],
            ));
        }
    };
    for agent in requested {
        if !agents.contains(agent) {
            agents.push(*agent);
        }
    }
    Ok(())
}

fn marked_section(languages: AgentLanguages) -> String {
    format!("{BEGIN_LINE}\n{}{END_LINE}\n", languages.block())
}

/// Byte range of an existing marked section, `None` when there is none.
/// A begin marker without a later end marker fails closed: replacing from an
/// unterminated marker could delete the user's content.
///
/// Markers count only as whole lines (the begin line starts with the prefix, the
/// end line is exactly the end marker), so prose that merely mentions a marker
/// is user text. Exactly one begin line followed by exactly one end line is a
/// managed section; zero of both is none; every other layout (repeated, nested,
/// reversed, orphan end) fails closed, because replacing across it could delete
/// the user's content between or after the markers.
fn find_section(text: &str, shown: &str) -> Result<Option<(usize, usize)>, String> {
    let mut begins = Vec::new();
    let mut ends = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        if bare.starts_with(BEGIN_PREFIX) {
            begins.push(offset);
        } else if bare == END_LINE {
            ends.push(offset + line.len());
        }
        offset += line.len();
    }
    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => Ok(None),
        ([start], [end]) if start < end => Ok(Some((*start, *end))),
        ([_], []) => Err(format!(
            "existing {shown} has a ripr section start marker without its end marker; fix or remove the marker, nothing was written"
        )),
        _ => Err(format!(
            "existing {shown} has repeated, nested, reversed or unpaired ripr section markers; leave exactly one begin and one end marker line, or remove them, nothing was written"
        )),
    }
}

/// Plan one agent instruction file: create it, append the marked section to
/// the user's file, refresh the section with `--force`, or leave it alone.
pub(super) fn plan_agent_target(
    agent: InitAgent,
    path: PathBuf,
    force: bool,
    languages: AgentLanguages,
) -> Result<InitTarget, String> {
    let shown = output::path::human_path(&path);
    let section = marked_section(languages);
    if !path_is_occupied(&path)? {
        let mut body = String::new();
        if agent == InitAgent::Cursor {
            body.push_str(&languages.cursor_frontmatter());
        }
        body.push_str(&section);
        return Ok(InitTarget {
            path,
            action: InitAction::Create,
            body,
        });
    }
    // The atomic write swaps the directory entry, which would replace a
    // symlinked (shared or dotfiles-managed) file with a plain copy.
    if std::fs::symlink_metadata(&path)
        .map_err(|err| format!("cannot inspect {shown}: {err}"))?
        .file_type()
        .is_symlink()
    {
        return Err(format!(
            "existing {shown} is a symlink; add the ripr section to its target by hand (`ripr init --agents {} --dry-run` prints it), nothing was written",
            agent_name(agent)
        ));
    }
    if !path.is_file() {
        return Err(format!(
            "existing {shown} is not a regular file; remove it or pass another --root, nothing was written"
        ));
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|err| format!("cannot read existing {shown}: {err}; fix its permissions or encoding, or remove it, nothing was written"))?;
    match find_section(&text, &shown)? {
        Some((start, end)) => {
            let mut body = format!("{}{}{}", &text[..start], section, &text[end..]);
            // A refresh also brings ripr's own Cursor front matter (its globs)
            // up to the current languages; anything else before the section
            // is the user's and stays as written.
            if agent == InitAgent::Cursor
                && let Some(len) = AgentLanguages::generated_cursor_frontmatter_len(&body)
                && len <= start
            {
                body.replace_range(..len, &languages.cursor_frontmatter());
            }
            if body == text || !force {
                return Ok(InitTarget {
                    path,
                    action: InitAction::LeaveUnchanged,
                    body: text,
                });
            }
            Ok(InitTarget {
                path,
                action: InitAction::Update,
                body,
            })
        }
        None => {
            let mut body = text;
            if !body.is_empty() && !body.ends_with('\n') {
                body.push('\n');
            }
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(&section);
            Ok(InitTarget {
                path,
                action: InitAction::Update,
                body,
            })
        }
    }
}

fn agent_name(agent: InitAgent) -> &'static str {
    match agent {
        InitAgent::Claude => "claude",
        InitAgent::Codex => "codex",
        InitAgent::Cursor => "cursor",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> Result<PathBuf, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-init-agents-{name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).map_err(|err| err.to_string())?;
        Ok(root)
    }

    fn plan(agent: InitAgent, path: &Path, force: bool) -> Result<InitTarget, String> {
        plan_agent_target(agent, path.to_path_buf(), force, AgentLanguages::RUST)
    }

    /// Plan, then apply the way `apply_init_plan` writes an update.
    fn apply(target: &InitTarget) -> Result<(), String> {
        if target.action == InitAction::LeaveUnchanged {
            return Ok(());
        }
        if let Some(parent) = target.path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        std::fs::write(&target.path, &target.body).map_err(|err| err.to_string())
    }

    fn read(path: &Path) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|err| err.to_string())
    }

    #[test]
    fn block_fits_the_budget_and_names_the_stop_rules() {
        assert!(
            AGENT_BLOCK.lines().count() <= 40,
            "block grew past 40 lines"
        );
        for needle in [
            "DONE",
            "STOP and report",
            "Analyzer limit",
            "exits 2 or 3",
            "ripr repair",
        ] {
            assert!(AGENT_BLOCK.contains(needle), "block lost {needle:?}");
        }
    }

    #[test]
    fn block_uses_one_complete_post_edit_report() {
        let find = AGENT_BLOCK.split("1. FIND").nth(1).unwrap_or("");
        let verify = AGENT_BLOCK.split("4. VERIFY").nth(1).unwrap_or("");
        let done = verify.split("5. DONE").nth(1).unwrap_or("");
        let done = done.split("STOP and report").next().unwrap_or("");
        assert!(find.contains("ripr check --quiet --format agent"));
        assert!(find.contains("Add `--format agent`"));
        assert!(find.contains(
            "only if no `--format` is present; for `--from` plus `--suppression-policy`"
        ));
        assert!(find.contains("keep it as printed"));
        assert_eq!(
            verify.matches("ripr check --quiet --format agent").count(),
            1
        );
        assert!(
            !AGENT_BLOCK.contains("json-compact"),
            "ordinary verification must not require the full JSON document"
        );
        assert!(!done.contains("ripr check"));
        for condition in [
            "same report",
            "last line `verify=complete|...`",
            "`verify=refused`",
            "a missing `verify=` line",
            "cut-off report",
            "`done=` or an empty gap list alone is never enough",
            "if inputs change, repeat VERIFY",
        ] {
            assert!(done.contains(condition), "block lost {condition:?}");
        }
    }

    #[test]
    fn quickstart_page_carries_the_identical_block() -> Result<(), String> {
        let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/AGENT_QUICKSTART.md");
        let text = read(&page)?;
        assert!(
            text.contains(AGENT_BLOCK),
            "docs/AGENT_QUICKSTART.md must contain the exact block `ripr init --agents` writes"
        );
        Ok(())
    }

    #[test]
    fn create_writes_a_marked_section_and_cursor_gets_frontmatter() -> Result<(), String> {
        let root = temp_root("create")?;
        let claude = plan(InitAgent::Claude, &root.join("CLAUDE.md"), false)?;
        assert_eq!(claude.action, InitAction::Create);
        assert!(claude.body.starts_with(BEGIN_PREFIX));
        assert!(claude.body.ends_with(&format!("{END_LINE}\n")));
        let cursor = plan(InitAgent::Cursor, &root.join(CURSOR_RULE_FILE), false)?;
        assert!(cursor.body.starts_with("---\n"));
        assert!(cursor.body.contains(BEGIN_PREFIX));
        Ok(())
    }

    #[test]
    fn append_keeps_the_users_content_and_a_rerun_is_idempotent() -> Result<(), String> {
        let root = temp_root("idem")?;
        let path = root.join("AGENTS.md");
        let mine = "# My project\n\nUse tabs.";
        std::fs::write(&path, mine).map_err(|err| err.to_string())?;
        let first = plan(InitAgent::Codex, &path, false)?;
        assert_eq!(first.action, InitAction::Update);
        assert!(first.body.starts_with("# My project\n\nUse tabs.\n\n"));
        apply(&first)?;
        let after_first = read(&path)?;
        let second = plan(InitAgent::Codex, &path, false)?;
        assert_eq!(second.action, InitAction::LeaveUnchanged);
        apply(&second)?;
        assert_eq!(
            read(&path)?,
            after_first,
            "a rerun must not change the file"
        );
        let forced = plan(InitAgent::Codex, &path, true)?;
        assert_eq!(forced.action, InitAction::LeaveUnchanged);
        assert_eq!(forced.body, after_first);
        assert_eq!(after_first.matches(BEGIN_PREFIX).count(), 1);
        Ok(())
    }

    #[test]
    fn an_edited_section_is_kept_unless_forced_and_force_keeps_the_rest() -> Result<(), String> {
        let root = temp_root("force")?;
        let path = root.join("CLAUDE.md");
        let text = format!("before\n{BEGIN_LINE}\nstale edited text\n{END_LINE}\nafter\n");
        std::fs::write(&path, &text).map_err(|err| err.to_string())?;
        let kept = plan(InitAgent::Claude, &path, false)?;
        assert_eq!(kept.action, InitAction::LeaveUnchanged);
        assert_eq!(
            kept.body, text,
            "no-force must not rewrite an edited section"
        );
        let forced = plan(InitAgent::Claude, &path, true)?;
        assert_eq!(forced.action, InitAction::Update);
        assert_eq!(
            forced.body,
            format!("before\n{}after\n", marked_section(AgentLanguages::RUST)),
            "force must replace exactly the marked section and nothing else"
        );
        Ok(())
    }

    #[test]
    fn marker_text_in_prose_is_not_a_section() -> Result<(), String> {
        let root = temp_root("prose")?;
        let path = root.join("CLAUDE.md");
        let text = format!("Docs: write `{BEGIN_PREFIX}` to start a section.\nmore\n");
        std::fs::write(&path, &text).map_err(|err| err.to_string())?;
        let planned = plan(InitAgent::Claude, &path, true)?;
        assert_eq!(planned.action, InitAction::Update);
        assert!(planned.body.starts_with(&text), "user prose must be kept");
        assert!(
            planned
                .body
                .ends_with(&marked_section(AgentLanguages::RUST))
        );
        Ok(())
    }

    const PYTHON: AgentLanguages = AgentLanguages {
        rust: false,
        python: true,
        typescript: false,
        python_not_enabled: false,
        typescript_not_enabled: false,
    };
    const TYPESCRIPT: AgentLanguages = AgentLanguages {
        rust: false,
        python: false,
        typescript: true,
        python_not_enabled: false,
        typescript_not_enabled: false,
    };
    const ALL_ENABLED: [LanguageId; 3] =
        [LanguageId::Rust, LanguageId::Python, LanguageId::TypeScript];

    #[test]
    fn rust_root_keeps_the_rust_text_and_globs() {
        assert_eq!(AgentLanguages::RUST.block(), AGENT_BLOCK);
        let front = AgentLanguages::RUST.cursor_frontmatter();
        assert!(front.contains("changing Rust behavior"), "{front}");
        assert!(front.contains("globs: [\"**/*.rs\"]"), "{front}");
    }

    #[test]
    fn python_block_names_pytest_and_does_not_promise_guided_repair() {
        let block = PYTHON.block();
        for needle in [
            "(Python, preview)",
            "pytest -k",
            "unittest",
            "assert result == expected",
        ] {
            assert!(block.contains(needle), "python block lost {needle:?}");
        }
        for banned in [
            "cargo",
            "assert_eq!",
            "/target/",
            "Optional guided repair",
            "(Rust",
        ] {
            assert!(!block.contains(banned), "python block mentions {banned:?}");
        }
        assert!(block.contains("`ripr repair` and `--seam-id` are Rust only"));
        assert!(block.contains("run that exact command"));
        assert!(
            !block.contains("gaps=0` with no"),
            "weak DONE rule came back"
        );
        for needle in [
            "`summary.findings` = `summary.exposed`",
            "analysis_outcome.analysis_complete",
            "complete_no_findings",
            "empty scope, no changed behavior, partial analysis",
            "Analyzer limit",
            "after 3 exact-assert tests",
            "exits 2 or 3",
            "`unanalyzed_working_tree: true` mean STOP",
            "commit it, or pass `--worktree` to both FIND and VERIFY",
        ] {
            assert!(block.contains(needle), "python block lost {needle:?}");
            assert!(
                TYPESCRIPT.block().contains(needle),
                "typescript block lost {needle:?}"
            );
        }
        let front = PYTHON.cursor_frontmatter();
        assert!(front.contains("changing Python behavior"), "{front}");
        assert!(front.contains("globs: [\"**/*.py\"]"), "{front}");
    }

    #[test]
    fn typescript_block_names_jest_and_vitest_and_globs_cover_ts_and_js() {
        let block = TYPESCRIPT.block();
        for needle in [
            "TypeScript/JavaScript, preview",
            "npx jest",
            "npx vitest",
            "toBe",
        ] {
            assert!(block.contains(needle), "typescript block lost {needle:?}");
        }
        for banned in [
            "cargo",
            "assert_eq!",
            "/target/",
            "Optional guided repair",
            "pytest",
        ] {
            assert!(
                !block.contains(banned),
                "typescript block mentions {banned:?}"
            );
        }
        let front = TYPESCRIPT.cursor_frontmatter();
        for glob in ["**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"] {
            assert!(front.contains(glob), "{front}");
        }
        assert!(!front.contains("**/*.rs"), "{front}");
    }

    #[test]
    fn mixed_rust_and_python_keeps_the_rust_block_and_adds_the_preview_note() {
        let mixed = AgentLanguages {
            python: true,
            ..AgentLanguages::RUST
        };
        let block = mixed.block();
        assert!(block.starts_with(AGENT_BLOCK));
        assert!(block.contains("`ripr repair` and `ripr agent stub` are Rust only"));
        assert!(block.contains("pytest -k"));
        assert!(block.contains("advisory"));
        assert!(block.contains("`summary.findings` = `summary.exposed`"));
        let front = mixed.cursor_frontmatter();
        assert!(front.contains("Rust/Python behavior"), "{front}");
        assert!(front.contains("\"**/*.rs\", \"**/*.py\""), "{front}");
    }

    #[test]
    fn detect_reads_the_root_markers() -> Result<(), String> {
        let root = temp_root("detect")?;
        assert_eq!(
            AgentLanguages::detect(&root, &ALL_ENABLED),
            AgentLanguages::RUST
        );
        std::fs::write(root.join("pyproject.toml"), "[project]\nname = \"x\"\n")
            .map_err(|err| err.to_string())?;
        assert_eq!(AgentLanguages::detect(&root, &ALL_ENABLED), PYTHON);
        std::fs::write(root.join("package.json"), "{}").map_err(|err| err.to_string())?;
        assert_eq!(
            AgentLanguages::detect(&root, &ALL_ENABLED),
            AgentLanguages {
                python: true,
                ..TYPESCRIPT
            }
        );
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n")
            .map_err(|err| err.to_string())?;
        assert!(AgentLanguages::detect(&root, &ALL_ENABLED).rust);
        let planned = plan_agent_target(
            InitAgent::Cursor,
            root.join(CURSOR_RULE_FILE),
            false,
            AgentLanguages::detect(&root, &ALL_ENABLED),
        )?;
        assert!(planned.body.contains("**/*.py"), "{}", planned.body);
        Ok(())
    }

    #[test]
    fn detected_but_not_enabled_languages_are_not_promised() {
        let found = [LanguageId::Python, LanguageId::JavaScript];
        // `enabled = ["rust"]` in a Python + JavaScript repo: no preview block.
        let rust_only = AgentLanguages::from_languages(&found, &[LanguageId::Rust]);
        assert_eq!(
            rust_only,
            AgentLanguages {
                python_not_enabled: true,
                typescript_not_enabled: true,
                ..AgentLanguages::RUST
            }
        );
        let block = rust_only.block();
        assert!(block.starts_with(AGENT_BLOCK), "{block}");
        for banned in [
            "pytest",
            "npx jest",
            "(Python, preview)",
            "Python/TypeScript files here are a preview",
        ] {
            assert!(
                !block.contains(banned),
                "disabled language promised: {banned:?}"
            );
        }
        assert!(
            block.contains("Python and TypeScript/JavaScript files here are NOT analyzed"),
            "{block}"
        );
        assert!(block.contains("lacks `python` and `typescript`"), "{block}");
        assert!(block.contains("Run `ripr doctor` for the fix"), "{block}");
        let front = rust_only.cursor_frontmatter();
        assert!(front.contains("globs: [\"**/*.rs\"]"), "{front}");

        // Python enabled, JavaScript not: Python preview plus a TS note only.
        let python_only =
            AgentLanguages::from_languages(&found, &[LanguageId::Rust, LanguageId::Python]);
        assert_eq!(
            python_only,
            AgentLanguages {
                typescript_not_enabled: true,
                ..PYTHON
            }
        );
        let block = python_only.block();
        assert!(block.contains("(Python, preview)"), "{block}");
        assert!(!block.contains("npx jest"), "{block}");
        assert!(
            block.contains("TypeScript/JavaScript files here are NOT analyzed"),
            "{block}"
        );
        assert!(!block.contains("Python and TypeScript"), "{block}");

        // Everything enabled: no note.
        let all = AgentLanguages::from_languages(&found, &ALL_ENABLED);
        assert!(!all.block().contains("NOT analyzed"));
        assert!(all.python && all.typescript && !all.rust);
    }

    #[test]
    fn force_refreshes_generated_cursor_frontmatter_but_not_user_frontmatter() -> Result<(), String>
    {
        let root = temp_root("cursor-front")?;
        let path = root.join(CURSOR_RULE_FILE);
        let mixed = AgentLanguages {
            python: true,
            ..AgentLanguages::RUST
        };
        apply(&plan(InitAgent::Cursor, &path, false)?)?;
        let rust_text = read(&path)?;
        assert!(rust_text.starts_with(&AgentLanguages::RUST.cursor_frontmatter()));
        // The user adds notes after the section.
        let with_notes = format!("{rust_text}\nMy own rule.\n");
        std::fs::write(&path, &with_notes).map_err(|err| err.to_string())?;

        // Python appears; without --force nothing changes.
        let kept = plan_agent_target(InitAgent::Cursor, path.clone(), false, mixed)?;
        assert_eq!(kept.action, InitAction::LeaveUnchanged);
        assert_eq!(kept.body, with_notes);

        let forced = plan_agent_target(InitAgent::Cursor, path.clone(), true, mixed)?;
        assert_eq!(forced.action, InitAction::Update);
        assert_eq!(
            forced.body,
            format!(
                "{}{}\nMy own rule.\n",
                mixed.cursor_frontmatter(),
                marked_section(mixed)
            ),
            "force must refresh ripr's front matter and section and keep the user's notes"
        );
        assert!(
            forced.body.contains("\"**/*.rs\", \"**/*.py\""),
            "{}",
            forced.body
        );
        apply(&forced)?;
        let again = plan_agent_target(InitAgent::Cursor, path.clone(), true, mixed)?;
        assert_eq!(
            again.action,
            InitAction::LeaveUnchanged,
            "refresh is idempotent"
        );

        // User-authored front matter is never rewritten, even with --force.
        let custom = format!(
            "---\ndescription: my team rule\nglobs: [\"src/**/*.rs\"]\n---\n{}",
            marked_section(AgentLanguages::RUST)
        );
        std::fs::write(&path, &custom).map_err(|err| err.to_string())?;
        let forced = plan_agent_target(InitAgent::Cursor, path.clone(), true, mixed)?;
        assert_eq!(forced.action, InitAction::Update);
        assert_eq!(
            forced.body,
            format!(
                "---\ndescription: my team rule\nglobs: [\"src/**/*.rs\"]\n---\n{}",
                marked_section(mixed)
            )
        );
        // A ripr-shaped front matter in a non-Cursor file is user text.
        let claude = root.join("CLAUDE.md");
        let text = format!(
            "{}{}",
            AgentLanguages::RUST.cursor_frontmatter(),
            marked_section(AgentLanguages::RUST)
        );
        std::fs::write(&claude, &text).map_err(|err| err.to_string())?;
        let forced = plan_agent_target(InitAgent::Claude, claude, true, mixed)?;
        assert!(
            forced
                .body
                .starts_with(&AgentLanguages::RUST.cursor_frontmatter())
        );
        Ok(())
    }

    #[test]
    fn repeated_nested_reversed_and_orphan_markers_fail_closed() -> Result<(), String> {
        let root = temp_root("layouts")?;
        let path = root.join("CLAUDE.md");
        let layouts = [
            format!("{BEGIN_LINE}\na\n{END_LINE}\nmine\n{BEGIN_LINE}\nb\n{END_LINE}\n"),
            format!("{BEGIN_LINE}\na\n{BEGIN_LINE}\nb\n{END_LINE}\nmine\n"),
            format!("{BEGIN_LINE}\na\n{END_LINE}\nmine\n{END_LINE}\n"),
            format!("mine\n{END_LINE}\n{BEGIN_LINE}\na\n"),
            format!("mine\n{END_LINE}\nmore\n"),
        ];
        for text in &layouts {
            std::fs::write(&path, text).map_err(|err| err.to_string())?;
            for force in [false, true] {
                let err = plan(InitAgent::Claude, &path, force)
                    .err()
                    .ok_or("an ambiguous marker layout must be refused")?;
                assert!(err.contains("nothing was written"), "{err}");
            }
            assert_eq!(read(&path)?, *text, "the file must be untouched");
        }
        Ok(())
    }

    #[test]
    fn unterminated_marker_fails_closed_without_writing() -> Result<(), String> {
        let root = temp_root("unterminated")?;
        let path = root.join("CLAUDE.md");
        let text = format!("keep me\n{BEGIN_LINE}\nmy notes with no end marker\n");
        std::fs::write(&path, &text).map_err(|err| err.to_string())?;
        for force in [false, true] {
            let err = plan(InitAgent::Claude, &path, force)
                .err()
                .ok_or("an unterminated marker must be refused")?;
            assert!(err.contains("without its end marker"), "{err}");
        }
        assert_eq!(read(&path)?, text);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_and_non_file_targets_are_refused() -> Result<(), String> {
        let root = temp_root("symlink")?;
        let real = root.join("real.md");
        std::fs::write(&real, "mine\n").map_err(|err| err.to_string())?;
        let link = root.join("CLAUDE.md");
        std::os::unix::fs::symlink(&real, &link).map_err(|err| err.to_string())?;
        let err = plan(InitAgent::Claude, &link, true)
            .err()
            .ok_or("a symlinked instruction file must be refused")?;
        assert!(err.contains("is a symlink"), "{err}");
        let dir = root.join("AGENTS.md");
        std::fs::create_dir(&dir).map_err(|err| err.to_string())?;
        let err = plan(InitAgent::Codex, &dir, true)
            .err()
            .ok_or("a directory must be refused")?;
        assert!(err.contains("not a regular file"), "{err}");
        assert_eq!(read(&real)?, "mine\n");
        Ok(())
    }

    #[test]
    fn non_utf8_content_is_refused_not_clobbered() -> Result<(), String> {
        let root = temp_root("binary")?;
        let path = root.join("CLAUDE.md");
        std::fs::write(&path, [0xff, 0xfe, 0x00]).map_err(|err| err.to_string())?;
        let err = plan(InitAgent::Claude, &path, true)
            .err()
            .ok_or("unreadable text must be refused")?;
        assert!(err.contains("cannot read existing"), "{err}");
        Ok(())
    }

    #[test]
    fn parse_accepts_each_target_all_and_dedups() -> Result<(), String> {
        let mut agents = Vec::new();
        parse_agents("cursor", &mut agents)?;
        parse_agents("all", &mut agents)?;
        assert_eq!(
            agents,
            vec![InitAgent::Cursor, InitAgent::Claude, InitAgent::Codex]
        );
        let err = parse_agents("copilot", &mut agents)
            .err()
            .ok_or("unknown target must be refused")?;
        assert!(err.contains("copilot") && err.contains("cursor"), "{err}");
        Ok(())
    }
}
