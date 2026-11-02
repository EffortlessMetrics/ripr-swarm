//! Doc examples that rustdoc runs as tests (#6648, RIPR-SPEC-0279).
//!
//! `cargo test` compiles each runnable fenced example in a library's `///`
//! comments as its own crate that depends on the library, and runs it. Code
//! pinned only by such an example is tested, so this pass registers each one
//! as a `TestFact` of the file that holds it. The synthetic body is the
//! example's code with hidden `# ` lines restored, wrapped in a `#[test]`
//! function whose lines match the comment lines, so every fact points at
//! the source the user wrote.
//!
//! The admission is narrow and fails closed:
//!
//! - the file composes, through resolved `mod` edges only, under its
//!   package's default `src/lib.rs` library root, and the manifest leaves
//!   `autolib` on, moves no target, keeps `[lib] doctest` on and builds a
//!   Rust library (`lib`, `rlib`, or no `crate-type`; not `proc-macro`);
//! - the example documents a public production function of that file,
//!   declared on the line that follows the comment (a doc comment on a
//!   statement, or on a private function whose example cannot compile,
//!   is not one);
//! - the comment is the item's only doc fragment (no second `///` run
//!   and no `#[doc]` or doc-bearing attribute on the item, which rustdoc
//!   would join into one document);
//! - the comment holds no `<!--`, no line opening an HTML block, no `~~~`,
//!   no tab-indented line and no fence indented four or more spaces, where
//!   CommonMark may read the backticks as something other than a code
//!   block;
//! - its fence is plain or names only `rust` or an edition (`ignore`,
//!   `no_run`, `compile_fail`, `should_panic`, `text` and every other
//!   attribute refuse);
//! - its code declares no `fn main` (rustdoc then runs the code as written,
//!   not wrapped) and parses cleanly;
//! - no refusing gate sits between the comment and the item, and the file
//!   carries no refusing inner `#![cfg(..)]` gate: a plain `#[cfg(..)]`
//!   admits the example only when its predicate holds for the doctest
//!   build under cargo defaults (`doctest` on between the comment and the
//!   item, `test` off, `feature = "x"` resolved against the owning
//!   manifest's default-enabled features, #12230). Any other `#[cfg(..)]`
//!   (target, custom and unknown predicates, `cfg_attr`, a malformed
//!   predicate) and any `#[cfg_attr(..)]` — including an inner
//!   `#![cfg_attr(.., cfg(..))]` — refuse, as does any gate with no
//!   manifest establishing default features.
//!
//! The file-level `#![cfg(..)]` gate answers a narrower question than the
//! item gate: whether the file is in the library `cargo test` builds, which
//! never sets `doctest` (a `#![cfg(doctest)]` root collects a doctest that
//! cannot link against the normally built rlib). It admits only predicates
//! true with `doctest` off, so a `#![cfg(doctest)]` file still refuses while the item-level
//! `#[cfg(doctest)]` exception (#8639) stands: the doctest build links the
//! normally built library, in which a `doctest`-only item is absent.
//!
//! A doctest is its own crate: call identity reads its scopes from the
//! synthetic text and names the library by its import names, never through
//! `crate::` (`RustIndex::doctest_source`).

use super::cfg_predicates::{
    raw_string_start, rust_ident_name, skip_raw_string, skip_regular_string,
};
use super::{FunctionSourceRole, RustIndex, SourceRoleProvenanceEdgeKind, TestFact};
use crate::analysis::syntax::ra::summarize_file_with_parser;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

/// The synthetic function name the parser sees; the registered test takes
/// rustdoc's display name instead.
pub(crate) const SYNTHETIC_NAME: &str = "ripr_doctest";

/// Fence attributes that leave a block compiled and run as an ordinary
/// test. rustdoc treats any other word as a language or a mode that does not
/// run the assertions as written.
const RUNNABLE_FENCE_WORDS: &[&str] = &[
    "rust",
    "edition2015",
    "edition2018",
    "edition2021",
    "edition2024",
];

