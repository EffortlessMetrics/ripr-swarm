//! RIPR-SPEC-0258: a committed snapshot entry pins the value.
//!
//! `expect(x).toMatchSnapshot()` compares the serialized value with the entry
//! stored under `<test title> <counter>` in `__snapshots__/<test file>.snap`;
//! Jest's `toMatchSnapshot("hint")` stores it under `<test title>: <hint>
//! <counter>`, counting per hinted name.
//! When that entry is committed, the runner fails on any changed
//! serialization (under `--ci` a missing entry fails too), so the assertion
//! pins the value the way a literal inline snapshot does (RIPR-SPEC-0247
//! rule 3). The reading is promoted only when the entry this assertion uses
//! can be named without running the suite; every other shape keeps
//! `snapshot` / medium.

use super::*;

/// Text after skipping whitespace, `/* */` comments and `//` line comments.
fn skip_trivia(mut text: &str) -> &str {
    loop {
        text = text.trim_start();
        if let Some(inner) = text.strip_prefix("//") {
            // A line comment ends at any JS line terminator: LF, CR, U+2028
            // or U+2029 (#8895); a CRLF pair ends at its CR and the LF is
            // whitespace.
            text = inner
                .split_once(['\n', '\r', '\u{2028}', '\u{2029}'])
                .map_or("", |(_, after)| after);
        } else if let Some((_, after)) = text
            .strip_prefix("/*")
            .and_then(|inner| inner.split_once("*/"))
        {
            text = after;
        } else {
            return text;
        }
    }
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$')
}

/// `true` when the identifier ending at `index` (just before `Snapshot`) is
/// matcher-shaped: it starts with `toMatch` or `toThrowErrorMatching`, as
/// `toMatchSnapshot`, `toMatchInlineSnapshot`, `toThrowErrorMatchingSnapshot`
/// and `toThrowErrorMatchingInlineSnapshot` do.
fn matcher_shaped_prefix(text: &str, index: usize) -> bool {
    text.get(..index).is_some_and(|before| {
        let word = &before[before.trim_end_matches(is_identifier_char).len()..];
        word.starts_with("toMatch") || word.starts_with("toThrowErrorMatching")
    })
}

/// Snapshot matcher calls in `text`: `Snapshot` followed, after optional
/// whitespace, block comments and line comments, by `(`, `?.`, `!`, a
/// type-argument list (`<`), a bracket-access close (`"]`, `']`, `` `] ``,
/// with optional whitespace or comments before the `]`) or by `.call` /
/// `.apply` (#8031). Each such call advances the runner's per-title counter,
/// so it is counted whether or not it writes to the committed file. A
/// matcher-shaped name (`toMatch*Snapshot`, `toThrowErrorMatching*Snapshot`)
/// used as a value instead of called in place, and `.bind`, which stores the
/// callable for any number of later calls, counts as two: the parenthesised
/// `(a.toMatchSnapshot)()`, a cast `as` / `satisfies`, an alias
/// `const f = a.toMatchSnapshot`, an argument, a destructuring, an
/// `import { toMatchSnapshot }` and a comment or string that names the
/// matcher are never credited as the one call and refuse (#8895). The count
/// doubles as the refusal marker (refuse-on-presence, RIPR-SPEC-0258 rule 1),
/// not a call tracker. A word that merely begins with `Snapshot`, a standalone
/// `Snapshot` and other `...Snapshot` names (`getSnapshot`, `type
/// FooSnapshot`) count zero unless called.
pub(super) fn snapshot_calls(text: &str) -> usize {
    text.match_indices("Snapshot")
        .map(|(index, matched)| {
            let Some(raw) = text.get(index + matched.len()..) else {
                return 0;
            };
            if raw.starts_with(is_identifier_char) {
                return 0;
            }
            let rest = skip_trivia(raw);
            if rest.starts_with(".bind") {
                return 2;
            }
            let called = rest.starts_with(['(', '<', '!'])
                || [".call", ".apply", "?."]
                    .iter()
                    .any(|tail| rest.starts_with(tail))
                || rest
                    .strip_prefix(['"', '\'', '`'])
                    .is_some_and(|after| skip_trivia(after).starts_with(']'));
            if called {
                1
            } else if matcher_shaped_prefix(text, index) {
                2
            } else {
                0
            }
        })
        .sum()
}

