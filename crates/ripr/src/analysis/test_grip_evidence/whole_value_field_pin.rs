//! Whole-value field credit shared with supplied-diff analysis (#7894).
//!
//! A repo `field_construction` seam whose assertion is `assert_eq!(c, e)` with
//! an independently bound expected literal does not name the changed field in
//! assertion text. The existing token matcher stays conservative; this path
//! reuses the bounded `OwnerReturnPin` whole-value field authority instead.

use super::{FunctionSummary, OracleFact, RepoSeam, RustIndex, SeamKind, TestSummary};
use crate::analysis::classify::{
    OwnerPinSyntax, OwnerReturnPin, ProbeContext, is_inline_expect_snapshot,
};
use crate::domain::{DeltaKind, Probe, ProbeFamily, ProbeId, SourceLocation};

pub(super) struct WholeValueFieldPin<'a> {
    owner: &'a FunctionSummary,
    probe: Probe,
    pin: OwnerReturnPin,
}

impl<'a> WholeValueFieldPin<'a> {
    pub(super) fn establish(
        seam: &RepoSeam,
        owner: Option<&'a FunctionSummary>,
        index: &RustIndex,
    ) -> Option<Self> {
        if seam.kind() != SeamKind::FieldConstruction {
            return None;
        }
        let owner = owner?;
        if owner.file != seam.file() || owner.id.0.replace('\\', "/") != seam.owner() {
            return None;
        }
        if crate::analysis::classify::field_assignment_target(seam.expression()).is_some() {
            return None;
        }
        let probe = Probe {
            id: ProbeId(format!("repo-whole-value-field:{}", seam.id().as_str())),
            location: SourceLocation::new(seam.file().to_path_buf(), seam.display_line(), 1),
            owner: Some(owner.id.clone()),
            family: ProbeFamily::FieldConstruction,
            delta: DeltaKind::Unknown,
            before: None,
            after: Some(seam.expression().to_string()),
            expression: seam.expression().to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let pin = OwnerReturnPin::establish(&probe, owner, index)?;
        if !pin.credits_whole_value_field() {
            return None;
        }
        Some(Self { owner, probe, pin })
    }

    pub(super) fn admits(
        &self,
        test: &TestSummary,
        oracle: &OracleFact,
        index: &RustIndex,
        syntax: &OwnerPinSyntax,
    ) -> bool {
        if matches!(oracle.kind, crate::domain::OracleKind::Snapshot)
            && !is_inline_expect_snapshot(oracle)
        {
            return false;
        }
        if !super::oracle_kind_matches_seam_kind(SeamKind::FieldConstruction, &oracle.kind) {
            return false;
        }
        let context = ProbeContext::new(
            &self.probe,
            Some(self.owner),
            Vec::new(),
            false,
            index,
            false,
        );
        self.pin.admits(
            test,
            oracle,
            index,
            &|file, name| {
                index.files().get(file).is_none_or(|facts| {
                    context.test_file_imports_foreign_callee_name(file, &facts.source, name)
                })
            },
            syntax,
        )
    }
}
