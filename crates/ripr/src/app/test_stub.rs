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
use crate::analysis::test_stub::{
    RustTestStub, TestStubPlacement, TestStubRefusal, rust_test_stub,
    rust_test_stub_for_classified_seam,
};
use crate::analysis::{ClassifiedSeam, RepoSeam};
use crate::app::CheckOutput;
use crate::config::RiprConfig;
use crate::domain::{ExposureClass, Finding, ProbeFamily};
use std::path::{Path, PathBuf};

/// Which gap the caller means.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TestStubSelector {
    SeamId(String),
    /// `file:line`, as `ripr check` prints a finding location.
    At {
        file: String,
        line: usize,
    },
}

impl TestStubSelector {
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
}

#[derive(Debug)]
pub(crate) enum TestStubError {
    /// The selector named no gap ripr can stub; the message says where to
    /// look instead.
    NotFound(String),
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
        TestStubSelector::At { file, line } => resolve_at_location(root, config, file, *line),
    }
}

/// The one `--at FILE:LINE` resolver, shared by `ripr agent stub --at` and
/// the route `ripr check` prints (#5471).
///
/// When `file` names a file under `root`, the candidates are that file's
/// seams from a parse of the file alone: no workspace index, test evidence,
/// or classification, so the location `check` reported is not re-judged
/// by a second classifier (which is how the route and the command
/// disagreed). `check` already decided the location is a gap; the stub
/// needs only the seam's shape and the owner source, and is placed inline
/// (an integration-file placement needs classified evidence, so it stays on
/// `--seam-id`).
///
/// A suffix that names no file under `root` (`src/lib.rs` for
/// `crates/a/src/lib.rs`) keeps the repo-wide classified lookup so its
/// more-than-one-file refusal still applies; `check` never prints that form.
fn resolve_at_location(
    root: &Path,
    config: &RiprConfig,
    file: &str,
    line: usize,
) -> Result<TestStubResolution, TestStubError> {
    match scoped_file(root, file) {
        Some(relative) => {
            let seams =
                analysis::file_seams_without_evidence_at_with_config(root, config, &relative)
                    .map_err(TestStubError::Operational)?;
            let candidates = seams.iter().map(AtCandidate::Shape).collect::<Vec<_>>();
            resolve_at(root, candidates, file, line)
        }
        None => {
            let (classified, _) = analysis::inventory_classified_seams_at_with_config(root, config)
                .map_err(TestStubError::Operational)?;
            resolve_at(root, classified_candidates(&classified), file, line)
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
/// `root`; `None` otherwise.
fn scoped_file(root: &Path, file: &str) -> Option<PathBuf> {
    let normalized = file.replace('\\', "/");
    let path = Path::new(normalized.trim_start_matches("./"));
    let relative = if path.is_absolute() {
        path.strip_prefix(root).ok()?.to_path_buf()
    } else {
        path.to_path_buf()
    };
    (!relative.as_os_str().is_empty() && root.join(&relative).is_file()).then_some(relative)
}

/// What `ripr check` prints under its selected finding for the test-stub
/// route (#5471).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StubRouteDecision {
    /// The shared resolver produces a stub here: print
    /// `ripr agent stub --at FILE:LINE`.
    Stub { file: String, line: usize },
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

/// The `FILE:LINE` a stub route names for `finding`, relative to `root`
/// because `--at` resolves against `--root`. `None` when the finding is not
/// a non-exposed Rust gap in a family the stub producer handles.
pub(crate) fn stub_route_location(finding: &Finding, root: &Path) -> Option<(String, usize)> {
    let location = &finding.probe.location.file;
    let eligible = finding.class != ExposureClass::Exposed
        && matches!(
            finding.probe.family,
            ProbeFamily::Predicate
                | ProbeFamily::ReturnValue
                | ProbeFamily::ErrorPath
                | ProbeFamily::MatchArm
        )
        && location.extension().and_then(|ext| ext.to_str()) == Some("rs");
    if !eligible {
        return None;
    }
    let relative = location.strip_prefix(root).unwrap_or(location);
    let file = crate::output::path::display_path(relative);
    Some((
        file.trim_start_matches("./").to_string(),
        finding.probe.location.line,
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
    let (file, line) = stub_route_location(finding, root)?;
    let resolution = resolve_at_location(root, config, &file, line).ok()?;
    let decision = match resolution.outcome {
        Ok(_) => StubRouteDecision::Stub { file, line },
        Err(refusal) => StubRouteDecision::Refused {
            reason: refusal.reason(),
        },
    };
    Some(StubRoute {
        finding_id: finding.id.clone(),
        decision,
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
    stub_route_location(finding, root)?;
    let config = crate::config::load_for_root(root).ok()?;
    stub_route_for_finding(root, &config, finding)
}

/// A `ripr check` finding line is the changed line, which is not always a
/// seam's own line. Candidates are the seams on that line, then the seams in
/// the same function nearest first; the first one that yields a stub wins,
/// and when none does, the nearest candidate's refusal names why.
fn resolve_at(
    root: &Path,
    candidates: Vec<AtCandidate<'_>>,
    file: &str,
    line: usize,
) -> Result<TestStubResolution, TestStubError> {
    let noun = match candidates.first() {
        Some(AtCandidate::Classified(_)) => "reported gap",
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
        return Err(TestStubError::NotFound(format!(
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
    candidates.sort_by_key(|candidate| candidate.seam().display_line().abs_diff(line));
    let mut first_refusal = None;
    for candidate in candidates {
        let resolution = candidate.resolution(source.clone());
        if resolution.outcome.is_ok() {
            return Ok(resolution);
        }
        first_refusal.get_or_insert(resolution);
    }
    if let Some(resolution) = first_refusal {
        return Ok(resolution);
    }
    let mut nearby = in_file;
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
    Err(TestStubError::NotFound(format!(
        "no {noun} is in the function at {file}:{line}; nearest: {}",
        listed.join(", ")
    )))
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
    }
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
    fn at_selector_parses_file_and_line_and_refuses_junk() {
        assert_eq!(
            TestStubSelector::parse_at("src/lib.rs:12"),
            Ok(TestStubSelector::At {
                file: "src/lib.rs".to_string(),
                line: 12
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

    fn ungripped(file: &str, line: usize) -> ClassifiedSeam {
        use crate::analysis::seams::{
            ExpectedSink, RepoSeam, RequiredDiscriminator, SeamGripClass, SeamKind,
        };
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
                new_test_target: None,
            },
            seam,
            class: SeamGripClass::Ungripped,
        }
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
        );
        assert!(
            matches!(&result, Err(TestStubError::NotFound(message))
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
        let _ = std::fs::remove_dir_all(&root);
        let expected = Some(PathBuf::from("crates/a/src/lib.rs"));
        assert_eq!(exact, expected);
        assert_eq!(dotted, expected);
        assert_eq!(windows, expected);
        assert_eq!(absolute, expected);
        assert_eq!(suffix, None);
        assert_eq!(directory, None);
        Ok(())
    }

    #[test]
    fn line_of_offset_is_one_based() {
        assert_eq!(line_of_offset("a\nb\nc", 0), 1);
        assert_eq!(line_of_offset("a\nb\nc", 2), 2);
        assert_eq!(line_of_offset("a\nb\nc", 4), 3);
    }
}