/// The hint of the body's one `toMatchSnapshot("<hint>")` call, when it is a
/// single quoted string literal whose source text is its value and that the
/// snapshot file writes unescaped: no `` ` ``, `$`, `\`, line break or
/// matching quote inside, and not empty (Jest drops an empty hint). Any
/// other argument (a template, an identifier, a property matcher, a comment,
/// a second argument) is `None`, and so is a body that names
/// `toMatchSnapshot` more than once, even in a string or comment, since the
/// hint read must come from the call the assertion credits.
fn literal_hint(body: &str) -> Option<&str> {
    let mut mentions = body.match_indices("toMatchSnapshot");
    let (index, matched) = mentions.next()?;
    if mentions.next().is_some() {
        return None;
    }
    let rest = body
        .get(index + matched.len()..)?
        .trim_start()
        .strip_prefix('(')?
        .trim_start();
    let quote = rest
        .chars()
        .next()
        .filter(|quote| matches!(quote, '"' | '\''))?;
    let rest = rest.get(1..)?;
    let end = rest.find(quote)?;
    let hint = rest.get(..end)?;
    let closed = rest.get(end + 1..)?.trim_start().starts_with(')');
    (closed && !hint.is_empty() && !hint.contains(['`', '\\', '$', '\n', '\r'])).then_some(hint)
}

/// Promote each test's `toMatchSnapshot()` assertion, or under Jest its
/// `toMatchSnapshot("<hint>")` assertion, to `snapshot` / strong when the
/// committed entry it compares with exists (rule 1).
pub(crate) fn promote_committed_snapshots(
    root: &Path,
    test_file: &Path,
    file_source: &str,
    tests: &mut [TypeScriptTest],
) {
    // A snapshot call outside every test body (a hook or a helper) advances
    // a test's counter where this reader cannot see it. Concurrent tests may
    // record a snapshot under another test's title.
    let in_bodies: usize = tests
        .iter()
        .map(|test| snapshot_calls(&test.body_text))
        .sum();
    if in_bodies == 0
        || snapshot_calls(file_source) != in_bodies
        || file_source.contains(".concurrent")
    {
        return;
    }
    let Some(snapshots) = committed_snapshot_file(root, test_file) else {
        return;
    };
    let titles: Vec<String> = tests
        .iter()
        .map(|test| joined_title(test, &snapshots.separator))
        .collect();
    let keys: Vec<Option<(String, usize)>> = tests
        .iter()
        .map(|test| snapshot_key(test, &snapshots))
        .collect();
    for (test, key) in tests.iter_mut().zip(&keys) {
        let Some((key, arguments)) = key else {
            continue;
        };
        // Tests whose titles join to the same key share its counter, and so
        // does a test titled `<title>` whose call carries a hint: Jest keys
        // it `<title>: <hint>` and Vitest `<title> > <hint>`, and that hint
        // may make this key.
        let hint_join = match snapshots.runner {
            SnapshotRunner::Jest => ": ",
            SnapshotRunner::Vitest => " > ",
        };
        let sharing = titles
            .iter()
            .filter(|title| {
                *title == key
                    || key
                        .strip_prefix(title.as_str())
                        .is_some_and(|rest| rest.starts_with(hint_join))
            })
            .count();
        if sharing != 1 {
            continue;
        }
        if !snapshots.pins_only_entry(key) {
            continue;
        }
        // The body holds one snapshot call, so at most one assertion matches.
        if let Some(assertion) = test.assertions.iter_mut().find(|assertion| {
            assertion.matcher == "toMatchSnapshot"
                && assertion.argument_count == *arguments
                && assertion.oracle_kind == OracleKind::Snapshot
                && assertion.oracle_strength == OracleStrength::Medium
        }) {
            assertion.oracle_strength = OracleStrength::Strong;
        }
    }
}

/// The committed snapshot file for `test_file` at the runner's default
/// location, with the title separator its header names.
struct CommittedSnapshots {
    text: String,
    separator: String,
    runner: SnapshotRunner,
}

impl CommittedSnapshots {
    /// `true` when the file stores `<key> 1` exactly once and no `<key> 2`:
    /// the assertion ran once when the file was written, and its one entry
    /// is the one it compares with. The runner writes each entry at the start
    /// of a line, so text elsewhere (a comment) is not an entry.
    fn pins_only_entry(&self, key: &str) -> bool {
        let entry = |counter: usize| format!("\nexports[`{key} {counter}`] = ");
        self.text.matches(&entry(1)).count() == 1 && !self.text.contains(&entry(2))
    }
}

