//! `ripr init --agents`: the user-facing agent instruction block.
//!
//! The block is a short find-gap, write-test, verify loop with an explicit
//! done rule and stop rule. It is written into the instruction file each agent
//! already reads, inside a marked section, so a rerun is idempotent and the
//! user's own content outside the markers is never touched.

use super::init::{InitAction, InitTarget, path_is_occupied};
use crate::cli::commands_options::InitAgent;
use crate::cli::suggest::unknown_value;
use crate::output;
use std::path::{Path, PathBuf};

/// Project MCP file Cursor reads; same `mcpServers` shape as `.mcp.json`.
pub(super) const CURSOR_MCP_FILE: &str = ".cursor/mcp.json";

const CURSOR_RULE_FILE: &str = ".cursor/rules/ripr.mdc";

const BEGIN_PREFIX: &str = "<!-- ripr:agents:begin";
const BEGIN_LINE: &str = "<!-- ripr:agents:begin (written by `ripr init --agents`; `--force` refreshes this section, keep your own notes outside the markers) -->";
const END_LINE: &str = "<!-- ripr:agents:end -->";

const CURSOR_FRONTMATTER: &str = "---\ndescription: ripr static test-gap finder; run it after changing Rust behavior\nalwaysApply: false\nglobs: [\"**/*.rs\"]\n---\n";

pub(super) const FOLLOW_UP: &str = "Commit the agent instruction file(s) so every session sees them. Register the MCP server only if your agent prefers tools over the CLI: see docs/AGENT_QUICKSTART.md.";

/// The instruction block. `docs/AGENT_QUICKSTART.md` carries the identical
/// text; a unit test keeps the two equal.
pub(super) const AGENT_BLOCK: &str = r#"## ripr: static test-gap finder (Rust). Run it after you change behavior.
ripr reads your diff and says which changed lines no test would notice. It never runs
tests or mutants; "exposed" is static evidence only. It does not edit files.

Before you start: commit the source change; `/target/` must be in .gitignore.
Only edit tests. Never change production code to make a gap go away.

Loop (max 3 rounds per gap):
1. FIND    `ripr check --quiet`   (add `--base <branch>` if there is no origin/main|master)
           Read "Start here": File, Missing check, Related test, Next step.
           Exit code is 0 even with gaps. 2 = could not run; 3 = ripr declined (read stderr).
2. DETAIL  Run the `ripr explain ...` / `ripr context ...` lines it printed, exactly as printed
           (ids belong to that run). Optional: `ripr agent stub --at FILE:LINE --kind KIND [--write]`
           prints a compiling skeleton; it fails on purpose (`todo!`) until you fill it in.
3. WRITE   One focused test next to the existing tests (the file "Related test" names).
           Use assert_eq! on the exact changed value(s): every changed field, branch and
           boundary (below, equal, above). Not `.is_some()`, `.is_empty()`, `<=`, or only
           whole-struct equality.
4. VERIFY  `cargo test <your_test_name>` must pass. Then `ripr check --quiet` again
           (a dirty tree is analyzed automatically; new files need `git add -N PATH`).
5. DONE    when `ripr check --quiet --json | jq '[.findings[]|select(.classification!="exposed")]|length'`
           prints 0 (human form: "State: no gap selected for repair").
           Report what you added; ripr's pass is static, not proof that tests catch real bugs.

STOP and report instead of looping when:
- the top finding says "Analyzer limit" / "Tests cannot close this static-analysis limitation";
- the same gap remains after 3 exact-assert tests (it may be an equivalent change, e.g. `>` vs
  `>=` where both branches return the same value); or `ripr check` exits 2 or 3.

Optional guided repair (Rust seams ripr can bind to one test file):
  `ripr repair <seam-id-or-probe-id> --root .` -> edit only the named test file -> `ripr continue`
  If it refuses (exit 2 or 3), fall back to steps 3-5. `ripr status --root .` resumes an attempt.

Do not read target/ripr/pilot/*.json (2-4 MB) or docs/OUTPUT_SCHEMA.md (1 MB).
MCP is optional and read-only; the CLI above is cheaper. Setup: ripr init --mcp
"#;

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

fn marked_section() -> String {
    format!("{BEGIN_LINE}\n{AGENT_BLOCK}{END_LINE}\n")
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
) -> Result<InitTarget, String> {
    let shown = output::path::human_path(&path);
    let section = marked_section();
    if !path_is_occupied(&path)? {
        let mut body = String::new();
        if agent == InitAgent::Cursor {
            body.push_str(CURSOR_FRONTMATTER);
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
            if text[start..end] == section || !force {
                return Ok(InitTarget {
                    path,
                    action: InitAction::LeaveUnchanged,
                    body: text,
                });
            }
            let body = format!("{}{}{}", &text[..start], section, &text[end..]);
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
        plan_agent_target(agent, path.to_path_buf(), force)
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
            format!("before\n{}after\n", marked_section()),
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
        assert!(planned.body.ends_with(&marked_section()));
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
