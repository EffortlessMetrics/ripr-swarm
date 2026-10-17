//! `ripr agent stub`: resolve one gap to a compiling Rust test stub and,
//! when asked, write it.
//!
//! The producer is `analysis::test_stub`; this use case owns seam selection
//! (by seam id or by the `file:line` a `ripr check` finding prints), the
//! source read the producer works from, and the guarded write.
//!
//! It also owns the decision `ripr check` prints under its selected finding
//! (#5471): the route is printed only when this same resolver produces a
//! stub for that location, so the printed command and its answer cannot
//! disagree.

use crate::analysis;
use crate::analysis::owner_fn_line_span;
use crate::analysis::seams::SeamGripClass;
use crate::analysis::seams::SeamKind;
use crate::analysis::test_stub::{
    RustTestStub, TestStubPlacement, TestStubRefusal, rust_test_stub,
    rust_test_stub_for_classified_seam,
};
use crate::analysis::{ClassifiedSeam, RepoSeam};
use crate::app::CheckOutput;
use crate::config::RiprConfig;
use crate::domain::{ExposureClass, Finding};
use std::path::{Path, PathBuf};

/// Which gap the caller means.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TestStubSelector {
    SeamId(String),
    /// `file:line`, as `ripr check` prints a finding location. `kind` is
    /// the finding's probe family (`--kind`), when the caller has one.
    At {
        file: String,
        line: usize,
        kind: Option<String>,
    },
}

impl TestStubSelector {
    /// Attach `--kind FAMILY` to an `--at` selector. The family is the probe
    /// family `ripr check` reports (`predicate`, `return_value`,
    /// `error_path`, `match_arm`, ...); a name with no seam kind is refused.
    pub(crate) fn with_kind(self, family: &str) -> Result<Self, String> {
        let Self::At { file, line, .. } = self else {
            return Err("agent stub --kind applies to --at only".to_string());
        };
        if analysis::seam_kind_for_probe_family(family).is_none() {
            return Err(format!(
                "--kind expects a probe family such as predicate, return_value, error_path, match_arm, call_deletion, field_construction, or side_effect, got {family:?}"
            ));
        }
        Ok(Self::At {
            file,
            line,
            kind: Some(family.to_string()),
        })
    }

    pub(crate) fn parse_at(value: &str) -> Result<Self, String> {
        let (file, line) = value
            .rsplit_once(':')
            .ok_or_else(|| format!("--at expects FILE:LINE, got {value:?}"))?;
        let line = line.parse::<usize>().map_err(|error| {
            format!("--at expects FILE:LINE with a line number, got {value:?}: {error}")
        })?;
        if file.trim().is_empty() || line == 0 {
            return Err(format!("--at expects FILE:LINE, got {value:?}"));
        }
        Ok(Self::At {
            file: file.to_string(),
            line,
            kind: None,
        })
    }
}

#[derive(Debug)]
pub(crate) struct TestStubResolution {
    pub(crate) seam_id: String,
    pub(crate) owner: String,
    /// The owner file text the stub was computed from; the write re-reads
    /// and refuses when the file changed in between.
    pub(crate) source: String,
    pub(crate) outcome: Result<RustTestStub, TestStubRefusal>,
    /// Inventory grip class when classification named this seam.
    /// `--at --kind` selects from a file parse without classifying; the
    /// use case looks the class up afterward so the stub can disclose it
    /// (#7290). `None` when that lookup did not find the seam.
    pub(crate) grip_class: Option<SeamGripClass>,
    /// Set when `--at FILE:LINE` named a line that holds no gap and the
    /// nearest gap in the same function was stubbed instead (#6298), so the
    /// substitution is stated rather than left to the generated comment.
    pub(crate) at_note: Option<String>,
}

#[derive(Debug)]
pub(crate) enum TestStubError {
    /// The selector named no gap ripr can stub; the message says where to
    /// look instead.
    NotFound(String),
    /// Several candidate files or disjoint tied seams match the selector;
    /// no seam was selected and the message names how to disambiguate.
    Ambiguous(String),
    Operational(String),
}

pub(crate) fn resolve_test_stub(
    root: &Path,
    config: &RiprConfig,
    selector: &TestStubSelector,
) -> Result<TestStubResolution, TestStubError> {
    match selector {
        // A seam id is repo-wide, so it is looked up in the full inventory.
        TestStubSelector::SeamId(id) => {
            let (classified, _) = analysis::inventory_classified_seams_at_with_config(root, config)
                .map_err(TestStubError::Operational)?;
            let entry = classified
                .iter()
                .find(|entry| entry.seam.id().as_str() == id)
                .ok_or_else(|| TestStubError::NotFound(format!("seam_id {id} was not found")))?;
            let source = read_owner_source(root, &entry.seam)?;
            Ok(resolution_for(entry, source))
        }
        TestStubSelector::At { file, line, kind } => {
            let mut resolution = resolve_at_location(root, config, file, *line, kind.as_deref())?;
            if resolution.grip_class.is_none() {
                resolution.grip_class =
                    inventory_grip_class(root, config, file, &resolution.seam_id);
            }
            Ok(resolution)
        }
    }
}

/// A validated probe-family spelling paired with its selection kind.
#[derive(Clone, Copy)]
struct AtKind<'a> {
    family: &'a str,
    seam_kind: SeamKind,
}

/// The one `--at FILE:LINE` resolver, shared by `ripr agent stub --at` and
/// the route `ripr check` prints (#5471).
///
/// With `kind` (the finding's probe family, `--kind`) and a `file` under
/// `root`, the candidates are that file's seams from a parse of the file
/// alone: no workspace index, test evidence, or classification, so the
/// finding `check` reported is not re-judged by a second classifier (which
/// is how the route and the command disagreed). `check` already decided the
/// location is a gap of that kind; the stub needs only the seam's shape and
/// the owner source, and is placed inline (an integration-file placement
/// needs classified evidence, so it stays on `--seam-id`). Only seams of the
/// matching seam kind are tried, in line-then-nearest order, so a line
/// holding a boundary and an error variant stubs the one `check` reported
/// and never a seam of another kind.
///
/// Without `kind` nothing has vouched for the location, so a file under
/// `root` is classified in a scan scoped to that file and only its reported
/// gaps are tried: a bare `--at` never stubs a seam the tests already pin.
///
/// A suffix that names no file under `root` (`src/lib.rs` for
/// `crates/a/src/lib.rs`) keeps the repo-wide classified lookup so its
/// more-than-one-file refusal still applies; `check` never prints that form.
fn resolve_at_location(
    root: &Path,
    config: &RiprConfig,
    file: &str,
    line: usize,
    kind: Option<&str>,
) -> Result<TestStubResolution, TestStubError> {
    let kind = kind.and_then(|family| {
        analysis::seam_kind_for_probe_family(family).map(|seam_kind| AtKind { family, seam_kind })
    });
    match (scoped_file(root, file), kind) {
        (Some(relative), Some(_)) => {
            let seams =
                analysis::file_seams_without_evidence_at_with_config(root, config, &relative)
                    .map_err(TestStubError::Operational)?;
            let candidates = seams.iter().map(AtCandidate::Shape).collect::<Vec<_>>();
            resolve_at(root, candidates, file, line, kind)
        }
        (Some(relative), None) => {
            let scoped = analysis::inventory_diff_scoped_classified_seams_at_with_config(
                root,
                config,
                &[relative],
                &[],
            )
            .map_err(TestStubError::Operational)?;
            resolve_at(
                root,
                classified_candidates(&scoped.classified),
                file,
                line,
                kind,
            )
        }
        (None, _) => {
            let (classified, _) = analysis::inventory_classified_seams_at_with_config(root, config)
                .map_err(TestStubError::Operational)?;
            resolve_at(root, classified_candidates(&classified), file, line, kind)
        }
    }
}

