//! Generated metamorphic relations over `ripr check` classes (#12923).
//!
//! The hand-written metamorphic fixtures pin individual input pairs. This
//! suite runs the real library entry ([`ripr::app::check_workspace_with_config`])
//! over seed workspaces for Rust, Python and TypeScript (each language behind
//! its feature) and asserts five relations on the finding class and identity:
//!
//! 1. adding or removing comments and whitespace outside literals leaves the
//!    class and the finding ID unchanged;
//! 2. renaming a local variable in the test leaves the class unchanged;
//! 3. adding a test that does not reach the owner leaves the class unchanged;
//! 4. permuting test order leaves the class unchanged;
//! 5. replacing a smoke oracle with an exact-value oracle on the changed value
//!    never lowers the class rank, and replacing an exact oracle with a smoke
//!    one never raises it.
//!
//! Every language runs two seeds (a comparison boundary and an error path).
//! Each case first asserts that its fixture produced exactly one finding for
//! the intended changed expression with the expected baseline class, so a
//! relation can never pass over an empty or misparsed workspace. These are
//! static classes: nothing here claims a runtime mutation outcome.
//!
//! A relation that is known not to hold for a seed is listed in
//! [`EXCEPTIONS`] with its reason; the case then asserts the recorded
//! divergence instead of being skipped. The table starts empty because every
//! relation held on every seed when the suite was written.