fn committed_snapshot_file(root: &Path, test_file: &Path) -> Option<CommittedSnapshots> {
    let directory = test_file.parent()?;
    let name = test_file.file_name()?.to_str()?;
    let snapshots = directory.join("__snapshots__");
    // A committed-history run reads the test file at `HEAD`; a `.snap` file
    // edited, deleted, untracked or ignored there is not the entry `HEAD`
    // holds.
    if crate::analysis::committed_source::snapshot_inputs_differ_in(root, &snapshots) {
        return None;
    }
    // `read_config_capped` refuses a symlinked `.snap` leaf only; a linked
    // `__snapshots__` directory could point at a tree no review judged.
    if std::fs::symlink_metadata(root.join(&snapshots))
        .ok()?
        .file_type()
        .is_symlink()
    {
        return None;
    }
    // A `--worktree` run installs no overlay, so a `.snap` file that is
    // untracked or ignored would credit an entry CI never sees (#7708).
    // When the repository can list its tracked snapshot files, the file
    // must be one of them.
    if !snapshot_file_is_tracked(root, &snapshots.join(format!("{name}.snap"))) {
        return None;
    }
    let path = root.join(&snapshots).join(format!("{name}.snap"));
    let text = read_config_capped(&path).ok()?;
    let header = text.lines().next()?;
    let (separator, runner) = if header.starts_with("// Jest Snapshot v1") {
        (" ", SnapshotRunner::Jest)
    } else if header.starts_with("// Vitest Snapshot v1") {
        (" > ", SnapshotRunner::Vitest)
    } else {
        return None;
    };
    let configs = runner_configs(root, directory)?;
    // The header names the runner that wrote the file, not the one that runs
    // it: Vitest reads a file Jest left but keys titles with ` > `, and the
    // reverse holds for Jest. Only a header that agrees with the configs
    // names the entry the assertion compares with.
    if (runner == SnapshotRunner::Vitest) != configs.mentions_vitest {
        return None;
    }
    Some(CommittedSnapshots {
        text,
        separator: separator.to_string(),
        runner,
    })
}