/// One `--at` candidate: a seam shape from a single-file parse, or a
/// classified seam from the repo-wide inventory.
#[derive(Clone, Copy)]
enum AtCandidate<'a> {
    Shape(&'a RepoSeam),
    Classified(&'a ClassifiedSeam),
}

impl<'a> AtCandidate<'a> {
    fn seam(self) -> &'a RepoSeam {
        match self {
            Self::Shape(seam) => seam,
            Self::Classified(entry) => &entry.seam,
        }
    }

    fn resolution(self, source: String) -> TestStubResolution {
        match self {
            Self::Shape(seam) => {
                let outcome = rust_test_stub(seam, None, &source);
                TestStubResolution {
                    seam_id: seam.id().as_str().to_string(),
                    owner: seam.owner().to_string(),
                    source,
                    outcome,
                    grip_class: None,
                    at_note: None,
                }
            }
            Self::Classified(entry) => resolution_for(entry, source),
        }
    }
}

/// The reported gaps of a classified inventory as `--at` candidates.
fn classified_candidates(classified: &[ClassifiedSeam]) -> Vec<AtCandidate<'_>> {
    classified
        .iter()
        .filter(|entry| entry.class.is_headline_eligible())
        .map(AtCandidate::Classified)
        .collect()
}

/// `file` as a root-relative path when it names a regular file under
/// `root`; `None` otherwise. Both sides are canonicalized, so `..` or a
/// symlink cannot point `--at` (and `--write`) at a file outside the root.
fn scoped_file(root: &Path, file: &str) -> Option<PathBuf> {
    let normalized = file.replace('\\', "/");
    let path = Path::new(normalized.trim_start_matches("./"));
    let canonical_root = root.canonicalize().ok()?;
    let target = if path.is_absolute() {
        path.canonicalize().ok()?
    } else {
        root.join(path).canonicalize().ok()?
    };
    let relative = target.strip_prefix(&canonical_root).ok()?.to_path_buf();
    (!relative.as_os_str().is_empty() && target.is_file()).then_some(relative)
}

/// What `ripr check` prints under its selected finding for the test-stub
/// route (#5471).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StubRouteDecision {
    /// The shared resolver produces a stub here: print
    /// `ripr agent stub --at FILE:LINE --kind FAMILY`.
    Stub {
        file: String,
        line: usize,
        kind: &'static str,
    },
    /// The shared resolver refuses: print its reason instead of a command
    /// that would only refuse.
    Refused { reason: &'static str },
}

/// A route decision bound to the finding it was computed for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StubRoute {
    pub(crate) finding_id: String,
    pub(crate) decision: StubRouteDecision,
}

/// The `FILE:LINE` and probe family a stub route names for `finding`, the
/// file relative to `root` because `--at` resolves against `--root`. `None`
/// when the finding is not a non-exposed Rust gap in a family the stub
/// producer handles.
pub(crate) fn stub_route_location(
    finding: &Finding,
    root: &Path,
) -> Option<(String, usize, &'static str)> {
    let location = &finding.probe.location.file;
    let eligible = finding.class != ExposureClass::Exposed
        && finding.probe.offers_stub_route()
        && location.extension().and_then(|ext| ext.to_str()) == Some("rs");
    if !eligible {
        return None;
    }
    let relative = location.strip_prefix(root).unwrap_or(location);
    let file = crate::output::path::display_path(relative);
    Some((
        file.trim_start_matches("./").to_string(),
        finding.probe.location.line,
        finding.probe.family.as_str(),
    ))
}

/// The route decision for one finding, from the resolver
/// `ripr agent stub --at` runs. `None` when the finding gets no route or the
/// resolver finds no seam there; nothing is printed then.
pub(crate) fn stub_route_for_finding(
    root: &Path,
    config: &RiprConfig,
    finding: &Finding,
) -> Option<StubRoute> {
    let (file, line, kind) = stub_route_location(finding, root)?;
    if let Some(name) = finding.probe.changed_const_name() {
        let decision = const_consumer_route(root, config, &file, line, name)?;
        return Some(StubRoute {
            finding_id: finding.id.clone(),
            decision,
        });
    }
    let resolution = resolve_at_location(root, config, &file, line, Some(kind)).ok()?;
    let decision = match resolution.outcome {
        Ok(_) => StubRouteDecision::Stub { file, line, kind },
        Err(refusal) => StubRouteDecision::Refused {
            reason: refusal.reason(),
        },
    };
    Some(StubRoute {
        finding_id: finding.id.clone(),
        decision,
    })
}

/// Seam kinds a constant's consumer is tried as, most common first.
const CONSUMER_KINDS: [&str; 4] = ["return_value", "predicate", "error_path", "match_arm"];

/// The most consuming functions of a changed constant tried for a stub.
const CONSUMER_FN_LIMIT: usize = 6;