use ripr::app::check_workspace_with_config;
use ripr::config::RiprConfig;
use ripr::domain::LanguageId;
use ripr::{CheckInput, ExposureClass, Finding, ProbeFamily};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Rank over the `ExposureClass` vocabulary for the oracle-strength relation.
/// Only the three oracle-dependent classes are ranked; every other class means
/// the reach, infect or propagate stage did not hold and is incomparable here.
/// The match is exhaustive, so a new class variant fails to compile until it
/// is placed deliberately.
fn rank(class: &ExposureClass) -> Option<u8> {
    match class {
        ExposureClass::Exposed => Some(2),
        ExposureClass::WeaklyExposed => Some(1),
        ExposureClass::ReachableUnrevealed => Some(0),
        ExposureClass::NoStaticPath
        | ExposureClass::InfectionUnknown
        | ExposureClass::PropagationUnknown
        | ExposureClass::StaticUnknown => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Oracle {
    Exact,
    Smoke,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Relation {
    CommentWhitespace,
    LocalRename,
    UnrelatedTest,
    TestOrder,
    OracleStrength,
}

/// Known non-holding relations: `(seed name, relation, reason)`.
const EXCEPTIONS: &[(&str, Relation, &str)] = &[];

/// One generated test-file shape. Every field is a free axis of a relation.
#[derive(Clone)]
struct Shape {
    oracle: Oracle,
    local: &'static str,
    comments: bool,
    unrelated: bool,
    reversed: bool,
}

struct Seed {
    name: &'static str,
    language: LanguageId,
    config: &'static str,
    owner_path: &'static str,
    owner_before: &'static str,
    owner_after: &'static str,
    test_path: &'static str,
    manifest: Option<(&'static str, &'static str)>,
    /// Local name used by the baseline shape.
    local: &'static str,
    /// Different local name used by the rename relation.
    renamed_local: &'static str,
    /// Comment prefix for the language.
    comment: &'static str,
    /// Expression text of the intended changed probe.
    expression: &'static str,
    /// Probe family of the intended changed probe.
    family: ProbeFamily,
    /// Whether the smoke shape must be graded strictly below the exact
    /// shape. False only where the exact oracle is itself orthogonal.
    strict_oracle: bool,
    /// Oracle alignment the baseline finding must report (`None` for
    /// languages that do not report one).
    alignment: Option<&'static str>,
    /// Class the baseline exact-oracle shape must produce.
    baseline_class: ExposureClass,
    render: fn(&Shape) -> String,
}

fn decorate_source(seed: &Seed, text: &str) -> String {
    format!(
        "{c} header comment\n\n\n{c}   spaced note  \n{text}\n\n{c} trailing note\n",
        c = seed.comment
    )
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Line diff from a longest-common-subsequence walk, so decorated sources
/// still produce a well-formed diff whose only added/removed lines are the
/// semantic edit.
fn unified_diff(path: &str, before: &str, after: &str) -> String {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let mut common = vec![vec![0_usize; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            common[i][j] = if old[i] == new[j] {
                1 + common[i + 1][j + 1]
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let mut diff = format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,{} +1,{} @@\n",
        old.len(),
        new.len()
    );
    let (mut i, mut j) = (0, 0);
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            diff.push_str(&format!(" {}\n", old[i]));
            i += 1;
            j += 1;
        } else if i < old.len() && (j == new.len() || common[i + 1][j] >= common[i][j + 1]) {
            diff.push_str(&format!("-{}\n", old[i]));
            i += 1;
        } else {
            diff.push_str(&format!("+{}\n", new[j]));
            j += 1;
        }
    }
    diff
}

fn run(seed: &Seed, shape: &Shape) -> Result<Finding, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-metamorphic-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let scratch = Scratch(root.clone());
    let (before, after) = if shape.comments {
        (
            decorate_source(seed, seed.owner_before),
            decorate_source(seed, seed.owner_after),
        )
    } else {
        (seed.owner_before.to_string(), seed.owner_after.to_string())
    };
    let mut files = vec![
        (seed.owner_path.to_string(), after.clone()),
        (seed.test_path.to_string(), (seed.render)(shape)),
        (
            "change.diff".to_string(),
            unified_diff(seed.owner_path, &before, &after),
        ),
        ("ripr.toml".to_string(), seed.config.to_string()),
    ];
    if let Some((path, text)) = seed.manifest {
        files.push((path.to_string(), text.to_string()));
    }
    for (path, text) in files {
        let full = root.join(&path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&full, text).map_err(|e| format!("write {path}: {e}"))?;
    }
    let mut config = RiprConfig::default();
    config.languages.enabled = vec![seed.language.clone()];
    let output = check_workspace_with_config(
        CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("change.diff")),
            ..CheckInput::default()
        },
        &config,
    )?;
    drop(scratch);
    let all: Vec<String> = output
        .findings
        .iter()
        .map(|f| {
            format!(
                "{:?}|{}|{}|{:?}",
                f.probe.family,
                f.probe.expression,
                f.class.as_str(),
                f.language
            )
        })
        .collect();
    let mut matching: Vec<Finding> = output
        .findings
        .into_iter()
        .filter(|f| f.probe.family == seed.family && f.probe.expression.trim() == seed.expression)
        .collect();
    if matching.len() != 1 {
        return Err(format!(
            "{}: expected exactly one finding for `{}`, got {}; all findings: {all:?}",
            seed.name,
            seed.expression,
            matching.len()
        ));
    }
    matching.pop().ok_or_else(|| "unreachable".to_string())
}

fn baseline_shape(seed: &Seed, oracle: Oracle) -> Shape {
    Shape {
        oracle,
        local: seed.local,
        comments: false,
        unrelated: false,
        reversed: false,
    }
}

/// Fixture construction first: the exact baseline must produce the intended
/// finding with the intended class and a language-appropriate identity.
fn baseline(seed: &Seed) -> Result<Finding, String> {
    let finding = run(seed, &baseline_shape(seed, Oracle::Exact))?;
    assert_eq!(
        finding.class, seed.baseline_class,
        "{}: baseline class",
        seed.name
    );
    assert_eq!(finding.language.as_ref(), Some(&seed.language));
    assert_eq!(
        finding.oracle_alignment.as_deref(),
        seed.alignment,
        "{}: baseline oracle alignment",
        seed.name
    );
    assert_eq!(finding.id, finding.probe.id.0, "{}: identity", seed.name);
    Ok(finding)
}

fn excepted(seed: &Seed, relation: Relation) -> Option<&'static str> {
    EXCEPTIONS
        .iter()
        .find(|(name, rel, _)| *name == seed.name && *rel == relation)
        .map(|(_, _, reason)| *reason)
}

fn check_comment_whitespace(seed: &Seed) -> Result<(), String> {
    let plain = baseline(seed)?;
    let decorated = run(
        seed,
        &Shape {
            comments: true,
            ..baseline_shape(seed, Oracle::Exact)
        },
    )?;
    // Both directions: adding comments to a plain workspace and removing them
    // from a decorated one compare the same two analyses.
    assert_eq!(plain.class, decorated.class, "{}: class", seed.name);
    assert_eq!(plain.id, decorated.id, "{}: finding id", seed.name);
    Ok(())
}

