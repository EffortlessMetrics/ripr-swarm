//! Same-file assertion macros called from tests (#9122).
//!
//! `assert_next_eq!(io, frame)` defined by a local `macro_rules!` hides its
//! assertion inside a token tree, so the test read as having no assertion.
//! This producer credits the call only from the macro's own transcriber text:
//!
//! - every arm of the unique, scoped, un-gated definition must itself show
//!   an assertion the ordinary extractor recognizes (`assert!`,
//!   `assert_eq!`, ...). A macro that only logs, or has one arm without such
//!   evidence, credits nothing. A hand-rolled `if bad { panic!() }` body
//!   shows no recognized assertion and stays the opaque custom assertion
//!   macro limit (#7089) rather than becoming an oracle here;
//! - the credit is the weakest arm and is never stronger than `weak`: the
//!   call-site arguments, repetitions and conditional arms are not
//!   evaluated, so the transcriber cannot prove what the call pins;
//! - the call is a plain statement of the test's own body (see
//!   `syntax::local_macro_calls`), the test holds no `use` item, and the
//!   macro is defined in the test's own file. Macros from other files or
//!   crates (`tokio_test::assert_ready!`) contribute nothing.

use super::super::{OracleFact, RustIndex, TestFact};
use crate::analysis::extract::{
    extract_assertions, extract_identifier_tokens, mask_comments_and_strings,
    retain_reached_oracles,
};
use crate::analysis::syntax::{LocalMacroCall, local_macro_calls};
use crate::domain::{OracleKind, OracleStrength};
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub(crate) fn credit_same_file_assertion_macros(index: &mut RustIndex) {
    let candidate_files = index
        .tests()
        .iter()
        .map(|test| &test.file)
        .collect::<std::collections::BTreeSet<_>>();
    let files = index
        .files()
        .iter()
        .filter(|(file, facts)| {
            !facts.used_lexical_fallback
                && candidate_files.contains(file)
                && facts.source.contains("macro_rules")
        })
        .collect::<Vec<_>>();
    let calls_by_file: BTreeMap<PathBuf, Vec<LocalMacroCall>> = files
        .par_iter()
        .map(|(file, facts)| ((*file).clone(), local_macro_calls(&facts.source)))
        .filter(|(_, calls)| !calls.is_empty())
        .collect();
    if calls_by_file.is_empty() {
        return;
    }

    let mut credited: BTreeMap<(PathBuf, usize, String), Vec<OracleFact>> = BTreeMap::new();
    for test in index.tests().iter() {
        let Some(calls) = calls_by_file.get(&test.file) else {
            continue;
        };
        if test_holds_use_item(test) {
            continue;
        }
        let mut oracles = calls
            .iter()
            .filter(|call| {
                call.function == test.name
                    && test.start_line <= call.start_line
                    && call.end_line <= test.end_line
            })
            // Never beside an assertion the ordinary extractor already read
            // on the call's lines: a local macro that reuses a recognized
            // assertion macro's name keeps that macro's own grade.
            .filter(|call| {
                !test
                    .assertions
                    .iter()
                    .any(|own| call.start_line <= own.line && own.line <= call.end_line)
            })
            .filter_map(macro_call_oracle)
            .collect::<Vec<_>>();
        // The same never-run statements the ordinary extractor drops.
        retain_reached_oracles(&mut oracles, &test.body, test.start_line);
        if !oracles.is_empty() {
            credited.insert(
                (test.file.clone(), test.start_line, test.name.clone()),
                oracles,
            );
        }
    }
    if credited.is_empty() {
        return;
    }
    index.for_each_test_mut(|test| {
        if let Some(oracles) =
            credited.get(&(test.file.clone(), test.start_line, test.name.clone()))
        {
            test.assertions.extend(oracles.iter().cloned());
            test.assertions
                .sort_by(|left, right| left.line.cmp(&right.line).then(left.text.cmp(&right.text)));
        }
    });
}

/// A `use` item can import a same-named macro over the local definition.
fn test_holds_use_item(test: &TestFact) -> bool {
    mask_comments_and_strings(&test.body)
        .split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .any(|word| word == "use")
}

/// The oracle one call earns: the weakest arm's evidence, capped at weak.
fn macro_call_oracle(call: &LocalMacroCall) -> Option<OracleFact> {
    let mut weakest: Option<OracleStrength> = None;
    for arm in &call.arms {
        let strength = arm_strength(arm)?;
        weakest = Some(match weakest {
            Some(current) if rank(&current) <= rank(&strength) => current,
            _ => strength,
        });
    }
    let text = call.text.trim().to_string();
    Some(OracleFact {
        line: call.start_line,
        kind: OracleKind::Unknown,
        strength: weakest?,
        observed_tokens: extract_identifier_tokens(&text),
        text,
        ok_value_observed: None,
    })
}

/// What one transcriber shows, capped; `None` when it shows no assertion.
fn arm_strength(arm: &str) -> Option<OracleStrength> {
    let mut assertions = extract_assertions(arm, 1);
    retain_reached_oracles(&mut assertions, arm, 1);
    let best = assertions
        .iter()
        .map(|assertion| assertion.strength.clone())
        .filter(|strength| rank(strength) > rank(&OracleStrength::Unknown))
        .max_by_key(rank);
    best.map(cap)
}

fn cap(strength: OracleStrength) -> OracleStrength {
    match strength {
        OracleStrength::Strong | OracleStrength::Medium => OracleStrength::Weak,
        other => other,
    }
}

/// Evidence order: `Unknown` is the floor of what counts as evidence.
fn rank(strength: &OracleStrength) -> u8 {
    match strength {
        OracleStrength::Strong => 5,
        OracleStrength::Medium => 4,
        OracleStrength::Weak => 3,
        OracleStrength::Smoke => 2,
        OracleStrength::Unknown => 1,
        OracleStrength::None => 0,
    }
}