/// A changed `const`/`static` has no seam, so the route goes to the first
/// function in the same file that names it: a stub for that function is the
/// place to pin the constant's effect. The first consumer that yields a stub
/// wins; otherwise the first refusal names why. `None` when no consumer is
/// found or none resolves, so nothing is printed.
fn const_consumer_route(
    root: &Path,
    config: &RiprConfig,
    file: &str,
    const_line: usize,
    name: &str,
) -> Option<StubRouteDecision> {
    let source = std::fs::read_to_string(root.join(file)).ok()?;
    let lines = source.lines().collect::<Vec<_>>();
    let code = code_lines_of(&source);
    // Test modules (the parser's governed `cfg(test)` inventory) are never
    // stub targets; an unparsed file has no consumer to find anyway.
    let test_start = analysis::governed_cfg_test_modules(&source)?
        .iter()
        .map(|module| source[..module.item_start].matches('\n').count() + 1)
        .min()
        .unwrap_or(lines.len() + 1);
    // The same name declared twice (an associated const per impl, a const
    // generic, a local `const`) cannot be told apart by spelling: no route.
    if code
        .iter()
        .filter(|text| declares_constant(text, name))
        .count()
        > 1
    {
        return None;
    }
    let mut seen_fns: Vec<(usize, usize)> = Vec::new();
    let mut tried = 0;
    let mut first_refusal = None;
    for index in 0..lines.len() {
        let line = index + 1;
        if line == const_line
            || line >= test_start
            || !code
                .get(index)
                .is_some_and(|code| names_identifier(code, name))
        {
            continue;
        }
        if seen_fns
            .iter()
            .any(|&(start, end)| (start..=end).contains(&line))
        {
            continue;
        }
        let Some(span) = owner_fn_line_span(&source, line) else {
            continue;
        };
        seen_fns.push(span);
        if is_test_fn(&lines, span.0) {
            continue;
        }
        tried += 1;
        if tried > CONSUMER_FN_LIMIT {
            break;
        }
        for kind in CONSUMER_KINDS {
            let Ok(resolution) = resolve_at_location(root, config, file, line, Some(kind)) else {
                continue;
            };
            match resolution.outcome {
                Ok(_) => {
                    return Some(StubRouteDecision::Stub {
                        file: file.to_string(),
                        line,
                        kind,
                    });
                }
                Err(refusal) => {
                    first_refusal.get_or_insert(StubRouteDecision::Refused {
                        reason: refusal.reason(),
                    });
                }
            }
        }
    }
    first_refusal
}

/// `source` with string, char and raw-string literals and comments blanked,
/// keeping every newline so line numbers hold. Block comments nest and
/// strings, raw strings and block comments may span lines; a leading `*` is
/// code (a dereference) unless a block comment is open. A `'` that opens no
/// char literal is a lifetime and stays.
fn code_lines_of(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let (mut block_depth, mut in_string, mut in_line_comment) = (0usize, false, false);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '\n' {
            in_line_comment = false;
            out.push('\n');
        } else if in_line_comment {
            out.push(' ');
        } else if block_depth > 0 {
            if c == '*' && next == Some('/') {
                block_depth -= 1;
                out.push_str("  ");
                i += 1;
            } else if c == '/' && next == Some('*') {
                block_depth += 1;
                out.push_str("  ");
                i += 1;
            } else {
                out.push(' ');
            }
        } else if in_string {
            if c == '\\' && next.is_some_and(|next| next != '\n') {
                out.push_str("  ");
                i += 1;
            } else {
                in_string = c != '"';
                out.push(' ');
            }
        } else if c == '"' {
            in_string = true;
            out.push(' ');
        } else if c == '/' && next == Some('/') {
            in_line_comment = true;
            out.push_str("  ");
            i += 1;
        } else if c == '/' && next == Some('*') {
            block_depth += 1;
            out.push_str("  ");
            i += 1;
        } else if let Some(end) = raw_string_end(&chars, i).or_else(|| char_literal_end(&chars, i))
        {
            for &blanked in &chars[i..end] {
                out.push(if blanked == '\n' { '\n' } else { ' ' });
            }
            i = end - 1;
        } else {
            out.push(c);
        }
        i += 1;
    }
    out.lines().map(str::to_string).collect()
}

/// The exclusive end of a raw string (`r"…"`, `r#"…"#`, `br"…"`) starting at
/// `at`, running to the end of the source when it never closes.
fn raw_string_end(chars: &[char], at: usize) -> Option<usize> {
    let after_prefix = match (chars[at], chars.get(at + 1)) {
        ('r', _) => at + 1,
        ('b', Some('r')) => at + 2,
        _ => return None,
    };
    if at > 0 && (chars[at - 1].is_alphanumeric() || chars[at - 1] == '_') {
        return None;
    }
    let hashes = chars[after_prefix..]
        .iter()
        .take_while(|&&c| c == '#')
        .count();
    if chars.get(after_prefix + hashes) != Some(&'"') {
        return None;
    }
    let body = after_prefix + hashes + 1;
    let closer = (body..chars.len()).find(|&k| {
        chars[k] == '"' && (1..=hashes).all(|offset| chars.get(k + offset) == Some(&'#'))
    });
    Some(closer.map_or(chars.len(), |k| k + 1 + hashes))
}

/// The exclusive end of a char literal (`'x'`, `'\\''`, `'\\u{1F600}'`)
/// starting at `at`; `None` for a lifetime.
fn char_literal_end(chars: &[char], at: usize) -> Option<usize> {
    if chars[at] != '\'' {
        return None;
    }
    match (chars.get(at + 1)?, chars.get(at + 2)) {
        ('\\', _) => (at + 3..chars.len().min(at + 12))
            .find(|&k| chars[k] == '\'')
            .map(|k| k + 1),
        (c, Some('\'')) if *c != '\'' && *c != '\n' => Some(at + 3),
        _ => None,
    }
}

/// Whether the function starting at `start` (1-based) is a test: one of the
/// attribute lines directly above it names `test`, `rstest` or `test_case`.
fn is_test_fn(lines: &[&str], start: usize) -> bool {
    let above = start.saturating_sub(1).min(lines.len());
    lines[..above]
        .iter()
        .rev()
        .take(8)
        .map(|text| text.trim())
        .take_while(|text| {
            !text.is_empty()
                && (text.starts_with("#[")
                    || text.starts_with("//")
                    || !text.ends_with(['{', '}', ';']))
        })
        .filter(|text| text.starts_with("#["))
        .any(|text| {
            let path = text[2..].split(['(', ']', '=']).next().unwrap_or("").trim();
            matches!(
                path.rsplit("::").next(),
                Some("test" | "rstest" | "test_case")
            )
        })
}

/// Whether `text` declares `name` as a `const` or `static`, as an item, a
/// local or a const generic parameter.
fn declares_constant(text: &str, name: &str) -> bool {
    let words = text
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    words
        .windows(2)
        .any(|window| matches!(window, ["const" | "static", declared] if *declared == name))
        || words
            .windows(3)
            .any(|window| matches!(window, ["static", "mut", declared] if *declared == name))
}