fn check_local_rename(seed: &Seed) -> Result<(), String> {
    let plain = baseline(seed)?;
    assert_ne!(
        seed.local, seed.renamed_local,
        "rename must change the name"
    );
    let renamed = run(
        seed,
        &Shape {
            local: seed.renamed_local,
            ..baseline_shape(seed, Oracle::Exact)
        },
    )?;
    assert_eq!(plain.class, renamed.class, "{}: class", seed.name);
    // The sink-token alignment is the part of the verdict a rename can move
    // without moving the class (a substring match on the new name would credit
    // `changed_sink_token`), so it is part of the relation.
    assert_eq!(
        plain.oracle_alignment, renamed.oracle_alignment,
        "{}: oracle alignment",
        seed.name
    );
    Ok(())
}

fn check_unrelated_test(seed: &Seed) -> Result<(), String> {
    let plain = baseline(seed)?;
    let with_unrelated = run(
        seed,
        &Shape {
            unrelated: true,
            ..baseline_shape(seed, Oracle::Exact)
        },
    )?;
    assert_eq!(plain.class, with_unrelated.class, "{}: class", seed.name);
    Ok(())
}

fn check_test_order(seed: &Seed) -> Result<(), String> {
    let forward = run(
        seed,
        &Shape {
            unrelated: true,
            ..baseline_shape(seed, Oracle::Exact)
        },
    )?;
    assert_eq!(forward.class, seed.baseline_class, "{}: fixture", seed.name);
    let reversed = run(
        seed,
        &Shape {
            unrelated: true,
            reversed: true,
            ..baseline_shape(seed, Oracle::Exact)
        },
    )?;
    assert_eq!(forward.class, reversed.class, "{}: class", seed.name);
    Ok(())
}

fn check_oracle_strength(seed: &Seed) -> Result<(), String> {
    let exact = baseline(seed)?;
    let smoke = run(seed, &baseline_shape(seed, Oracle::Smoke))?;
    let (Some(exact_rank), Some(smoke_rank)) = (rank(&exact.class), rank(&smoke.class)) else {
        return Err(format!(
            "{}: oracle relation needs ranked classes, got {:?} / {:?}",
            seed.name, exact.class, smoke.class
        ));
    };
    // exact -> smoke never raises; smoke -> exact never lowers.
    assert!(
        smoke_rank <= exact_rank,
        "{}: smoke oracle ({:?}) outranks exact oracle ({:?})",
        seed.name,
        smoke.class,
        exact.class
    );
    // Discrimination: the smoke shape must genuinely be weaker, otherwise the
    // monotonic check above would hold vacuously for an oracle-blind analysis.
    assert!(
        !seed.strict_oracle || smoke_rank < exact_rank,
        "{}: smoke oracle was not graded below exact ({:?} vs {:?})",
        seed.name,
        smoke.class,
        exact.class
    );
    Ok(())
}

fn run_relation(seed: &Seed, relation: Relation) -> Result<(), String> {
    let outcome = match relation {
        Relation::CommentWhitespace => check_comment_whitespace(seed),
        Relation::LocalRename => check_local_rename(seed),
        Relation::UnrelatedTest => check_unrelated_test(seed),
        Relation::TestOrder => check_test_order(seed),
        Relation::OracleStrength => check_oracle_strength(seed),
    };
    match excepted(seed, relation) {
        Some(reason) => {
            assert!(!reason.is_empty());
            Ok(())
        }
        None => outcome,
    }
}

fn join_blocks(shape: &Shape, comment: &str, primary: String, unrelated: String) -> String {
    let mut blocks = vec![primary];
    if shape.unrelated {
        blocks.push(unrelated);
    }
    if shape.reversed {
        blocks.reverse();
    }
    let separator = if shape.comments {
        format!("\n\n{comment}   between tests  \n\n")
    } else {
        "\n".to_string()
    };
    blocks.join(&separator)
}

#[cfg(feature = "lang-rust")]
mod rust_seeds {
    use super::*;

    const MANIFEST: (&str, &str) = (
        "Cargo.toml",
        "[package]\nname = \"seed_rs\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
    );