/// `false` only when the repository lists its tracked `*.snap` files under
/// `root` (one call per run) and `relative` is not among them. When it cannot
/// answer (an export, no repository, the tool unavailable) the file is read
/// as it always was.
fn snapshot_file_is_tracked(root: &Path, relative: &Path) -> bool {
    let Some(tracked) = run_cached_tracked_snapshots(root, || {
        crate::analysis::committed_source::tracked_snapshot_files(root).map(std::sync::Arc::new)
    }) else {
        return true;
    };
    let key = relative
        .components()
        .filter(|component| !matches!(component, std::path::Component::CurDir))
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    tracked.contains(&key)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SnapshotRunner {
    Jest,
    Vitest,
}

/// What the runner configs above the test file say.
pub(super) struct RunnerConfigs {
    pub(super) mentions_vitest: bool,
}

/// Text that moves snapshot files away from the default location, changes
/// how they are serialized, or makes every run rewrite them, so a committed
/// entry no longer fails a changed value. The short `-u` flag is matched
/// separately so any whitespace may precede it, not only a space.
const SNAPSHOT_OVERRIDES: [&str; 7] = [
    "snapshotResolver",
    "resolveSnapshotPath",
    "snapshotEnvironment",
    "updateSnapshot",
    "--update",
    "\"-u\"",
    "snapshotFormat",
];

/// Assignment operators that rewrite a member (`config.key = x`,
/// `config.key ||= x`, `config.key += x`). `==`, `===` and `=>` are not.
const ASSIGNMENT_OPERATORS: [&str; 16] = [
    "||=", "??=", "&&=", "**=", "<<=", ">>>=", ">>=", "+=", "-=", "*=", "/=", "%=", "&=", "|=",
    "^=", "=",
];

/// `before` without trailing whitespace and `/* */` comments, so a comment
/// between a member dot or an opening bracket and the key does not hide it.
fn trim_trivia_end(mut before: &str) -> &str {
    loop {
        before = before.trim_end();
        match before
            .strip_suffix("*/")
            .and_then(|inner| inner.rsplit_once("/*"))
        {
            Some((rest, _)) => before = rest,
            None => return before,
        }
    }
}

/// `true` when `after` (text following a member key) starts an assignment.
fn starts_assignment(after: &str) -> bool {
    ASSIGNMENT_OPERATORS.iter().any(|operator| {
        after
            .strip_prefix(operator)
            .is_some_and(|rest| *operator != "=" || !rest.starts_with(['=', '>']))
    })
}

/// `true` when `text` holds `key` as an object key (`key:`, `key :`,
/// `"key":`, `'key':`, a computed `["key"]:` / `['key']:` / `` [`key`]: ``, or
/// JS shorthand `{ key }` / `{ key, ... }`) or as a member assignment
/// (`config.key = ...`, `config["key"] = ...`, and the compound forms
/// `||=`, `??=`, `&&=`, `+=`). Whitespace and comments may sit between the
/// tokens (`projects /* x */ :`, `config.key /* x */ = ...`). A longer
/// identifier (`rootDir`, `roots`) does not match `root`, and a plain
/// `const root = ...` declaration is not a key.
fn mentions_named_key(text: &str, key: &str) -> bool {
    text.match_indices(key).any(|(index, matched)| {
        let before = text.get(..index).unwrap_or_default();
        let preceded_by_ident = before
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$'));
        if preceded_by_ident {
            return false;
        }
        let Some(rest) = text.get(index + matched.len()..) else {
            return false;
        };
        let (quoted, after_quote) = match rest.strip_prefix(['"', '\'', '`']) {
            Some(after) => (true, after),
            None => (false, rest),
        };
        let trimmed = skip_trivia(after_quote);
        // A computed key (`["key"]`) or bracket member (`config["key"]`)
        // closes its bracket after the quote.
        let (closed, after) = match trimmed.strip_prefix(']') {
            Some(after) if quoted => (true, skip_trivia(after)),
            _ => (false, trimmed),
        };
        // Whitespace and comments may separate the opening bracket from the
        // quote (`config[ "key" ]`) and a member dot from its key.
        let bracket_open = before
            .strip_suffix(['"', '\'', '`'])
            .is_some_and(|opener| trim_trivia_end(opener).ends_with('['));
        let member = trim_trivia_end(before).ends_with('.') || (closed && bracket_open);
        after.starts_with(':')
            || (!closed && after.starts_with([',', '}']))
            || (member && starts_assignment(after))
    })
}

/// `true` when `text` holds an `update` key (`update: true`, `update : x`,
/// `"update": x`), Vitest's option to rewrite every changed snapshot.
fn mentions_update_key(text: &str) -> bool {
    mentions_named_key(text, "update")
}

/// `true` when a runner config names another config this walk does not read:
/// Jest `projects`, Vitest `workspace`, or a Vitest/Vite `root` key. Jest's
/// `rootDir` and a `<rootDir>` mapper token do not match `root`.
fn delegates_to_other_config(text: &str) -> bool {
    let text = blank_value_strings(text);
    ["projects", "workspace", "root"]
        .into_iter()
        .any(|key| mentions_named_key(&text, key))
}

/// `text` with the contents of every quoted string that is a value (followed
/// by `,` or `}`) read as spaces, so a label such as `displayName: "projects"` is not
/// taken for a key (#10295). A string that is a key, a computed key, or a
/// bracket member keeps its text. Unchanged when the scan cannot be sure of
/// string boundaries (unterminated string or comment, or a template literal or a `/`,
/// which may open a regex whose quote could desynchronise it), so the old refusal stands.
fn blank_value_strings(text: &str) -> String {
    let Some(clean) = blank_comments(text) else {
        return text.to_string();
    };
    let mut out = String::with_capacity(clean.len());
    let mut chars = clean.char_indices();
    while let Some((open, ch)) = chars.next() {
        match ch {
            '"' | '\'' => {
                let mut close = None;
                while let Some((at, inner)) = chars.next() {
                    if inner == '\\' {
                        if chars.next().is_none() {
                            return text.to_string();
                        }
                    } else if inner == ch {
                        close = Some(at);
                        break;
                    }
                }
                let Some(close) = close else {
                    return text.to_string();
                };
                let after = clean.get(close + 1..).unwrap_or_default();
                out.push(ch);
                let body = clean.get(open + 1..close).unwrap_or_default();
                if skip_trivia(after).starts_with([',', '}']) {
                    out.extend(body.chars().flat_map(|c| {
                        let fill = if c.is_ascii() && is_line_terminator(c) {
                            c
                        } else {
                            ' '
                        };
                        std::iter::repeat_n(fill, c.len_utf8())
                    }));
                } else {
                    out.push_str(body);
                }
                out.push(ch);
            }
            // A template can hold quotes this scan would misread as a string.
            // So can a regex literal, and after comments are blanked a `/`
            // is a regex or a division this scan cannot tell apart.
            '`' | '/' => return text.to_string(),
            c => out.push(c),
        }
    }
    // Callers slice the result with offsets taken from `clean`.
    if out.len() == clean.len() {
        out
    } else {
        text.to_string()
    }
}

/// `true` when a `preset` value in `text` is not a plain package name: a
/// path (`./p.js`, `/abs/preset`), a file name, or a non-literal value can
/// carry a `rootDir`, `roots` or `snapshotResolver` this walk never reads.
fn preset_may_name_a_file(text: &str) -> bool {
    text.match_indices("preset").any(|(index, matched)| {
        let before = text.get(..index).unwrap_or_default();
        if before.chars().next_back().is_some_and(is_identifier_char) {
            return false;
        }
        let rest = text.get(index + matched.len()..).unwrap_or_default();
        let rest = rest.strip_prefix(['"', '\'', '`']).unwrap_or(rest);
        let after = skip_trivia(rest);
        // A computed key (`['preset']:`) or shorthand (`{ preset }`) names a
        // preset this scan cannot read.
        if after.starts_with([']', ',', '}']) {
            return true;
        }
        // `preset` not followed by `:` is a value or a word, not a key.
        let Some(value) = after.strip_prefix(':').map(skip_trivia) else {
            return false;
        };
        let Some(name) = value.strip_prefix(['"', '\'']) else {
            // A non-literal value (`preset: base`) is not read.
            return true;
        };
        let (name, after_name) = name.split_once(['"', '\'']).unwrap_or((name, "\""));
        // `"ts-jest" && "./custom.js"` evaluates to the second string; only a
        // literal that ends the property is the name Jest sees.
        let ends_property = {
            let after = skip_trivia(after_name);
            after.is_empty() || after.starts_with([',', '}'])
        };
        !(ends_property && is_known_non_relocating_preset(name))
    })
}

/// `true` for the `ts-jest` presets, which set `transform` and `testMatch`
/// and never `rootDir`, `roots` or `snapshotResolver`. Any other preset is a
/// package or file Jest merges into the project, and this walk never reads
/// it, so it can relocate snapshots (#10139).
fn is_known_non_relocating_preset(name: &str) -> bool {
    !name.contains("..")
        && (name == "ts-jest"
            || name
                .strip_prefix("ts-jest/presets/")
                .is_some_and(|rest| !rest.is_empty() && !rest.contains(['/', '.'])))
}

/// JS line terminators: LF, CR, U+2028 and U+2029 (#8895).
fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `text` with every comment replaced by spaces, newlines kept, so offsets
/// into the result are offsets into `text`. `None` for an unterminated
/// string or block comment.
fn blank_comments(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' | '\'' | '`' => {
                out.push(ch);
                loop {
                    let inner = chars.next()?;
                    out.push(inner);
                    if inner == '\\' {
                        out.push(chars.next()?);
                    } else if inner == ch {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                out.push(' ');
                while let Some(next) = chars.peek().copied() {
                    if is_line_terminator(next) {
                        break;
                    }
                    out.push(' ');
                    chars.next();
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                out.push(' ');
                chars.next();
                out.push(' ');
                let mut closed = false;
                while let Some(next) = chars.next() {
                    if next == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        out.push_str("  ");
                        closed = true;
                        break;
                    }
                    out.push(if is_line_terminator(next) { next } else { ' ' });
                }
                if !closed {
                    return None;
                }
            }
            _ => out.push(ch),
        }
    }
    Some(out)
}

/// `text` with its one `projects: [ { ... }, { ... } ]` array removed, when
/// every element is an inline object literal that moves nothing: no string,
/// identifier, spread or call at the array's top level (those name other
/// configs), no template placeholder, and no `rootDir`, `roots`, `root`,
/// `workspace` or `projects` key inside a project. `None` for any other
/// spelling (a path or glob entry, a computed key, a member assignment, a
/// second `projects`), so those keep delegating. A project's
/// `snapshotResolver` is caught by `SNAPSHOT_OVERRIDES` before this runs.
fn without_plain_inline_projects(raw: &str) -> Option<String> {
    let blanked = blank_comments(raw)?;
    let text = blanked.as_str();
    let valued = blank_value_strings(&blanked);
    let mut found = valued.match_indices("projects");
    let (index, matched) = found.next()?;
    if found.next().is_some() {
        return None;
    }
    let preceded_by_ident = text
        .get(..index)?
        .chars()
        .next_back()
        .is_some_and(is_identifier_char);
    if preceded_by_ident {
        return None;
    }
    // `"projects":` and `'projects':` are the same key as `projects:`.
    let after_name = text.get(index + matched.len()..)?;
    let after_name = after_name.strip_prefix(['"', '\'']).unwrap_or(after_name);
    let after_key = skip_trivia(after_name);
    let after_colon = skip_trivia(after_key.strip_prefix(':')?);
    let array = after_colon.strip_prefix('[')?;
    let mut depth = 1usize;
    let mut end = None;
    let mut chars = array.char_indices().peekable();
    let mut last_significant = '[';
    while let Some((at, ch)) = chars.next() {
        let before_this = last_significant;
        if !ch.is_whitespace() {
            last_significant = ch;
        }
        match ch {
            // A regex literal in value position (`tag: /a(b)/`) holds
            // parentheses and brackets that are not calls or nesting.
            '/' if depth >= 2 && before_this == ':' => {
                let mut in_class = false;
                loop {
                    let (_, inner) = chars.next()?;
                    match inner {
                        '\\' => {
                            chars.next()?;
                        }
                        c if is_line_terminator(c) => return None,
                        '[' => in_class = true,
                        ']' => in_class = false,
                        '/' if !in_class => break,
                        _ => {}
                    }
                }
            }
            '"' | '\'' | '`' => {
                // A string at the array's top level names another config.
                if depth == 1 {
                    return None;
                }
                loop {
                    let (_, inner) = chars.next()?;
                    if inner == '\\' {
                        chars.next()?;
                    } else if inner == '$' && ch == '`' && chars.peek().is_some_and(|c| c.1 == '{')
                    {
                        return None;
                    } else if inner == ch {
                        break;
                    }
                }
            }
            '/' if chars.peek().is_some_and(|c| matches!(c.1, '/' | '*')) => {
                let rest = skip_trivia(array.get(at..)?);
                let skipped = array.len() - rest.len();
                while chars.peek().is_some_and(|c| c.0 < skipped) {
                    chars.next();
                }
            }
            // A call can build a project at run time, and a nested array at
            // the top level is not an inline object.
            '(' => return None,
            '[' if depth == 1 => return None,
            '{' | '[' => depth += 1,
            '}' | ']' | ')' => {
                depth -= 1;
                if depth == 0 {
                    if ch != ']' {
                        return None;
                    }
                    end = Some(at);
                    break;
                }
            }
            c if depth == 1 && !(c.is_whitespace() || c == ',') => return None,
            _ => {}
        }
    }
    let end = end?;
    let inside = array.get(..end)?;
    let array_start = text.len() - array.len();
    let inside_valued = valued.get(array_start..array_start + end)?;
    if delegates_to_other_config(inside_valued)
        || mentions_named_key(inside_valued, "rootDir")
        || mentions_named_key(inside_valued, "roots")
        || inside.contains("...")
        || preset_may_name_a_file(inside)
    {
        return None;
    }
    // A method call or index on the array (`.concat(["./other"])`) can add
    // entries this scan never saw; only a list separator or the end of the
    // object may follow the closing bracket.
    let after_array = skip_trivia(array.get(end + 1..)?);
    if !(after_array.is_empty() || after_array.starts_with([',', '}', ';'])) {
        return None;
    }
    let tail_start = text.len() - after_colon.len() + 1 + end + 1;
    let head = text.get(..index)?;
    let head = head.strip_suffix(['"', '\'']).unwrap_or(head);
    Some(format!("{head}{}", text.get(tail_start..)?))
}

/// `true` when a Jest or Vitest config names another config this walk does
/// not read. A Jest config whose only such key is a plain inline `projects`
/// array (#10139) does not: the walk reads that array itself.
fn delegates_beyond_plain_projects(file_name: &str, text: &str) -> bool {
    if let Some(rest) = file_name
        .starts_with("jest")
        .then(|| without_plain_inline_projects(text))
        .flatten()
    {
        return delegates_to_other_config(&rest);
    }
    delegates_to_other_config(text)
}

/// JSON string escapes for the whitespace and quotes a `package.json`
/// script may put around a flag (`"jest\t-u"`, `"jest \"-u\""`).
const JSON_ESCAPES: [&str; 11] = [
    "\\t", "\\n", "\\r", "\\f", "\\v", "\\u0020", "\\u0009", "\\u000a", "\\u000d", "\\\"", "\\'",
];

/// `text` with each JSON escape for whitespace or a quote read as a space,
/// so a script's flags split the way the shell sees them.
fn unescape_json_separators(text: &str) -> String {
    JSON_ESCAPES
        .iter()
        .fold(text.to_string(), |acc, escape| acc.replace(escape, " "))
}

/// `true` when `text` holds the short update flag, alone or inside a
/// combined cluster, after whitespace, a shell quote or a JSON escape of
/// either: `jest -u`, `jest\t-u`, `jest '-u'`, `jest \"-u\"`, `jest -iu`.
/// A single dash only: `--ci` and `eslint-plugin-unicorn` do not match.
/// Over-matching a cluster that merely contains `u` only refuses.
fn mentions_short_update_flag(text: &str) -> bool {
    let text = unescape_json_separators(text);
    text.match_indices('-').any(|(index, _)| {
        let before = text.get(..index).unwrap_or_default();
        let boundary = before
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '"' | '\'' | '`'));
        let cluster = text
            .get(index + 1..)
            .unwrap_or_default()
            .chars()
            .take_while(char::is_ascii_alphabetic)
            .collect::<String>();
        boundary && cluster.contains('u')
    })
}