/// One runnable fenced example from a `///` comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DocExample {
    /// 1-based line of the opening fence.
    pub(super) fence_line: usize,
    /// The example's code with hidden lines restored, one entry per line
    /// between the fences.
    pub(super) code: Vec<String>,
    /// 1-based line of the item the comment documents.
    pub(super) item_line: usize,
}

impl DocExample {
    /// A source whose `#[test]` function opens on the fence line and closes
    /// on the closing fence line, so parsed fact lines are source lines.
    pub(super) fn synthetic_source(&self) -> String {
        let mut text = "\n".repeat(self.fence_line.saturating_sub(1));
        text.push_str("#[test] fn ");
        text.push_str(SYNTHETIC_NAME);
        text.push_str("() {\n");
        for line in &self.code {
            text.push_str(line);
            text.push('\n');
        }
        text.push_str("}\n");
        text
    }
}

pub(super) fn register_doctests(index: &mut RustIndex) {
    let mut generated: Vec<(TestFact, Arc<str>)> = Vec::new();
    for (path, file) in &index.files {
        // Most files hold no fenced example; check that before the manifest:
        // only a file that can hold an example pays for feature resolution.
        if !file.data.source.contains("```") {
            continue;
        }
        let enabled = index.member_crates.default_enabled_features(path);
        let enabled = enabled.as_ref();
        if file_has_inner_cfg_gate(&file.data.source, enabled) {
            continue;
        }
        let examples = doc_examples(&file.data.source, enabled);
        if examples.is_empty() || !composes_under_doctested_library(index, path) {
            continue;
        }
        for example in examples {
            if !documents_production_function(index, path, example.item_line) {
                continue;
            }
            let source = example.synthetic_source();
            let Ok(facts) = summarize_file_with_parser(path, &source) else {
                continue;
            };
            let Some(mut test) = facts
                .tests
                .into_iter()
                .find(|test| test.name == SYNTHETIC_NAME)
            else {
                continue;
            };
            test.name = display_name(path, &file.data.source, &example);
            test.file = path.clone();
            generated.push((test, Arc::from(source)));
        }
    }
    let mut touched = std::collections::BTreeSet::new();
    for (test, source) in generated {
        let key = (test.file.clone(), test.start_line);
        // Idempotent re-registration (#8547 reuse): skip only while the
        // doctest registered by an earlier pass is still live. A reused
        // index re-runs every post-parse phase, and earlier phases can
        // drop the synthetic test; the recorded source alone must not
        // suppress registering it again, or reuse loses the example.
        let already_live = index.tests().iter().any(|live| {
            live.file == test.file && live.start_line == test.start_line && live.name == test.name
        });
        if already_live {
            continue;
        }
        index
            .doctest_sources
            .insert(key, (test.name.clone(), source));
        let file_path = test.file.clone();
        let id = index.test_facts.allocate(test);
        index.test_order.push(id);
        if let Some(file) = index.files.get_mut(&file_path) {
            file.tests.push(id);
            touched.insert(file_path);
        }
    }
    // One sort per file that gained examples, not one per example.
    for path in touched {
        if let Some(file) = index.files.get_mut(&path) {
            file.tests.sort_by(|&left, &right| {
                let left = &index.test_facts[left];
                let right = &index.test_facts[right];
                left.start_line
                    .cmp(&right.start_line)
                    .then(left.end_line.cmp(&right.end_line))
                    .then(left.name.cmp(&right.name))
            });
        }
    }
}

/// Whether `path` composes, through resolved `mod` edges only, under a
/// package's default library root whose manifest runs doctests.
fn composes_under_doctested_library(index: &RustIndex, path: &Path) -> bool {
    let Some(file) = index.files.get(path) else {
        return false;
    };
    let provenance = &file.data.role_provenance;
    if provenance.earliest_unresolved_reason.is_some()
        || index.include_targets.contains(path)
        || provenance.edges.iter().any(|edge| {
            edge.kind != SourceRoleProvenanceEdgeKind::Module
                || edge.requires_test
                || index.include_targets.contains(&edge.parent)
        })
    {
        return false;
    }
    let root = provenance
        .edges
        .first()
        .map_or(path, |edge| edge.parent.as_path());
    root.ends_with(Path::new("src").join("lib.rs"))
        && index.member_crates.runs_library_doctests(root)
}