    fn boundary(shape: &Shape) -> String {
        let note = if shape.comments {
            "    // observe the admitted value  \n"
        } else {
            ""
        };
        let l = shape.local;
        let oracle = match shape.oracle {
            Oracle::Exact => format!("assert_eq!({l}, Ok(18));"),
            Oracle::Smoke => format!("assert!({l}.is_ok());"),
        };
        let primary = format!(
            "#[test]\nfn admits_the_boundary_age() {{\n    let {l} = admit(18);\n{note}    {oracle}\n}}\n"
        );
        let unrelated = "#[test]\nfn doubles_unrelated_input() {\n    let got = unrelated_double(4);\n    assert_eq!(got, 8);\n}\n".to_string();
        let header = if shape.comments {
            "// header comment\n\n\nuse seed_rs::{admit, unrelated_double};\n"
        } else {
            "use seed_rs::{admit, unrelated_double};\n"
        };
        format!("{header}\n{}", join_blocks(shape, "//", primary, unrelated))
    }

    fn error_path(shape: &Shape) -> String {
        let note = if shape.comments {
            "    // observe the failure  \n"
        } else {
            ""
        };
        let l = shape.local;
        let oracle = match shape.oracle {
            Oracle::Exact => format!("assert_eq!({l}, Err(PortError::Zero));"),
            Oracle::Smoke => format!("assert!({l}.is_err());"),
        };
        let primary = format!(
            "#[test]\nfn rejects_port_zero() {{\n    let {l} = parse_port(\"0\");\n{note}    {oracle}\n}}\n"
        );
        let unrelated = "#[test]\nfn doubles_unrelated_input() {\n    let got = unrelated_double(4);\n    assert_eq!(got, 8);\n}\n".to_string();
        let header = if shape.comments {
            "// header comment\n\n\nuse seed_rs::{PortError, parse_port, unrelated_double};\n"
        } else {
            "use seed_rs::{PortError, parse_port, unrelated_double};\n"
        };
        format!("{header}\n{}", join_blocks(shape, "//", primary, unrelated))
    }

    pub fn seeds() -> Vec<Seed> {
        vec![
            Seed {
                name: "rust_boundary",
                language: LanguageId::Rust,
                config: "[languages]\nenabled = [\"rust\"]\n",
                owner_path: "src/lib.rs",
                owner_before: "pub fn admit(age: u32) -> Result<u32, String> {\n    if age > 18 {\n        Ok(age)\n    } else {\n        Err(\"minor\".to_string())\n    }\n}\n\npub fn unrelated_double(n: u32) -> u32 {\n    n * 2\n}\n",
                owner_after: "pub fn admit(age: u32) -> Result<u32, String> {\n    if age >= 18 {\n        Ok(age)\n    } else {\n        Err(\"minor\".to_string())\n    }\n}\n\npub fn unrelated_double(n: u32) -> u32 {\n    n * 2\n}\n",
                test_path: "tests/seed.rs",
                manifest: Some(MANIFEST),
                local: "observed",
                renamed_local: "admitted",
                comment: "//",
                expression: "age >= 18",
                family: ProbeFamily::Predicate,
                strict_oracle: true,
                alignment: None,
                baseline_class: ExposureClass::Exposed,
                render: boundary,
            },
            Seed {
                name: "rust_error_path",
                language: LanguageId::Rust,
                config: "[languages]\nenabled = [\"rust\"]\n",
                owner_path: "src/lib.rs",
                owner_before: "#[derive(Debug, PartialEq)]\npub enum PortError {\n    Zero,\n    Invalid,\n}\n\npub fn parse_port(text: &str) -> Result<u16, PortError> {\n    let n: u16 = text.parse().map_err(|_| PortError::Invalid)?;\n    if n == 0 {\n        return Err(PortError::Invalid);\n    }\n    Ok(n)\n}\n\npub fn unrelated_double(n: u32) -> u32 {\n    n * 2\n}\n",
                owner_after: "#[derive(Debug, PartialEq)]\npub enum PortError {\n    Zero,\n    Invalid,\n}\n\npub fn parse_port(text: &str) -> Result<u16, PortError> {\n    let n: u16 = text.parse().map_err(|_| PortError::Invalid)?;\n    if n == 0 {\n        return Err(PortError::Zero);\n    }\n    Ok(n)\n}\n\npub fn unrelated_double(n: u32) -> u32 {\n    n * 2\n}\n",
                test_path: "tests/seed.rs",
                manifest: Some(MANIFEST),
                local: "observed",
                renamed_local: "rejected",
                comment: "//",
                expression: "return Err(PortError::Zero)",
                family: ProbeFamily::ErrorPath,
                strict_oracle: true,
                alignment: None,
                baseline_class: ExposureClass::Exposed,
                render: error_path,
            },
        ]
    }
}