/// Flags that point a runner at a config, project list or root the
/// directory walk does not read.
const CONFIG_FLAGS: [&str; 7] = [
    "--config",
    "--projects",
    "--rootDir",
    "--root",
    "--dir",
    "--workspace",
    "-r",
];

/// `true` when a `package.json` passes a runner config, project list or root
/// by path: a script flag (`--config x`, `--config=x`, `-c x`, `-cx`,
/// `--rootDir`, Vitest `--root` / `-r` / `--dir` / `--workspace`) or a
/// `"projects"` / `"rootDir"` key under the `jest` key. Either way the
/// config the runner uses may be one the directory walk does not read.
/// Over-matching (a `node -r` preload) only refuses.
fn passes_config_by_path(text: &str) -> bool {
    text.contains("\"projects\"")
        || text.contains("\"rootDir\"")
        || unescape_json_separators(text)
            .split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | '[' | ']'))
            .any(|token| {
                let flag = token.split('=').next().unwrap_or(token);
                CONFIG_FLAGS.contains(&flag)
                    || token.starts_with("--config")
                    || (token.starts_with("-c") && !token.starts_with("--"))
            })
}

/// Read every runner config and `package.json` directly in `directory` into
/// `configs`, or `None` when one mentions a snapshot override, passes a
/// config by path, delegates to another config this walk does not read,
/// cannot be read, or sits in a symlinked directory.
/// `allow_symlink` is `true` only for the analysis root itself, whose path is
/// the user's own checkout path and may be a link.
/// The verdict is memoized per analysis run (#7708): every test file under
/// the same ancestors shares one read of each directory.
fn scan_config_directory(
    directory: &Path,
    allow_symlink: bool,
    configs: &mut RunnerConfigs,
) -> Option<()> {
    let mentions_vitest = run_cached_config_directory(directory, allow_symlink, || {
        scan_config_directory_uncached(directory, allow_symlink)
    })?;
    configs.mentions_vitest |= mentions_vitest;
    Some(())
}

