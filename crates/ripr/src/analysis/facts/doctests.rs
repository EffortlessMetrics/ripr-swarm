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
//!   Rust library (`lib`, `rlib`, `proc-macro`, or no `crate-type`);
//! - the example documents a public production function of that file,
//!   declared on the line that follows the comment (a doc comment on a
//!   statement, or on a private function whose example cannot compile,
//!   is not one);
//! - the comment holds no `<!--`, `~~~` or fence indented four or more
//!   spaces, where CommonMark may read the backticks as something other
//!   than a code block;
//! - its fence is plain or names only `rust` or an edition (`ignore`,
//!   `no_run`, `compile_fail`, `should_panic`, `text` and every other
//!   attribute refuse);
//! - its code declares no `fn main` (rustdoc then runs the code as written,
//!   not wrapped) and parses cleanly.
//!
//! A doctest is its own crate: call identity reads its scopes from the
//! synthetic text and names the library by its import names, never through
//! `crate::` (`RustIndex::doctest_source`).

use super::{FunctionSourceRole, RustIndex, SourceRoleProvenanceEdgeKind, TestFact};
use crate::analysis::syntax::ra::summarize_file_with_parser;
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
        // Most files hold no fenced example; check that before the manifest.
        let examples = doc_examples(&file.data.source);
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
    for (test, source) in generated {
        let key = (test.file.clone(), test.start_line);
        if index.doctest_sources.contains_key(&key) {
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

/// Every runnable fenced example in the file's `///` comment runs.
pub(super) fn doc_examples(source: &str) -> Vec<DocExample> {
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
        let item_line = item_after(&lines, index);
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

/// The 1-based line of the first line after a doc run that is neither an
/// attribute, a blank line nor a plain comment: the documented item.
fn item_after(lines: &[&str], from: usize) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, line)| {
            let trimmed = line.trim_start();
            !(trimmed.is_empty() || trimmed.starts_with("#[") || trimmed.starts_with("//"))
        })
        .map(|(position, _)| position + 1)
}

type FencedBlock<'a> = (usize, &'a str, Vec<String>);

/// The fenced blocks of one doc run, as (fence line, info, restored code).
/// An unclosed fence yields nothing. So does a run where the backticks may
/// not open a code block: an HTML comment (a commented-out example), a
/// `~~~` fence that may wrap them, or a fence line indented four or more
/// spaces (an indented code block). The scanner does not model Markdown
/// block structure, so it refuses rather than guesses.
fn run_examples<'a>(run: &[&'a str], first_line: usize) -> Vec<FencedBlock<'a>> {
    let markdown_context = run
        .iter()
        .filter_map(|line| outer_doc_text(line))
        .any(|text| {
            let trimmed = text.trim_start();
            let indent = text.len() - trimmed.len();
            text.contains("<!--")
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