/// Whether the item on `item_line` is a public production function of
/// `path`. Doc comments inside `#[cfg(test)]` modules and on tests are not
/// compiled by rustdoc, and an example calling a private function from
/// outside the crate does not compile.
fn documents_production_function(index: &RustIndex, path: &Path, item_line: usize) -> bool {
    let Some(file) = index.files.get(path) else {
        return false;
    };
    file.functions.iter().any(|&id| {
        let function = &index.function_facts[id];
        function.source_role == FunctionSourceRole::Production
            && function.start_line <= item_line
            && item_line <= function.end_line
            && function_line_is(&file.data.source, item_line, &function.name)
    })
}

/// Whether source line `line` is a `pub fn name` item head: only bare `pub`
/// and the function qualifiers may precede `fn`, so a statement that only
/// mentions `fn` (`let x = 1; // fn name`) is not one.
fn function_line_is(source: &str, line: usize, name: &str) -> bool {
    let Some(line) = source.lines().nth(line.saturating_sub(1)) else {
        return false;
    };
    let code = line.trim_start();
    let Some(rest) = code.strip_prefix("pub ") else {
        return false;
    };
    let mut words = rest.split_whitespace();
    let qualified = words.by_ref().take_while(|word| *word != "fn").all(|word| {
        matches!(
            word,
            "const" | "async" | "unsafe" | "extern" | "\"C\"" | "\"Rust\""
        )
    });
    qualified
        && rest.split_whitespace().any(|word| word == "fn")
        && declared_fn_name(rest).is_some_and(|declared| declared == name)
}

/// rustdoc's display name, `<path> - <item> (line <fence>)`, with the bare
/// function name as the item: rustdoc qualifies a function in a submodule
/// (`codec::read_u16`), which this name does not.
fn display_name(path: &Path, source: &str, example: &DocExample) -> String {
    let item = source
        .lines()
        .nth(example.item_line.saturating_sub(1))
        .and_then(declared_fn_name)
        .unwrap_or_default();
    format!(
        "{} - {} (line {})",
        path.to_string_lossy().replace('\\', "/"),
        item,
        example.fence_line
    )
}

/// The name after `fn` on a declaration line (`pub(crate) const fn x(`).
fn declared_fn_name(line: &str) -> Option<&str> {
    let mut words = line.split(|c: char| !(c.is_alphanumeric() || c == '_'));
    words.by_ref().find(|word| *word == "fn")?;
    words.find(|word| !word.is_empty())
}

/// Every runnable fenced example admitted under `enabled`, the owning
/// package's default-enabled features (`None` when no manifest establishes
/// them, in which case any feature gate refuses: the pure-syntax scan
/// cannot tell whether default `cargo test` enables it).
pub(super) fn doc_examples(source: &str, enabled: Option<&BTreeSet<String>>) -> Vec<DocExample> {
    // Most files hold no fence at all; skip the line scan for them.
    if !source.contains("```") {
        return Vec::new();
    }
    let lines: Vec<&str> = source.lines().collect();
    let mut examples = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if outer_doc_text(lines[index]).is_none() {
            index += 1;
            continue;
        }
        let start = index;
        while index < lines.len() && outer_doc_text(lines[index]).is_some() {
            index += 1;
        }
        let item_line = item_after(&lines, index, enabled);
        // rustdoc joins every doc fragment on an item into one document, so
        // a run that shares its item with another `///` run or a `#[doc]`
        // attribute may sit inside a fence opened elsewhere: refuse it.
        if shares_its_document(&lines, start, index) {
            continue;
        }
        examples.extend(
            run_examples(&lines[start..index], start + 1)
                .into_iter()
                .filter_map(|(fence_line, info, code)| {
                    let item_line = item_line?;
                    (runnable_fence(info) && !declares_main(&code)).then_some(DocExample {
                        fence_line,
                        code,
                        item_line,
                    })
                }),
        );
    }
    examples
}