/// Whether `text` reads `name` as a whole identifier: not a field access (a
/// range `0..CAP` is a read), and
/// not behind a path other than `Self::`, `self::`, `crate::` or `super::`
/// (another module's or type's item of the same spelling).
fn names_identifier(text: &str, name: &str) -> bool {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(name).any(|(at, _)| {
        let before = &text[..at];
        if before.chars().next_back().is_some_and(is_ident)
            || text[at + name.len()..].chars().next().is_some_and(is_ident)
        {
            return false;
        }
        if before.ends_with('.') && !before.ends_with("..") {
            return false;
        }
        match before.strip_suffix("::") {
            Some(path) => {
                let segment = path
                    .rsplit(|c: char| !is_ident(c))
                    .next()
                    .unwrap_or_default();
                matches!(segment, "Self" | "self" | "crate" | "super")
            }
            None => true,
        }
    })
}

/// The route decision for the one finding the default human `ripr check`
/// selects. `check_config` drives that selection exactly as the renderer
/// does. Resolution loads configuration the way `ripr agent stub` does for
/// the same `root`, so the printed decision is that command's answer. It
/// parses only the selected finding's file: no workspace index or evidence.
pub(crate) fn check_stub_route(
    root: &Path,
    check_config: &RiprConfig,
    output: &CheckOutput,
) -> Option<StubRoute> {
    let finding = crate::output::human::selected_triage_finding(output, check_config)?;
    let (file, line, _) = stub_route_location(finding, root)?;
    if !finding_text_is_on_disk(root, &file, line, &finding.probe.expression) {
        return None;
    }
    let config = crate::config::load_for_root(root).ok()?;
    stub_route_for_finding(root, &config, finding)
}

/// Whether the finding's expression is still on disk in the function the
/// resolver will search (the finding line alone outside a function). A
/// `--diff` patch (a file or stdin) can disagree with the checkout, and a
/// removed-only finding names text the disk no longer holds; the resolver
/// reads the disk, so either would route to a seam the finding never named.
/// The function, not the exact line, is the scope because hunk headers can
/// be off by a line while the text is the same; whitespace is ignored so a
/// multi-line expression still matches.
fn finding_text_is_on_disk(root: &Path, file: &str, line: usize, expression: &str) -> bool {
    let squash = |text: &str| text.split_whitespace().collect::<String>();
    let wanted = squash(expression);
    if wanted.is_empty() || line == 0 {
        return false;
    }
    let Ok(source) = std::fs::read_to_string(root.join(file)) else {
        return false;
    };
    let (start, end) = owner_fn_line_span(&source, line).unwrap_or((line, line));
    let scope = source
        .lines()
        .skip(start.saturating_sub(1))
        .take(end.saturating_sub(start) + 1)
        .collect::<Vec<_>>()
        .join(" ");
    squash(&scope).contains(&wanted)
}

/// A `ripr check` finding line is the changed line, which is not always a
/// seam's own line. Candidates are the seams on that line, then the seams in
/// the same function nearest first, only those of seam kind `kind` when it
/// is given; the first one that yields a stub wins, and when none does, the
/// first candidate's refusal names why.
fn resolve_at(
    root: &Path,
    candidates: Vec<AtCandidate<'_>>,
    file: &str,
    line: usize,
    kind: Option<AtKind<'_>>,
) -> Result<TestStubResolution, TestStubError> {
    // An empty list says nothing about its source; only the `--kind` path
    // gathers shape candidates, so without a kind it was a gap list.
    let noun = match (candidates.first(), kind) {
        (Some(AtCandidate::Classified(_)), _) | (None, None) => "reported gap",
        _ => "seam ripr can stub",
    };
    let in_file = candidates
        .into_iter()
        .filter(|candidate| paths_match(candidate.seam().file(), file))
        .collect::<Vec<_>>();
    let mut files = in_file
        .iter()
        .map(|candidate| candidate.seam().file().to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    if files.len() > 1 {
        // A suffix like `src/lib.rs` can name several crates' files; reading
        // one file and applying another file's seam offsets would build a
        // stub for the wrong function.
        return Err(TestStubError::Ambiguous(format!(
            "{file} matches more than one file with gaps ({}); pass one of those paths or a seam ID",
            files.join(", ")
        )));
    }
    let Some(first) = in_file.first() else {
        // Name the parse failure: a file ripr cannot parse has no seams,
        // which is a static limit, not evidence that nothing changed.
        let unparsed = std::fs::read_to_string(root.join(file))
            .is_ok_and(|source| !analysis::rust_source_parses_cleanly(&source));
        let why = if unparsed {
            "; the file did not parse cleanly as current-edition Rust, so ripr has no seams for it"
        } else {
            ""
        };
        return Err(TestStubError::NotFound(format!(
            "no {noun} is in {file}{why}"
        )));
    };
    let source = read_owner_source(root, first.seam())?;
    let span = owner_fn_line_span(&source, line);
    let mut candidates = in_file
        .iter()
        .copied()
        .filter(|candidate| {
            let seam_line = candidate.seam().display_line();
            seam_line == line
                || span.is_some_and(|(start, end)| start <= seam_line && seam_line <= end)
        })
        .collect::<Vec<_>>();
    // With `--kind`, only a seam of the finding's kind speaks for it:
    // another kind's stub targets a different behavior (#6298) and its
    // refusal names the wrong blocker.
    if let Some(kind) = kind {
        candidates.retain(|candidate| candidate.seam().kind() == kind.seam_kind);
    }
    candidates.sort_by_key(|candidate| candidate.seam().display_line().abs_diff(line));
    // A single-file parse lists every seam, not only the reported gaps, so
    // two disjoint seams of one kind that rank equally (`a > 10 && b > 20`)
    // cannot be told apart by `--at` and `--kind`. Picking the first could
    // stub a boundary the tests already pin, so refuse and name the seam
    // IDs. Nested seams (`return Err(E)` around `E`) are one behavior.
    if let [lead, rest @ ..] = candidates.as_slice()
        && matches!(lead, AtCandidate::Shape(_))
    {
        let rank = |candidate: &AtCandidate<'_>| {
            (
                candidate.seam().kind(),
                candidate.seam().display_line().abs_diff(line),
            )
        };
        let tied = std::iter::once(lead)
            .chain(
                rest.iter()
                    .filter(|candidate| rank(candidate) == rank(lead)),
            )
            .map(|candidate| candidate.seam())
            .collect::<Vec<_>>();
        let disjoint = tied.iter().enumerate().any(|(index, a)| {
            tied[index + 1..]
                .iter()
                .any(|b| !seam_spans_nest(a, b) && !seam_spans_nest(b, a))
        });
        if disjoint {
            let ids = tied
                .iter()
                .map(|seam| format!("--seam-id {}", seam.id().as_str()))
                .collect::<Vec<_>>();
            return Err(TestStubError::Ambiguous(format!(
                "{file}:{} holds more than one {} seam, so --at cannot pick one; pass one of: {}",
                lead.seam().display_line(),
                lead.seam().kind().as_str(),
                ids.join(", ")
            )));
        }
    }
    let mut first_refusal = None;
    for candidate in candidates {
        let seam_line = candidate.seam().display_line();
        let mut resolution = candidate.resolution(source.clone());
        // A finding line is the changed line, which need not hold a gap of
        // its own; when the stub is for a different line, say so (#6298).
        if seam_line != line {
            resolution.at_note = Some(format!(
                "note: {file}:{line} has no stubbable gap; this stub is for the nearest one in `{}`, {file}:{seam_line}",
                resolution.owner
            ));
        }
        if resolution.outcome.is_ok() {
            return Ok(resolution);
        }
        first_refusal.get_or_insert(resolution);
    }
    if let Some(resolution) = first_refusal {
        return Ok(resolution);
    }
    let mut nearby = in_file;
    if let Some(kind) = kind {
        nearby.retain(|candidate| candidate.seam().kind() == kind.seam_kind);
    }
    nearby.sort_by_key(|candidate| candidate.seam().display_line().abs_diff(line));
    let listed = nearby
        .iter()
        .take(5)
        .map(|candidate| {
            format!(
                "{file}:{} (--seam-id {})",
                candidate.seam().display_line(),
                candidate.seam().id().as_str()
            )
        })
        .collect::<Vec<_>>();
    let noun = match kind {
        Some(kind) => format!("{} {noun}", kind.family),
        None => noun.to_string(),
    };
    Err(TestStubError::NotFound(format!(
        "no {noun} is in the function at {file}:{line}; nearest: {}",
        listed.join(", ")
    )))
}