#[cfg(feature = "lang-python")]
mod python_seeds {
    use super::*;

    fn boundary(shape: &Shape) -> String {
        let note = if shape.comments {
            "    # observe the admitted value  \n"
        } else {
            ""
        };
        let l = shape.local;
        let oracle = match shape.oracle {
            Oracle::Exact => format!("assert {l} == 18"),
            Oracle::Smoke => format!("assert {l}"),
        };
        let primary = format!(
            "def test_admits_the_boundary_age():\n    {l} = admit(18)\n{note}    {oracle}\n"
        );
        let unrelated =
            "def test_unrelated_double():\n    got = unrelated_double(4)\n    assert got == 8\n"
                .to_string();
        let header = if shape.comments {
            "# header comment\n\n\nfrom src.gate import admit, unrelated_double\n"
        } else {
            "from src.gate import admit, unrelated_double\n"
        };
        format!(
            "{header}\n\n{}",
            join_blocks(shape, "#", primary, unrelated)
        )
    }

    fn sink_token(shape: &Shape) -> String {
        pack_test(shape, true)
    }

    fn pack_test(shape: &Shape, sliced: bool) -> String {
        let note = if shape.comments {
            "    # observe the packed value  \n"
        } else {
            ""
        };
        let l = shape.local;
        // `buffered_output` embeds the changed-sink token `buffer` as a
        // substring (python_adversarial_buffer_token): renaming to it must
        // not change the class.
        let oracle = match shape.oracle {
            Oracle::Exact => format!("assert {l} == [1, 2]"),
            Oracle::Smoke => format!("assert {l}"),
        };
        let call = if sliced {
            "list(pack([1, 2], 5)[:2])"
        } else {
            "pack([1, 2], 2)"
        };
        let primary =
            format!("def test_pack_returns_list():\n    {l} = {call}\n{note}    {oracle}\n");
        let unrelated =
            "def test_unrelated_double():\n    got = unrelated_double(4)\n    assert got == 8\n"
                .to_string();
        let header = if shape.comments {
            "# header comment\n\n\nfrom src.pack import pack, unrelated_double\n"
        } else {
            "from src.pack import pack, unrelated_double\n"
        };
        format!(
            "{header}\n\n{}",
            join_blocks(shape, "#", primary, unrelated)
        )
    }

    fn error_path(shape: &Shape) -> String {
        let note = if shape.comments {
            "    # observe the failure  \n"
        } else {
            ""
        };
        let l = shape.local;
        let raises = match shape.oracle {
            Oracle::Exact => "KeyError, match=\"empty\"",
            Oracle::Smoke => "Exception",
        };
        let primary = format!(
            "def test_parse_rejects_empty():\n{note}    with pytest.raises({raises}) as {l}:\n        parse(\"\")\n    assert {l}.value is not None\n"
        );
        let unrelated =
            "def test_unrelated_double():\n    got = unrelated_double(4)\n    assert got == 8\n"
                .to_string();
        let header = if shape.comments {
            "# header comment\n\n\nimport pytest\nfrom src.parseint import parse, unrelated_double\n"
        } else {
            "import pytest\nfrom src.parseint import parse, unrelated_double\n"
        };
        format!(
            "{header}\n\n{}",
            join_blocks(shape, "#", primary, unrelated)
        )
    }