/// The text of an outer doc comment line (`///`, not `////`), with the one
/// space rustdoc strips.
fn outer_doc_text(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix("///")?;
    if rest.starts_with('/') {
        return None;
    }
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

/// The 1-based line of the first line after a doc run that is neither a
/// whole-line attribute, a blank line nor a plain comment: the documented
/// item. `#[inline] pub fn f` on one line is the item line itself, which
/// then reads as no function head and registers nothing.
fn item_after(lines: &[&str], from: usize, enabled: Option<&BTreeSet<String>>) -> Option<usize> {
    for (position, line) in lines.iter().enumerate().skip(from) {
        let trimmed = line.trim_start();
        if between_doc_and_item_cfg_refuses(trimmed, enabled) {
            return None;
        }
        if !is_between_doc_and_item(line) {
            return Some(position + 1);
        }
    }
    None
}

/// A blank line, a plain or doc comment, or an attribute that closes on its
/// line with at most a trailing comment after it. An attribute that does not
/// close there is the item line itself, which refuses.
fn is_between_doc_and_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.is_empty() || trimmed.starts_with("//") || whole_line_attribute(trimmed)
}

/// Whether the file is gated by an inner `#![cfg(..)]` attribute the
/// doctest build may not satisfy. A plain `cfg` satisfied for the library
/// `cargo test` builds (`test` and `doctest` off, default features on)
/// admits the file; anything else — target, custom and unknown predicates,
/// `cfg_attr`, a malformed predicate, or no manifest establishing default
/// features — refuses, as before.
fn file_has_inner_cfg_gate(source: &str, enabled: Option<&BTreeSet<String>>) -> bool {
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        // Only `#![cfg(..)]` gates the file; `#![cfg_attr(docsrs, ..)]` and
        // `#![cfg_attr(not(test), no_std)]` are ordinary crate-root headers.
        // An unparseable `cfg`/`cfg_attr` head fails closed like a gate.
        if trimmed.starts_with("#![") && inner_attribute_may_gate_file(trimmed) {
            if !inner_cfg_gate_admits(trimmed, enabled) {
                return true;
            }
        } else if trimmed.starts_with("#![") && starts_like_cfg_attribute(trimmed) {
            return true;
        }
        if !trimmed.starts_with('#') {
            break;
        }
    }
    false
}

/// Whether a gating inner `#![cfg(..)]` holds for the library build: the
/// same predicate language as the item gate, but `doctest` is off — the
/// library `cargo test` builds never sets it.
fn inner_cfg_gate_admits(trimmed: &str, enabled: Option<&BTreeSet<String>>) -> bool {
    if attribute_cfg_family(trimmed) != AttributeCfgFamily::Cfg {
        return false;
    }
    let Some((_, args)) = attribute_path_and_optional_args(trimmed) else {
        return false;
    };
    let Some(args) = args else {
        return false;
    };
    cfg_satisfied_under_doctest_defaults(args, enabled, false) == Some(true)
}

/// `#![cfg(..)]`, or an inner `#![cfg_attr(.., cfg(..))]` that can add a
/// `cfg` gate. A `cfg_attr` that only sets other attributes
/// (`no_std`, `feature(doc_cfg)`) does not gate the file.
fn inner_attribute_may_gate_file(trimmed: &str) -> bool {
    match attribute_cfg_family(trimmed) {
        AttributeCfgFamily::Cfg => true,
        AttributeCfgFamily::CfgAttr => attribute_path_and_optional_args(trimmed)
            .and_then(|(_, args)| args)
            .is_some_and(|args| {
                let compact: String = args.split_whitespace().collect();
                compact.contains(",cfg(")
            }),
        AttributeCfgFamily::Other => false,
    }
}