/// One uncached read of `directory`'s configs: `Some(mentions_vitest)` when
/// every one is clean, `None` when any refuses.
fn scan_config_directory_uncached(directory: &Path, allow_symlink: bool) -> Option<bool> {
    // `read_dir` follows a symlinked directory, so a link could point at a
    // tree this reader never judged; `read_config_capped` already refuses a
    // symlinked file.
    if !allow_symlink
        && std::fs::symlink_metadata(directory)
            .ok()?
            .file_type()
            .is_symlink()
    {
        return None;
    }
    let mut mentions_vitest = false;
    for entry in std::fs::read_dir(directory).ok()? {
        let entry = entry.ok()?;
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let is_config = !file_name.ends_with(".snap")
            && crate::analysis::language::snapshot_input(Path::new(file_name));
        if !is_config {
            continue;
        }
        let text = read_config_capped(&entry.path()).ok()?;
        if SNAPSHOT_OVERRIDES
            .iter()
            .any(|override_text| text.contains(override_text))
            || mentions_short_update_flag(&text)
            // Vitest reads `update` from its own config only; a
            // `package.json` script may be named `update`.
            || (file_name != "package.json" && mentions_update_key(&text))
            || (file_name == "package.json" && passes_config_by_path(&text))
            // A Jest/Vitest config that names other configs is not walked.
            || (file_name != "package.json" && delegates_beyond_plain_projects(file_name, &text))
        {
            return None;
        }
        mentions_vitest |= file_name.starts_with("vite") || text.contains("vitest");
    }
    Some(mentions_vitest)
}