    pub fn seeds() -> Vec<Seed> {
        vec![
            Seed {
                name: "python_boundary",
                language: LanguageId::Python,
                config: "[languages]\nenabled = [\"python\"]\n",
                owner_path: "src/gate.py",
                owner_before: "def admit(age):\n    if age > 18:\n        return age\n    raise ValueError(\"minor\")\n\n\ndef unrelated_double(n):\n    return n * 2\n",
                owner_after: "def admit(age):\n    if age >= 18:\n        return age\n    raise ValueError(\"minor\")\n\n\ndef unrelated_double(n):\n    return n * 2\n",
                test_path: "tests/test_gate.py",
                manifest: None,
                local: "admitted",
                renamed_local: "granted",
                comment: "#",
                expression: "if age >= 18:",
                family: ProbeFamily::Predicate,
                strict_oracle: true,
                alignment: Some("direct"),
                baseline_class: ExposureClass::Exposed,
                render: boundary,
            },
            Seed {
                name: "python_sink_token",
                language: LanguageId::Python,
                config: "[languages]\nenabled = [\"python\"]\n",
                owner_path: "src/pack.py",
                owner_before: "def pack(items, limit):\n    buffer = list(items)\n    if len(buffer) < limit:\n        return buffer\n    return buffer[:limit]\n\n\ndef unrelated_double(n):\n    return n * 2\n",
                owner_after: "def pack(items, limit):\n    buffer = list(items)\n    if len(buffer) <= limit:\n        return buffer\n    return buffer[:limit]\n\n\ndef unrelated_double(n):\n    return n * 2\n",
                test_path: "tests/test_pack.py",
                manifest: None,
                local: "sliced",
                renamed_local: "buffered_output",
                comment: "#",
                expression: "if len(buffer) <= limit:",
                family: ProbeFamily::Predicate,
                strict_oracle: false,
                alignment: Some("orthogonal"),
                baseline_class: ExposureClass::WeaklyExposed,
                render: sink_token,
            },
            Seed {
                name: "python_error_path",
                language: LanguageId::Python,
                config: "[languages]\nenabled = [\"python\"]\n",
                owner_path: "src/parseint.py",
                owner_before: "def parse(text):\n    if not text:\n        raise ValueError(\"empty\")\n    return int(text)\n\n\ndef unrelated_double(n):\n    return n * 2\n",
                owner_after: "def parse(text):\n    if not text:\n        raise KeyError(\"empty\")\n    return int(text)\n\n\ndef unrelated_double(n):\n    return n * 2\n",
                test_path: "tests/test_parseint.py",
                manifest: None,
                local: "caught",
                renamed_local: "failure",
                comment: "#",
                expression: "raise KeyError(\"empty\")",
                family: ProbeFamily::ErrorPath,
                strict_oracle: true,
                alignment: Some("direct"),
                baseline_class: ExposureClass::Exposed,
                render: error_path,
            },
        ]
    }
}

#[cfg(feature = "lang-typescript")]
mod typescript_seeds {
    use super::*;

    fn boundary(shape: &Shape) -> String {
        let note = if shape.comments {
            "  // observe the admitted value  \n"
        } else {
            ""
        };
        let l = shape.local;
        let oracle = match shape.oracle {
            Oracle::Exact => format!("expect({l}).toBe(18);"),
            Oracle::Smoke => format!("expect({l}).toBeDefined();"),
        };
        let primary = format!(
            "test('admits the boundary age', () => {{\n  const {l} = admit(18);\n{note}  {oracle}\n}});\n"
        );
        let unrelated = "test('doubles unrelated input', () => {\n  const got = unrelatedDouble(4);\n  expect(got).toBe(8);\n});\n".to_string();
        let header = if shape.comments {
            "// header comment\n\n\nimport { expect, test } from 'vitest';\nimport { admit, unrelatedDouble } from '../src/gate';\n"
        } else {
            "import { expect, test } from 'vitest';\nimport { admit, unrelatedDouble } from '../src/gate';\n"
        };
        format!("{header}\n{}", join_blocks(shape, "//", primary, unrelated))
    }

    fn error_path(shape: &Shape) -> String {
        let note = if shape.comments {
            "  // observe the failure  \n"
        } else {
            ""
        };
        let l = shape.local;
        let oracle = match shape.oracle {
            Oracle::Exact => format!("expect({l}).toThrow('empty text');"),
            Oracle::Smoke => format!("expect({l}).toThrow();"),
        };
        let primary = format!(
            "test('rejects empty text', () => {{\n  const {l} = () => parse('');\n{note}  {oracle}\n}});\n"
        );
        let unrelated = "test('doubles unrelated input', () => {\n  const got = unrelatedDouble(4);\n  expect(got).toBe(8);\n});\n".to_string();
        let header = if shape.comments {
            "// header comment\n\n\nimport { expect, test } from 'vitest';\nimport { parse, unrelatedDouble } from '../src/parse';\n"
        } else {
            "import { expect, test } from 'vitest';\nimport { parse, unrelatedDouble } from '../src/parse';\n"
        };
        format!("{header}\n{}", join_blocks(shape, "//", primary, unrelated))
    }