/// A `#[cfg(..)]` between the doc comment and the item refuses the example
/// unless its predicate holds for the doctest build under cargo defaults:
/// `doctest` is on (rustdoc sets it when running doctests, #8639), `test`
/// is off (the doctest links the normally built library), and
/// `feature = "x"` follows the owning manifest's default-enabled features
/// (#12230). Any other predicate — target, custom or unknown atoms,
/// `cfg_attr`, a malformed predicate — or no manifest establishing default
/// features refuses, as before.
fn between_doc_and_item_cfg_refuses(trimmed: &str, enabled: Option<&BTreeSet<String>>) -> bool {
    if !whole_line_attribute(trimmed) {
        return false;
    }
    match attribute_cfg_family(trimmed) {
        AttributeCfgFamily::CfgAttr => true,
        AttributeCfgFamily::Cfg => {
            if attribute_is_cfg_doctest(trimmed) {
                return false;
            }
            let Some((_, args)) = attribute_path_and_optional_args(trimmed) else {
                return true;
            };
            let Some(args) = args else {
                return true;
            };
            cfg_satisfied_under_doctest_defaults(args, enabled, true) != Some(true)
        }
        // A `cfg`/`cfg_attr` head the bracket reader could not parse
        // (unbalanced parens, missing argument) is not an ordinary
        // attribute: fail closed and refuse. The source does not compile,
        // so no doctest could run on it.
        AttributeCfgFamily::Other => starts_like_cfg_attribute(trimmed),
    }
}