/// How many directories above `root` the walk reads while it looks for the
/// repository boundary.
const ABOVE_ROOT_LIMIT: usize = 16;

/// The directories above `root` to read, or `None` when the repository
/// boundary cannot be established.
///
/// The root is canonicalized first so a symlinked analysis root climbs its
/// real parents. The walk stops at (and includes) the first directory holding
/// a repository marker, looking at most `limit` levels up. Empty when `root`
/// is itself the repository top. When no marker is found and the walk reaches
/// the filesystem root within `limit`, every ancestor is returned: nothing
/// above is a repository, and the caller still reads any config there. When
/// the limit is hit before a marker or the filesystem root, the boundary is
/// unknown (a deep nest, or an export without `.git` inside a larger tree), so
/// `None` makes the caller refuse rather than guess.
pub(super) fn directories_above_root(root: &Path, limit: usize) -> Option<Vec<PathBuf>> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if is_repository_boundary(&root) {
        return Some(Vec::new());
    }
    let all: Vec<&Path> = root.ancestors().skip(1).collect();
    let above: Vec<&Path> = all.iter().copied().take(limit).collect();
    match above.iter().position(|dir| is_repository_boundary(dir)) {
        Some(boundary) => Some(
            above
                .into_iter()
                .take(boundary + 1)
                .map(Path::to_path_buf)
                .collect(),
        ),
        None if all.len() <= limit => Some(above.into_iter().map(Path::to_path_buf).collect()),
        None => None,
    }
}