/// Whether `inner`'s source span lies within `outer`'s.
fn seam_spans_nest(outer: &RepoSeam, inner: &RepoSeam) -> bool {
    outer.byte_offset() <= inner.byte_offset()
        && inner.byte_offset() + inner.expression().len()
            <= outer.byte_offset() + outer.expression().len()
}

fn read_owner_source(root: &Path, seam: &RepoSeam) -> Result<String, TestStubError> {
    std::fs::read_to_string(root.join(seam.file())).map_err(|error| {
        TestStubError::Operational(format!("failed to read {}: {error}", seam.file().display()))
    })
}

fn resolution_for(entry: &ClassifiedSeam, source: String) -> TestStubResolution {
    let outcome = rust_test_stub_for_classified_seam(entry, &source);
    TestStubResolution {
        seam_id: entry.seam.id().as_str().to_string(),
        owner: entry.seam.owner().to_string(),
        source,
        outcome,
        grip_class: Some(entry.class),
        at_note: None,
    }
}

/// Inventory class for a seam selected from a file parse (`--at --kind`).
/// Classification is not used to pick the seam (#5471); this lookup is
/// disclosure only. A failed or empty inventory leaves `None`.
fn inventory_grip_class(
    root: &Path,
    config: &RiprConfig,
    file: &str,
    seam_id: &str,
) -> Option<SeamGripClass> {
    let relative = scoped_file(root, file)?;
    let scoped = analysis::inventory_diff_scoped_classified_seams_at_with_config(
        root,
        config,
        &[relative],
        &[],
    )
    .ok()?;
    scoped
        .classified
        .iter()
        .find(|entry| entry.seam.id().as_str() == seam_id)
        .map(|entry| entry.class)
}

/// Exact or path-suffix match in either direction, after normalizing
/// separators and a leading `./`, the same leniency `ripr explain` applies.
fn paths_match(stored: &Path, wanted: &str) -> bool {
    let normalize = |text: &str| {
        text.replace('\\', "/")
            .trim_start_matches("./")
            .trim_matches('/')
            .to_string()
    };
    let stored = normalize(&analysis::stable_path_text(stored));
    let wanted = normalize(wanted);
    !stored.is_empty()
        && !wanted.is_empty()
        && (stored == wanted
            || stored.ends_with(&format!("/{wanted}"))
            || wanted.ends_with(&format!("/{stored}")))
}

/// 1-based line of a byte offset in `source`.
pub(crate) fn line_of_offset(source: &str, offset: usize) -> usize {
    source
        .get(..offset)
        .map(|prefix| prefix.matches('\n').count() + 1)
        .unwrap_or(1)
}

/// Apply the stub: insert into the owner file, or create the new
/// integration file. Refuses when the owner file changed since the stub was
/// computed, or the new file already exists.
pub(crate) fn write_test_stub(
    root: &Path,
    resolution: &TestStubResolution,
    stub: &RustTestStub,
) -> Result<PathBuf, String> {
    let relative = stub.placement.file();
    let path = root.join(relative);
    match &stub.placement {
        TestStubPlacement::ExistingInlineModule { offset, .. }
        | TestStubPlacement::NewInlineModule { offset, .. } => {
            let current = std::fs::read_to_string(&path)
                .map_err(|error| format!("failed to read {}: {error}", relative.display()))?;
            if current != resolution.source {
                return Err(format!(
                    "{} changed after the stub was computed; rerun `ripr agent stub`",
                    relative.display()
                ));
            }
            let (head, tail) = current.split_at_checked(*offset).ok_or_else(|| {
                format!(
                    "stub offset {offset} is outside {}; rerun `ripr agent stub`",
                    relative.display()
                )
            })?;
            let updated = format!("{head}{}{tail}", stub.text);
            crate::atomic_file::write(&path, updated.as_bytes(), "test stub")?;
        }
        TestStubPlacement::NewIntegrationFile { .. } => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
            }
            crate::atomic_file::create_new(&path, stub.text.as_bytes()).map_err(|error| {
                let error = match error {
                    crate::atomic_file::CreateNewError::Staging(error)
                    | crate::atomic_file::CreateNewError::Link(error) => error,
                };
                format!("failed to create {}: {error}", relative.display())
            })?;
        }
    }
    Ok(relative.to_path_buf())
}

/// The root-relative manifest of the Cargo package that will own the stub:
/// the nearest `Cargo.toml` with a `[package]` table at or above the stub's
/// file, bounded by `root`. `None` when no package owns it, so no run command
/// is offered rather than one that would test a different package.
pub(crate) fn package_manifest_for(root: &Path, stub: &RustTestStub) -> Option<PathBuf> {
    stub.placement.file().ancestors().skip(1).find_map(|dir| {
        let manifest = dir.join("Cargo.toml");
        let text = std::fs::read_to_string(root.join(&manifest)).ok()?;
        let parsed = text.parse::<toml::Table>().ok()?;
        parsed.contains_key("package").then_some(manifest)
    })
}