/// Whether the attribute line opens a `cfg` or `cfg_attr` attribute the
/// bracket reader could not parse, tolerating the inner-attribute `!`.
/// `#[cfgfoo]` and lookalikes do not match: only the `cfg(`/`cfg_attr(`
/// heads and the bare words.
fn starts_like_cfg_attribute(trimmed: &str) -> bool {
    let Some(body) = trimmed.strip_prefix("#[") else {
        return false;
    };
    let body = body.strip_prefix('!').unwrap_or(body).trim_start();
    body.starts_with("cfg(") || body.starts_with("cfg_attr(") || body == "cfg" || body == "cfg_attr"
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttributeCfgFamily {
    Cfg,
    CfgAttr,
    Other,
}

fn attribute_cfg_family(trimmed: &str) -> AttributeCfgFamily {
    let Some((path, _)) = attribute_path_and_optional_args(trimmed) else {
        return AttributeCfgFamily::Other;
    };
    match path {
        "cfg" => AttributeCfgFamily::Cfg,
        "cfg_attr" => AttributeCfgFamily::CfgAttr,
        _ => AttributeCfgFamily::Other,
    }
}

/// True only for `#[cfg(doctest)]`, active when rustdoc runs doctests.
fn attribute_is_cfg_doctest(trimmed: &str) -> bool {
    let Some((path, args)) = attribute_path_and_optional_args(trimmed) else {
        return false;
    };
    path == "cfg"
        && args.is_some_and(|args| {
            let collapsed: String = args.split_whitespace().collect();
            rust_ident_name(&collapsed) == "doctest"
        })
}

/// A `cfg(...)` predicate in the doctest admission position.
#[derive(Debug, PartialEq, Eq)]
enum DoctestCfg {
    /// `test`: off in the doctest build, which links the normally built
    /// library.
    Test,
    /// `doctest`: rustdoc sets it when running doctests.
    Doctest,
    /// `feature = "name"`: resolved against default-enabled features.
    Feature(String),
    /// Any other atom (`unix`, `target_os = ".."`, custom cfgs): never
    /// established here.
    Unknown,
    All(Vec<DoctestCfg>),
    Any(Vec<DoctestCfg>),
    Not(Box<DoctestCfg>),
}

/// Nesting bound for an admission-position `cfg`: real gates nest a handful
/// of levels; anything past this bound refuses rather than recursing
/// without a limit.
const MAX_DOCTEST_CFG_NESTING: usize = 64;

/// Whether a `cfg(...)` predicate holds for a doctest built under cargo
/// defaults: `test` is off, `doctest` follows `doctest_on` (on between the
/// comment and the item, where rustdoc collects the example; off for the
/// file gate, which answers whether the file is in the library `cargo test`
/// builds), and `feature = "x"` follows `enabled`, the owning package's
/// default-enabled features. `None` is unestablished — another atom, a
/// feature with no manifest authority, or malformed input — and the caller
/// refuses rather than guesses.
fn cfg_satisfied_under_doctest_defaults(
    predicate: &str,
    enabled: Option<&BTreeSet<String>>,
    doctest_on: bool,
) -> Option<bool> {
    let mut parser = DoctestCfgParser {
        bytes: predicate.as_bytes(),
        pos: 0,
    };
    let cfg = parser.parse_predicate(0)?;
    parser.parse_end()?;
    cfg.evaluate(enabled, doctest_on)
}

impl DoctestCfg {
    fn evaluate(&self, enabled: Option<&BTreeSet<String>>, doctest_on: bool) -> Option<bool> {
        match self {
            DoctestCfg::Test => Some(false),
            DoctestCfg::Doctest => Some(doctest_on),
            DoctestCfg::Feature(name) => enabled.map(|set| set.contains(name)),
            DoctestCfg::Unknown => None,
            DoctestCfg::All(parts) => {
                let mut unknown = false;
                for part in parts {
                    match part.evaluate(enabled, doctest_on) {
                        Some(false) => return Some(false),
                        None => unknown = true,
                        Some(true) => {}
                    }
                }
                if unknown { None } else { Some(true) }
            }
            DoctestCfg::Any(parts) => {
                let mut unknown = false;
                for part in parts {
                    match part.evaluate(enabled, doctest_on) {
                        Some(true) => return Some(true),
                        None => unknown = true,
                        Some(false) => {}
                    }
                }
                if unknown { None } else { Some(false) }
            }
            DoctestCfg::Not(inner) => inner.evaluate(enabled, doctest_on).map(|value| !value),
        }
    }
}

struct DoctestCfgParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl DoctestCfgParser<'_> {
    fn parse_predicate(&mut self, depth: usize) -> Option<DoctestCfg> {
        if depth > MAX_DOCTEST_CFG_NESTING {
            return None;
        }
        self.skip_ws();
        let name = self.parse_ident()?;
        self.skip_ws();
        match self.bytes.get(self.pos) {
            Some(b'=') => {
                self.pos += 1;
                self.skip_ws();
                let value = self.parse_string()?;
                if name == "feature" {
                    Some(DoctestCfg::Feature(value))
                } else {
                    Some(DoctestCfg::Unknown)
                }
            }
            Some(b'(') => {
                self.pos += 1;
                let mut args = Vec::new();
                loop {
                    self.skip_ws();
                    // A trailing comma closes like `)`: `any(a,)` reads as
                    // `any(a)`, matching the shared cfg authority.
                    if self.bytes.get(self.pos) == Some(&b')') {
                        self.pos += 1;
                        break;
                    }
                    args.push(self.parse_predicate(depth + 1)?);
                    self.skip_ws();
                    match self.bytes.get(self.pos) {
                        Some(b',') => self.pos += 1,
                        Some(b')') => {
                            self.pos += 1;
                            break;
                        }
                        _ => return None,
                    }
                }
                match name.as_str() {
                    "all" => Some(DoctestCfg::All(args)),
                    "any" => Some(DoctestCfg::Any(args)),
                    "not" => match args.len() {
                        1 => Some(DoctestCfg::Not(Box::new(args.pop()?))),
                        _ => None,
                    },
                    // Only `all`/`any`/`not` combine predicates; anything
                    // else is malformed and refuses.
                    _ => None,
                }
            }
            _ => match name.as_str() {
                "test" => Some(DoctestCfg::Test),
                "doctest" => Some(DoctestCfg::Doctest),
                _ => Some(DoctestCfg::Unknown),
            },
        }
    }

    /// The predicate must consume its whole `cfg(...)` argument: a second
    /// predicate (`cfg(a, b)`) is malformed and refuses.
    fn parse_end(&mut self) -> Option<()> {
        self.skip_ws();
        (self.pos == self.bytes.len()).then_some(())
    }

    fn skip_ws(&mut self) {
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            self.pos += 1;
        }
    }

    fn parse_ident(&mut self) -> Option<String> {
        let start = self.pos;
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            self.pos += 1;
        }
        if self.pos == start {
            return None;
        }
        Some(
            str::from_utf8(self.bytes.get(start..self.pos)?)
                .ok()?
                .to_string(),
        )
    }

    /// A plain or raw string literal, compared literally: escapes cannot
    /// name a real feature, so they mismatch and refuse rather than guess.
    /// Anything else (byte strings, chars, numbers, comments) refuses.
    fn parse_string(&mut self) -> Option<String> {
        let (end, inner_start, inner_end) = if self.bytes.get(self.pos) == Some(&b'"') {
            let end = skip_regular_string(self.bytes, self.pos)?;
            (end, self.pos + 1, end - 1)
        } else if let Some((quote, hashes)) = raw_string_start(self.bytes, self.pos) {
            let end = skip_raw_string(self.bytes, quote, hashes)?;
            (end, quote + 1, end - hashes - 1)
        } else {
            return None;
        };
        let inner = self.bytes.get(inner_start..inner_end)?;
        self.pos = end;
        Some(str::from_utf8(inner).ok()?.to_string())
    }
}