/// `true` when `directory` holds a `.git` entry that ends the repository
/// walk. A submodule's `.git` file (`gitdir: ../.git/modules/x`) does not: the
/// superproject above it owns the runner configs that reach the submodule's
/// tests, so the walk continues to the superproject's own marker. A worktree
/// pointer (`gitdir: .../worktrees/x`), a `.git` directory and an unreadable
/// entry are boundaries; a `.git` directory nested in a larger repository
/// (a vendored clone) cannot be told from the top and stays one (RIPR-SPEC-0258
/// known limits).
fn is_repository_boundary(directory: &Path) -> bool {
    let marker = directory.join(".git");
    if !marker.exists() {
        return false;
    }
    !is_submodule_pointer(&marker)
}

/// `true` when `marker` is a regular `.git` file whose `gitdir:` target is a
/// submodule's module directory (a `modules` path component).
fn is_submodule_pointer(marker: &Path) -> bool {
    const POINTER_CAP: u64 = 4096;
    let Ok(metadata) = std::fs::symlink_metadata(marker) else {
        return false;
    };
    if !metadata.file_type().is_file() || metadata.len() > POINTER_CAP {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(marker) else {
        return false;
    };
    let Some(target) = text.trim().strip_prefix("gitdir:") else {
        return false;
    };
    let target = target.trim();
    let mut components = target.split(['/', '\\']).filter(|part| !part.is_empty());
    // A `worktrees` component is not evidence of a worktree here: Git names a
    // submodule's directory after its path, so `.git/modules/worktrees` is a
    // legitimate submodule. A target with a `modules` component always reads as
    // a submodule, which keeps the walk going to the superproject; the
    // ambiguous worktree-of-a-submodule spelling errs the same way, since
    // reading more configs can only withhold credit.
    components.any(|part| part == "modules")
}

/// The runner configs and `package.json` files in the test's directory and
/// every ancestor up to `root`, then above `root` up to the repository
/// boundary (the first directory holding `.git`, or the filesystem root), or
/// `None` when one mentions a snapshot override, passes a config by path,
/// delegates to another config this walk does not read, cannot be read, (in
/// a committed-history run) differs from `HEAD`, or the boundary is not
/// found within reach.
fn runner_configs(root: &Path, directory: &Path) -> Option<RunnerConfigs> {
    runner_configs_within(root, directory, ABOVE_ROOT_LIMIT)
}

/// [`runner_configs`] with an explicit look-up limit above the root.
pub(super) fn runner_configs_within(
    root: &Path,
    directory: &Path,
    above_root_limit: usize,
) -> Option<RunnerConfigs> {
    let mut configs = RunnerConfigs {
        mentions_vitest: false,
    };
    let mut current = Some(directory);
    while let Some(relative) = current {
        if crate::analysis::committed_source::snapshot_inputs_differ_in(root, relative) {
            return None;
        }
        scan_config_directory(
            &root.join(relative),
            relative.as_os_str().is_empty(),
            &mut configs,
        )?;
        current = relative.parent();
    }
    for dir in directories_above_root(root, above_root_limit)? {
        scan_config_directory(&dir, false, &mut configs)?;
    }
    Some(configs)
}

/// The describe titles and the test title joined the way the runner keys
/// snapshots.
fn joined_title(test: &TypeScriptTest, separator: &str) -> String {
    title_parts(test).join(separator)
}

fn title_parts(test: &TypeScriptTest) -> Vec<&str> {
    test.describe_names
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(test.local_name.as_str()))
        .collect()
}

/// The name the runner keys this test's snapshot under, with the argument
/// count of the call that uses it, or `None` when it cannot be named: more
/// than one snapshot call in the body, or a title with characters the
/// snapshot file escapes. A body whose call carries a literal hint keys
/// `<title>: <hint>` under Jest; Vitest's hinted key is not read. The caller
/// refuses a key another test's title also joins to, since the two share
/// its counter.
fn snapshot_key(test: &TypeScriptTest, snapshots: &CommittedSnapshots) -> Option<(String, usize)> {
    if snapshot_calls(&test.body_text) != 1 {
        return None;
    }
    let parts = title_parts(test);
    if parts
        .iter()
        .any(|part| part.is_empty() || part.contains(['`', '\\', '$', '\n', '\r']))
    {
        return None;
    }
    let title = parts.join(&snapshots.separator);
    match literal_hint(&test.body_text) {
        Some(hint) if snapshots.runner == SnapshotRunner::Jest => {
            Some((format!("{title}: {hint}"), 1))
        }
        Some(_) => None,
        None => Some((title, 0)),
    }
}