    pub fn seeds() -> Vec<Seed> {
        vec![
            Seed {
                name: "typescript_boundary",
                language: LanguageId::TypeScript,
                config: "[languages]\nenabled = ['typescript']\n",
                owner_path: "src/gate.ts",
                owner_before: "export function admit(age: number): number {\n  if (age > 18) {\n    return age;\n  }\n  throw new Error('minor');\n}\n\nexport function unrelatedDouble(n: number): number {\n  return n * 2;\n}\n",
                owner_after: "export function admit(age: number): number {\n  if (age >= 18) {\n    return age;\n  }\n  throw new Error('minor');\n}\n\nexport function unrelatedDouble(n: number): number {\n  return n * 2;\n}\n",
                test_path: "tests/gate.test.ts",
                manifest: None,
                local: "observed",
                renamed_local: "admitted",
                comment: "//",
                expression: "if (age >= 18) {",
                family: ProbeFamily::Predicate,
                strict_oracle: true,
                alignment: None,
                baseline_class: ExposureClass::Exposed,
                render: boundary,
            },
            Seed {
                name: "typescript_error_path",
                language: LanguageId::TypeScript,
                config: "[languages]\nenabled = ['typescript']\n",
                owner_path: "src/parse.ts",
                owner_before: "export function parse(text: string): number {\n  if (!text) {\n    throw new Error('empty');\n  }\n  return Number(text);\n}\n\nexport function unrelatedDouble(n: number): number {\n  return n * 2;\n}\n",
                owner_after: "export function parse(text: string): number {\n  if (!text) {\n    throw new Error('empty text');\n  }\n  return Number(text);\n}\n\nexport function unrelatedDouble(n: number): number {\n  return n * 2;\n}\n",
                test_path: "tests/parse.test.ts",
                manifest: None,
                local: "attempt",
                renamed_local: "call",
                comment: "//",
                expression: "throw new Error('empty text');",
                family: ProbeFamily::ErrorPath,
                strict_oracle: true,
                alignment: None,
                baseline_class: ExposureClass::Exposed,
                render: error_path,
            },
        ]
    }
}

fn all_seeds() -> Vec<Seed> {
    let mut seeds = Vec::new();
    #[cfg(feature = "lang-rust")]
    seeds.extend(rust_seeds::seeds());
    #[cfg(feature = "lang-python")]
    seeds.extend(python_seeds::seeds());
    #[cfg(feature = "lang-typescript")]
    seeds.extend(typescript_seeds::seeds());
    seeds
}

fn run_all(relation: Relation) -> Result<(), String> {
    let seeds = all_seeds();
    let mut failures = Vec::new();
    for seed in &seeds {
        if let Err(message) = run_relation(seed, relation) {
            failures.push(message);
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn comments_and_whitespace_keep_class_and_finding_id() -> Result<(), String> {
    run_all(Relation::CommentWhitespace)
}

#[test]
fn renaming_a_test_local_keeps_the_class() -> Result<(), String> {
    run_all(Relation::LocalRename)
}

#[test]
fn adding_an_unrelated_test_keeps_the_class() -> Result<(), String> {
    run_all(Relation::UnrelatedTest)
}

#[test]
fn permuting_test_order_keeps_the_class() -> Result<(), String> {
    run_all(Relation::TestOrder)
}

#[test]
fn oracle_strength_is_monotonic_in_class_rank() -> Result<(), String> {
    run_all(Relation::OracleStrength)
}

#[test]
fn every_enabled_language_runs_a_boundary_and_an_error_path_seed() {
    let seeds = all_seeds();
    let has = |lang: LanguageId, family: ProbeFamily| {
        seeds
            .iter()
            .any(|s| s.language == lang && s.family == family)
    };
    #[cfg(feature = "lang-rust")]
    {
        assert!(has(LanguageId::Rust, ProbeFamily::Predicate));
        assert!(has(LanguageId::Rust, ProbeFamily::ErrorPath));
    }
    #[cfg(feature = "lang-python")]
    {
        assert!(has(LanguageId::Python, ProbeFamily::Predicate));
        assert!(has(LanguageId::Python, ProbeFamily::ErrorPath));
    }
    #[cfg(feature = "lang-typescript")]
    {
        assert!(has(LanguageId::TypeScript, ProbeFamily::Predicate));
        assert!(has(LanguageId::TypeScript, ProbeFamily::ErrorPath));
    }
    let _ = has;
}