fn attribute_path_and_optional_args(trimmed: &str) -> Option<(&str, Option<&str>)> {
    let body = attribute_bracket_body(trimmed)?;
    let body = body.trim_start();
    let path_end = body
        .find(|c: char| c == '(' || c.is_whitespace())
        .unwrap_or(body.len());
    let path = rust_ident_name(body.get(..path_end)?.trim());
    let rest = body.get(path_end..)?.trim_start();
    let Some(rest) = rest.strip_prefix('(') else {
        return Some((path, None));
    };
    let mut depth = 1usize;
    for (position, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((path, Some(rest.get(..position)?.trim())));
                }
            }
            _ => {}
        }
    }
    None
}

fn attribute_bracket_body(trimmed: &str) -> Option<&str> {
    let mut rest = trimmed.trim_start().strip_prefix('#')?;
    rest = rest.strip_prefix('!').unwrap_or(rest);
    rest = rest.trim_start().strip_prefix('[')?;
    let mut depth = 1usize;
    for (position, ch) in rest.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(rest.get(..position)?.trim());
                }
            }
            _ => {}
        }
    }
    None
}

fn whole_line_attribute(trimmed: &str) -> bool {
    let Some(body) = trimmed.strip_prefix("#[") else {
        return false;
    };
    let mut depth = 1usize;
    for (position, ch) in body.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    let rest = body[position + 1..].trim();
                    return rest.is_empty() || rest.starts_with("//");
                }
            }
            _ => {}
        }
    }
    false
}

/// Whether an attribute line carries doc metadata (`#[doc ..]` or a
/// `cfg_attr(.., doc = ..)`), as opposed to a name that merely contains `doc`.
fn attribute_bears_doc_metadata(trimmed: &str) -> bool {
    if !trimmed.starts_with("#[") {
        return false;
    }
    // Whitespace is insignificant between `#[` and the path and around `=`
    // (`#[ doc = ".."]` is a doc fragment), so compare the compacted text.
    let compact: String = trimmed.split_whitespace().collect();
    compact.starts_with("#[doc") || compact.contains(",doc=")
}

/// Whether the doc run `lines[start..end]` shares its item's document with
/// another `///` run or a doc-bearing attribute (`#[doc = ..]`,
/// `#[cfg_attr(.., doc = ..)]`), before or after it.
fn shares_its_document(lines: &[&str], start: usize, end: usize) -> bool {
    let doc_fragment = |line: &str| {
        let trimmed = line.trim_start();
        outer_doc_text(line).is_some() || attribute_bears_doc_metadata(trimmed)
    };
    let after = lines
        .iter()
        .skip(end)
        .take_while(|line| is_between_doc_and_item(line))
        .any(|line| doc_fragment(line));
    let before = lines[..start]
        .iter()
        .rev()
        .take_while(|line| is_between_doc_and_item(line))
        .any(|line| doc_fragment(line));
    after || before
}