/// The command that runs just this test after the stub is in place.
/// `manifest` is the package manifest as the caller should spell it.
pub(crate) fn run_command(manifest: &str, stub: &RustTestStub) -> String {
    match &stub.placement {
        TestStubPlacement::NewIntegrationFile { file } => {
            let target = file
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_default();
            format!(
                "cargo test --manifest-path {manifest} --test {target} {}",
                stub.test_name
            )
        }
        _ => format!("cargo test --manifest-path {manifest} {}", stub.test_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_lines_blank_strings_and_comments_but_keep_dereferences() {
        let uses =
            |source: &str, line: usize| names_identifier(&code_lines_of(source)[line - 1], "CAP");
        assert!(!uses("let s = \"CAP\";", 1));
        assert!(!uses("let s = \"a\\\"CAP\";", 1));
        assert!(!uses("x + 1 // CAP", 1));
        assert!(!uses("/* CAP */", 1));
        assert!(!uses("/*\n * the CAP applies\n */\nlet x = 1;", 2));
        assert!(!uses("/* a /* b */ CAP */", 1));
        assert!(!uses("let s = \"line one\nCAP two\";", 2));
        assert!(uses("x.min(CAP) // cap", 1));
        assert!(uses("let s = \"x\"; y.min(CAP)", 1));
        assert!(uses("*total += CAP;", 1));
        assert!(uses("let y = a\n    * CAP;", 2));
        assert!(uses("/* c */ CAP", 1));
        // char literals, escapes and raw strings do not desync the scan
        assert!(!uses("let q = '\"'; // CAP", 1));
        assert!(uses("let q = '\"';\nx.min(CAP)", 2));
        assert!(!uses("let q = '\\'';\nlet s = \"CAP\";", 2));
        assert!(uses("let r = r\"a\\\";\nx.min(CAP)", 2));
        assert!(!uses("let r = r#\"say \"CAP\"\"#;", 1));
        assert!(!uses("let r = br\"CAP\";", 1));
        assert!(uses("fn f<'a>(x: &'a u64) -> u64 { x.min(CAP) }", 1));
        assert_eq!(code_lines_of("a\n// b\nc").len(), 3);
    }

    #[test]
    fn names_identifier_matches_whole_identifiers_only() {
        assert!(names_identifier("over.min(CAP) * rate", "CAP"));
        assert!(names_identifier("CAP", "CAP"));
        assert!(!names_identifier("over.min(CAPS)", "CAP"));
        assert!(!names_identifier("MAX_CAP + 1", "CAP"));
        assert!(!names_identifier("cap", "CAP"));
    }

    #[test]
    fn names_identifier_rejects_fields_and_foreign_paths() {
        assert!(names_identifier("Self::CAP + 1", "CAP"));
        assert!(names_identifier("crate::CAP", "CAP"));
        assert!(names_identifier("super::CAP", "CAP"));
        assert!(!names_identifier("other::CAP", "CAP"));
        assert!(!names_identifier("a::b::CAP", "CAP"));
        assert!(!names_identifier("self.CAP", "CAP"));
        assert!(names_identifier("for i in 0..CAP {", "CAP"));
        assert!(names_identifier("for i in 0..=CAP {", "CAP"));
        assert!(names_identifier("&buf[..CAP]", "CAP"));
    }

    #[test]
    fn declares_constant_sees_items_locals_and_const_generics() {
        assert!(declares_constant("pub const CAP: u64 = 5;", "CAP"));
        assert!(declares_constant("static mut CAP: u64 = 5;", "CAP"));
        assert!(declares_constant("fn f<const CAP: usize>() {}", "CAP"));
        assert!(!declares_constant("const CAPS: u64 = 5;", "CAP"));
        assert!(!declares_constant("let x = CAP;", "CAP"));
    }

    #[test]
    fn is_test_fn_reads_the_attributes_directly_above() {
        let lines = |text: &'static str| text.lines().collect::<Vec<_>>();
        let tokio = lines("#[tokio::test]\nasync fn t() {}");
        assert!(is_test_fn(&tokio, 2));
        let stacked = lines(
            "#[test]\n#[should_panic(\n    expected = \"x\"\n)]\n/// doc\n#[ignore]\nfn t() {}",
        );
        assert!(is_test_fn(&stacked, 7));
        let doc_ends_in_brace =
            lines("#[test]\n/// takes the `{` branch;\n#[cfg(feature = \"x\")]\nfn t() {}");
        assert!(is_test_fn(&doc_ends_in_brace, 4));
        let neighbour = lines("#[test]\nfn t() {}\nfn prod() {}");
        assert!(!is_test_fn(&neighbour, 3));
        assert!(!is_test_fn(&lines("#[inline]\nfn f() {}"), 2));
    }

    #[test]
    fn at_selector_parses_file_and_line_and_refuses_junk() {
        assert_eq!(
            TestStubSelector::parse_at("src/lib.rs:12"),
            Ok(TestStubSelector::At {
                file: "src/lib.rs".to_string(),
                line: 12,
                kind: None
            })
        );
        for junk in ["src/lib.rs", "src/lib.rs:x", ":3", "src/lib.rs:0"] {
            assert!(
                matches!(TestStubSelector::parse_at(junk), Err(message) if message.contains("FILE:LINE")),
                "{junk}"
            );
        }
    }

    #[test]
    fn path_match_accepts_suffixes_in_either_direction() {
        assert!(paths_match(Path::new("crates/a/src/lib.rs"), "src/lib.rs"));
        assert!(paths_match(
            Path::new("src/lib.rs"),
            "./crates/a/src/lib.rs"
        ));
        assert!(paths_match(Path::new("src\\lib.rs"), "src/lib.rs"));
        assert!(!paths_match(Path::new("src/lib.rs"), "lib.rs.bak"));
        assert!(!paths_match(Path::new("src/mylib.rs"), "lib.rs"));
    }

    fn classified_at(file: &str, line: usize, class: SeamGripClass) -> ClassifiedSeam {
        use crate::analysis::seams::{ExpectedSink, RepoSeam, RequiredDiscriminator, SeamKind};
        use crate::analysis::test_grip_evidence::TestGripEvidence;
        use crate::domain::{Confidence, StageEvidence, StageState};
        let stage = || StageEvidence::new(StageState::Yes, Confidence::Medium, "test stage");
        let seam = RepoSeam::new(
            file,
            "owner",
            SeamKind::PredicateBoundary,
            0,
            line,
            "a >= b",
            RequiredDiscriminator::BoundaryValue {
                description: "a == b".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        ClassifiedSeam {
            evidence: TestGripEvidence {
                seam_id: seam.id().clone(),
                related_tests: Vec::new(),
                reach: stage(),
                activate: stage(),
                propagate: stage(),
                observe: stage(),
                discriminate: stage(),
                observed_values: Vec::new(),
                missing_discriminators: Vec::new(),
                statically_contradicted_related_tests: 0,
                new_test_target: None,
            },
            seam,
            class,
        }
    }

    fn ungripped(file: &str, line: usize) -> ClassifiedSeam {
        classified_at(file, line, SeamGripClass::Ungripped)
    }

    fn scratch_stub_crate(label: &str, source: &str) -> Result<PathBuf, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-stub-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            format!("[package]\nname = \"{label}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(root.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
        Ok(root)
    }

    #[test]
    fn at_selector_refuses_a_suffix_that_names_two_files_with_gaps() {
        let classified = vec![
            ungripped("crates/a/src/lib.rs", 2),
            ungripped("crates/b/src/lib.rs", 2),
        ];
        let result = resolve_at(
            Path::new("/nonexistent"),
            classified_candidates(&classified),
            "src/lib.rs",
            2,
            None,
        );
        assert!(
            matches!(&result, Err(TestStubError::Ambiguous(message))
                if message.contains("crates/a/src/lib.rs, crates/b/src/lib.rs")
                    && message.contains("seam ID")),
            "{:?}",
            result.err()
        );
        // One full path is unambiguous and reaches the source read.
        let one = resolve_at(
            Path::new("/nonexistent"),
            classified_candidates(&classified),
            "crates/a/src/lib.rs",
            2,
            None,
        );
        assert!(
            matches!(&one, Err(TestStubError::Operational(_))),
            "{:?}",
            one.err()
        );
    }

    #[test]
    fn run_command_targets_the_package_that_owns_the_stub() -> Result<(), String> {
        use crate::analysis::test_stub::TestStubPlacement;
        let root = std::env::temp_dir().join(format!(
            "ripr-stub-manifest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("crates/a/src")).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/a\"]\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("crates/a/Cargo.toml"),
            "[package]\nname = \"a\"\n",
        )
        .map_err(|error| error.to_string())?;
        let stub = |placement| RustTestStub {
            placement,
            test_name: "owner_boundary".to_string(),
            text: String::new(),
            fill_ins: Vec::new(),
            derived_inputs: Vec::new(),
        };
        let inline = stub(TestStubPlacement::NewInlineModule {
            file: PathBuf::from("crates/a/src/lib.rs"),
            offset: 0,
        });
        let integration = stub(TestStubPlacement::NewIntegrationFile {
            file: PathBuf::from("crates/a/tests/owner.rs"),
        });
        let orphan = stub(TestStubPlacement::NewInlineModule {
            file: PathBuf::from("scripts/tool.rs"),
            offset: 0,
        });
        let inline_manifest = package_manifest_for(&root, &inline);
        let integration_manifest = package_manifest_for(&root, &integration);
        let orphan_manifest = package_manifest_for(&root, &orphan);
        let _ = std::fs::remove_dir_all(&root);

        // The workspace manifest at the root has no [package]; it is skipped.
        assert_eq!(inline_manifest, Some(PathBuf::from("crates/a/Cargo.toml")));
        assert_eq!(
            integration_manifest,
            Some(PathBuf::from("crates/a/Cargo.toml"))
        );
        assert_eq!(orphan_manifest, None);
        assert_eq!(
            run_command("crates/a/Cargo.toml", &inline),
            "cargo test --manifest-path crates/a/Cargo.toml owner_boundary"
        );
        assert_eq!(
            run_command("crates/a/Cargo.toml", &integration),
            "cargo test --manifest-path crates/a/Cargo.toml --test owner owner_boundary"
        );
        Ok(())
    }

    #[test]
    fn kind_selects_a_probe_family_and_applies_to_at_only() {
        let at =
            TestStubSelector::parse_at("src/lib.rs:2").and_then(|at| at.with_kind("error_path"));
        assert_eq!(
            at,
            Ok(TestStubSelector::At {
                file: "src/lib.rs".to_string(),
                line: 2,
                kind: Some("error_path".to_string()),
            })
        );
        assert!(matches!(
            TestStubSelector::parse_at("src/lib.rs:2")
                .and_then(|at| at.with_kind("static_unknown")),
            Err(message) if message.contains("--kind expects a probe family")
        ));
        assert!(matches!(
            TestStubSelector::SeamId("abc".to_string()).with_kind("predicate"),
            Err(message) if message.contains("--at only")
        ));
    }

    /// #5471: a finding line can hold seams of several kinds. The finding's
    /// probe family picks the one `check` reported; without it the seam on
    /// the line wins.
    #[test]
    fn kind_ranks_the_matching_seam_ahead_of_a_nearer_one() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-stub-kind-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"kind\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn check(input: &str) -> Result<u8, E> {\n    if input.is_empty() {\n        return Err(E::Long);\n    }\n    Ok(1)\n}\n\n#[derive(Debug, PartialEq)]\npub enum E {\n    Long,\n}\n",
        )
        .map_err(|error| error.to_string())?;
        let config = RiprConfig::default();
        let kind_of = |kind: Option<&str>| -> Result<(String, bool), String> {
            let resolution = resolve_at_location(&root, &config, "src/lib.rs", 2, kind)
                .map_err(|error| format!("{error:?}"))?;
            let seams = analysis::file_seams_without_evidence_at_with_config(
                &root,
                &config,
                Path::new("src/lib.rs"),
            )?;
            let seam = seams
                .iter()
                .find(|seam| seam.id().as_str() == resolution.seam_id)
                .ok_or("resolved seam is in the file")?;
            Ok((seam.kind().as_str().to_string(), resolution.outcome.is_ok()))
        };
        let plain = kind_of(None);
        let error_path = kind_of(Some("error_path"));
        let predicate = kind_of(Some("predicate"));
        let _ = std::fs::remove_dir_all(&root);
        let boundary = SeamKind::PredicateBoundary.as_str().to_string();
        let variant = SeamKind::ErrorVariant.as_str().to_string();
        assert_eq!(plain?, (boundary.clone(), true));
        assert_eq!(error_path?, (variant, true));
        assert_eq!(predicate?, (boundary, true));
        Ok(())
    }

    #[test]
    fn scoped_file_names_only_a_file_under_the_root() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-stub-scoped-file-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("crates/a/src")).map_err(|error| error.to_string())?;
        std::fs::write(root.join("crates/a/src/lib.rs"), "").map_err(|error| error.to_string())?;
        let exact = scoped_file(&root, "crates/a/src/lib.rs");
        let dotted = scoped_file(&root, "./crates/a/src/lib.rs");
        let windows = scoped_file(&root, "crates\\a\\src\\lib.rs");
        let absolute = scoped_file(&root, &root.join("crates/a/src/lib.rs").to_string_lossy());
        // A suffix is not a file under the root: it keeps the repo-wide
        // lookup and its more-than-one-file refusal.
        let suffix = scoped_file(&root, "src/lib.rs");
        let directory = scoped_file(&root, "crates/a/src");
        // A path that leaves the root, by `..` or a symlink, is not under it.
        let outside = root.with_extension("outside.rs");
        std::fs::write(&outside, "").map_err(|error| error.to_string())?;
        let outside_name = outside
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let dotdot = scoped_file(&root, &format!("crates/../../{outside_name}"));
        #[cfg(unix)]
        let linked = {
            std::os::unix::fs::symlink(&outside, root.join("crates/a/src/link.rs"))
                .map_err(|error| error.to_string())?;
            scoped_file(&root, "crates/a/src/link.rs")
        };
        let _ = std::fs::remove_file(&outside);
        let _ = std::fs::remove_dir_all(&root);
        let expected = Some(PathBuf::from("crates/a/src/lib.rs"));
        assert_eq!(exact, expected);
        assert_eq!(dotted, expected);
        assert_eq!(windows, expected);
        assert_eq!(absolute, expected);
        assert_eq!(suffix, None);
        assert_eq!(directory, None);
        assert_eq!(dotdot, None);
        #[cfg(unix)]
        assert_eq!(linked, None);
        Ok(())
    }

    #[test]
    fn line_of_offset_is_one_based() {
        assert_eq!(line_of_offset("a\nb\nc", 0), 1);
        assert_eq!(line_of_offset("a\nb\nc", 2), 2);
        assert_eq!(line_of_offset("a\nb\nc", 4), 3);
    }

    #[test]
    fn classified_candidates_omit_strongly_gripped_seams() {
        let classified = vec![
            ungripped("src/lib.rs", 2),
            classified_at("src/lib.rs", 4, SeamGripClass::StronglyGripped),
        ];
        let candidates = classified_candidates(&classified);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].seam().display_line(), 2);
    }

    /// #7290: `--at --kind` selects from a file parse, so a strongly-gripped
    /// seam still yields a stub. The use case must still look up the
    /// inventory class so the CLI can disclose it.
    #[test]
    fn at_kind_stub_discloses_a_strongly_gripped_inventory_class() -> Result<(), String> {
        const SOURCE: &str = "pub fn clamp_to(x: i32) -> i32 {\n    if x >= 100 { 100 } else { x }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn at_boundary_clamps() {\n        assert_eq!(clamp_to(100), 100);\n        assert_eq!(clamp_to(99), 99);\n        assert_eq!(clamp_to(500), 100);\n    }\n}\n";
        let root = scratch_stub_crate("strongly-gripped", SOURCE)?;
        let config = RiprConfig::default();
        let inventory = analysis::inventory_diff_scoped_classified_seams_at_with_config(
            &root,
            &config,
            &[PathBuf::from("src/lib.rs")],
            &[],
        )
        .map_err(|error| error.to_string())?;
        let fixture = inventory
            .classified
            .iter()
            .find(|entry| entry.seam.display_line() == 2)
            .ok_or_else(|| {
                format!(
                    "fixture must classify the comparison on line 2: {:?}",
                    inventory
                        .classified
                        .iter()
                        .map(|entry| (entry.seam.display_line(), entry.class.as_str()))
                        .collect::<Vec<_>>()
                )
            })?;
        assert_eq!(
            fixture.class,
            SeamGripClass::StronglyGripped,
            "fixture must already be strongly gripped before the stub lookup"
        );
        let selector = TestStubSelector::parse_at("src/lib.rs:2")?.with_kind("predicate")?;
        let resolution =
            resolve_test_stub(&root, &config, &selector).map_err(|error| format!("{error:?}"))?;
        let outcome_ok = resolution.outcome.is_ok();
        let grip = resolution.grip_class;
        let _ = std::fs::remove_dir_all(&root);
        assert!(outcome_ok, "stub as scaffold stays ready");
        assert_eq!(grip, Some(SeamGripClass::StronglyGripped));
        Ok(())
    }

    /// #6298: `--at` on a line that holds no gap stubs the nearest gap in the
    /// function, and the resolution says so; a line that holds a gap says
    /// nothing, so the note never fires on an exact match.
    #[test]
    fn at_a_line_without_a_gap_names_the_nearest_gap_it_stubbed() -> Result<(), String> {
        const SOURCE: &str = "pub struct Size(pub u64);
impl Size {
    pub fn whole(&self, unit: u64) -> Option<u64> {
        if unit == 0 || self.0 % unit != 0 {
            return None;
        }
        Some(self.0 / unit)
    }
}
";
        let root = scratch_stub_crate("at-note", SOURCE)?;
        let config = RiprConfig::default();
        let resolve = |line: usize, family: &str| {
            let selector =
                TestStubSelector::parse_at(&format!("src/lib.rs:{line}"))?.with_kind(family)?;
            resolve_test_stub(&root, &config, &selector).map_err(|error| format!("{error:?}"))
        };
        let exact = resolve(7, "return_value")?;
        let off = resolve(8, "return_value")?;
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(exact.at_note, None, "an exact line match needs no note");
        assert!(
            off.outcome.is_ok(),
            "the nearest gap is still stubbed: {:?}",
            off.outcome.err()
        );
        let note = off.at_note.ok_or("a substituted line must carry a note")?;
        assert!(
            note.starts_with("note: src/lib.rs:8 has no stubbable gap")
                && note.ends_with(", src/lib.rs:7"),
            "{note}"
        );
        Ok(())
    }

    /// A genuine open gap still yields a ready stub, and its inventory
    /// class is not the strongly-gripped omission case.
    #[test]
    fn at_kind_stub_on_an_open_gap_is_not_strongly_gripped() -> Result<(), String> {
        const SOURCE: &str =
            "pub fn clamp_to(x: i32) -> i32 {\n    if x >= 100 { 100 } else { x }\n}\n";
        let root = scratch_stub_crate("open-gap", SOURCE)?;
        let config = RiprConfig::default();
        let selector = TestStubSelector::parse_at("src/lib.rs:2")?.with_kind("predicate")?;
        let resolution =
            resolve_test_stub(&root, &config, &selector).map_err(|error| format!("{error:?}"))?;
        let outcome_ok = resolution.outcome.is_ok();
        let grip = resolution.grip_class;
        let _ = std::fs::remove_dir_all(&root);
        assert!(outcome_ok, "an open gap still yields a stub: {grip:?}");
        assert_ne!(grip, Some(SeamGripClass::StronglyGripped));
        Ok(())
    }
}