type FencedBlock<'a> = (usize, &'a str, Vec<String>);

/// The fenced blocks of one doc run, as (fence line, info, restored code).
/// An unclosed fence yields nothing. So does a run where the backticks may
/// not open a code block: an HTML comment (a commented-out example), an
/// HTML block, a `~~~` fence that may wrap them, a tab-indented line, or a
/// fence line indented four or more spaces (an indented code block). The
/// scanner does not model Markdown block structure, so it refuses rather
/// than guesses.
fn run_examples<'a>(run: &[&'a str], first_line: usize) -> Vec<FencedBlock<'a>> {
    let markdown_context = run
        .iter()
        .filter_map(|line| outer_doc_text(line))
        .any(|text| {
            let trimmed = text.trim_start();
            let indent = text.len() - trimmed.len();
            text.contains("<!--")
                || opens_html_block(trimmed)
                || trimmed.starts_with("~~~")
                || text.starts_with('\t')
                || (indent >= 4 && trimmed.starts_with("```"))
        });
    if markdown_context {
        return Vec::new();
    }
    let mut blocks = Vec::new();
    let mut open: Option<(usize, usize, &str, Vec<String>)> = None;
    for (offset, line) in run.iter().enumerate() {
        let Some(text) = outer_doc_text(line) else {
            continue;
        };
        let line_number = first_line + offset;
        let trimmed = text.trim_start();
        let ticks = trimmed.chars().take_while(|c| *c == '`').count();
        match open.take() {
            None => {
                if ticks >= 3 {
                    open = Some((line_number, ticks, trimmed[ticks..].trim(), Vec::new()));
                }
            }
            Some((fence_line, fence_ticks, info, mut code)) => {
                if ticks >= fence_ticks && trimmed[ticks..].trim().is_empty() {
                    blocks.push((fence_line, info, code));
                } else {
                    code.push(restore_hidden(text));
                    open = Some((fence_line, fence_ticks, info, code));
                }
            }
        }
    }
    blocks
}

/// A line starting with a tag (`<details>`, `</div>`) may open a CommonMark
/// HTML block, inside which backticks are raw HTML that rustdoc does not
/// run. An autolink (`<https://..>`, `<a@b.c>`) opens none.
fn opens_html_block(trimmed: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix('<') else {
        return false;
    };
    let tag_start = rest
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '/');
    // An autolink holds no whitespace; `<img src="https://..">` is a tag.
    let link = rest.split('>').next().is_some_and(|inner| {
        !inner.contains(char::is_whitespace) && (inner.contains("://") || inner.contains('@'))
    });
    tag_start && !link
}

/// rustdoc compiles a hidden line (`# code`, or a bare `#`) without the
/// marker and reads `##` as a literal `#`.
fn restore_hidden(text: &str) -> String {
    let trimmed = text.trim_start();
    let indent = &text[..text.len() - trimmed.len()];
    if trimmed == "#" {
        String::new()
    } else if let Some(rest) = trimmed.strip_prefix("# ") {
        format!("{indent}{rest}")
    } else if trimmed.starts_with("##") {
        format!("{indent}{}", &trimmed[1..])
    } else {
        text.to_string()
    }
}

/// Whether a fence's info string leaves the block a runnable Rust test.
fn runnable_fence(info: &str) -> bool {
    info.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|word| !word.is_empty())
        .all(|word| RUNNABLE_FENCE_WORDS.contains(&word))
}

/// Whether the code declares `fn main`, which rustdoc runs unwrapped.
fn declares_main(code: &[String]) -> bool {
    code.iter()
        .any(|line| declared_fn_name(line).is_some_and(|name| name == "main"))
}

#[cfg(test)]
mod tests;
