//! Producer-owned value locations, separate from the stable four-field DTO.
use crate::domain::{self, Probe, ValueContext};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LocatedValueFact {
    #[serde(skip)]
    pub(crate) file: Option<PathBuf>,
    pub(crate) line: usize,
    pub(crate) text: String,
    pub(crate) value: String,
    pub(crate) context: ValueContext,
}
impl LocatedValueFact {
    pub(crate) fn into_public(self) -> domain::ValueFact {
        domain::ValueFact {
            line: self.line,
            text: self.text,
            value: self.value,
            context: self.context,
        }
    }
}
// A legacy public DTO has no origin; conversion must never invent one.
impl From<domain::ValueFact> for LocatedValueFact {
    fn from(fact: domain::ValueFact) -> Self {
        let domain::ValueFact {
            line,
            text,
            value,
            context,
        } = fact;
        Self {
            file: None,
            line,
            text,
            value,
            context,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LocatedActivationEvidence {
    pub(crate) observed_values: Vec<LocatedValueFact>,
    pub(crate) missing_discriminators: Vec<domain::MissingDiscriminatorFact>,
}
impl LocatedActivationEvidence {
    pub(crate) fn into_public(self) -> (domain::ActivationEvidence, Vec<Option<PathBuf>>) {
        let files = self
            .observed_values
            .iter()
            .map(|v| v.file.clone())
            .collect();
        (
            domain::ActivationEvidence {
                observed_values: self
                    .observed_values
                    .into_iter()
                    .map(LocatedValueFact::into_public)
                    .collect(),
                missing_discriminators: self.missing_discriminators,
            },
            files,
        )
    }
}

/// This run-local context records locations at projection time. A receipt binds
/// the complete probe and the ordered four-field vector, never a short ID or a
/// guessed line/token match. Mutable public reports invalidate changed receipts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ValueOrigins(
    std::collections::BTreeMap<domain::SourceLocation, Vec<OriginReceipt>>,
);
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OriginReceipt {
    probe: Probe,
    values: Vec<domain::ValueFact>,
    files: Vec<Option<PathBuf>>,
}
impl ValueOrigins {
    /// Prepare the immutable snapshot before acquiring any shared collector lock.
    pub(crate) fn prepare(
        finding: &domain::Finding,
        files: Vec<Option<PathBuf>>,
    ) -> Option<OriginReceipt> {
        if files.is_empty() || finding.activation.observed_values.len() != files.len() {
            return None;
        }
        Some(OriginReceipt {
            probe: finding.probe.clone(),
            values: finding.activation.observed_values.clone(),
            files,
        })
    }
    pub(crate) fn insert_receipt(&mut self, receipt: OriginReceipt) {
        // Location is only a routing bucket. Full probe and ordered facts
        // remain the authority, including when several probes share a line.
        self.0
            .entry(receipt.probe.location.clone())
            .or_default()
            .push(receipt);
    }
    pub(crate) fn record(&mut self, finding: &domain::Finding, files: Vec<Option<PathBuf>>) {
        if let Some(receipt) = Self::prepare(finding, files) {
            self.insert_receipt(receipt);
        }
    }
    pub(crate) fn extend(&mut self, other: Self) {
        for (location, mut receipts) in other.0 {
            // Append, never overwrite: conflicting receipts remain ambiguous.
            self.0.entry(location).or_default().append(&mut receipts);
        }
    }
    pub(crate) fn files(&self, finding: &domain::Finding) -> Option<&[Option<PathBuf>]> {
        self.files_for(&finding.probe, &finding.activation.observed_values)
    }
    fn files_for(&self, probe: &Probe, values: &[domain::ValueFact]) -> Option<&[Option<PathBuf>]> {
        let mut matches =
            self.0.get(&probe.location)?.iter().filter(|r| {
                &r.probe == probe && r.values == values && r.files.len() == values.len()
            });
        let first = matches.next()?;
        if matches.any(|other| other.files != first.files) {
            return None;
        }
        Some(&first.files)
    }
    /// Used only by producer-local contexts while finalizing a known probe.
    pub(crate) fn rebind_probe(&mut self, before: &Probe, after: &Probe) {
        let receipts = std::mem::take(&mut self.0).into_values().flatten();
        for mut receipt in receipts {
            if &receipt.probe == before {
                receipt.probe = after.clone();
            }
            self.insert_receipt(receipt);
        }
    }
    pub(crate) fn rebase(&mut self, rebase: impl Fn(&Path) -> PathBuf) {
        let receipts = std::mem::take(&mut self.0).into_values().flatten();
        for mut receipt in receipts {
            receipt.probe.location.file = rebase(&receipt.probe.location.file);
            for file in receipt.files.iter_mut().flatten() {
                *file = rebase(file);
            }
            self.insert_receipt(receipt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context(receipts: Vec<OriginReceipt>) -> ValueOrigins {
        let mut context = ValueOrigins::default();
        for receipt in receipts {
            context.insert_receipt(receipt);
        }
        context
    }
    fn probe() -> Probe {
        Probe {
            id: domain::ProbeId("p".into()),
            location: domain::SourceLocation::new("src/lib.rs", 3, 1),
            owner: None,
            family: domain::ProbeFamily::Predicate,
            delta: domain::DeltaKind::Control,
            before: None,
            after: None,
            expression: "x > 5".into(),
            expected_sinks: vec![],
            required_oracles: vec![],
        }
    }
    fn values() -> Vec<domain::ValueFact> {
        [10, 11]
            .map(|n| domain::ValueFact {
                line: 4,
                text: format!("price({n})"),
                value: n.to_string(),
                context: ValueContext::FunctionArgument,
            })
            .to_vec()
    }
    fn receipt(file: &str) -> OriginReceipt {
        OriginReceipt {
            probe: probe(),
            values: values(),
            files: vec![Some(file.into()), None],
        }
    }
    // Check receipt matching independently of a Finding fixture's narration.
    fn lookup<'a>(
        ctx: &'a ValueOrigins,
        p: &Probe,
        v: &[domain::ValueFact],
    ) -> Option<&'a [Option<PathBuf>]> {
        ctx.files_for(p, v)
    }
    #[test]
    fn clones_keep_occurrences_and_unknown_origins() {
        let ctx = context(vec![receipt("tests/a.py"), receipt("tests/a.py")]);
        assert_eq!(
            lookup(&ctx, &probe(), &values()),
            Some([Some(PathBuf::from("tests/a.py")), None].as_slice())
        );
    }
    #[test]
    fn conflicts_are_sticky_under_every_order_and_repeated_receipts() {
        for order in [["a", "b", "a"], ["a", "a", "b"], ["b", "a", "a"]] {
            let mut ctx = ValueOrigins::default();
            for file in order {
                ctx.extend(context(vec![receipt(file)]));
            }
            assert!(lookup(&ctx, &probe(), &values()).is_none());
        }
    }
    #[test]
    fn changed_payload_order_or_full_probe_invalidates_receipt() {
        let ctx = context(vec![receipt("tests/a.py")]);
        let mut v = values();
        v[0].text.push('!');
        assert!(lookup(&ctx, &probe(), &v).is_none());
        v = values();
        v.reverse();
        assert!(lookup(&ctx, &probe(), &v).is_none());
        let mut p = probe();
        p.expression.push('!');
        assert!(lookup(&ctx, &p, &values()).is_none());
    }
    #[test]
    fn malformed_occurrence_count_and_same_location_probe_collision_fail_closed() {
        let mut malformed = receipt("tests/a.py");
        malformed.files.pop();
        let ctx = context(vec![malformed]);
        assert!(lookup(&ctx, &probe(), &values()).is_none());
        let mut other = receipt("tests/b.py");
        other.probe.expression = "different expression under the same short ID".into();
        let ctx = context(vec![receipt("tests/a.py"), other]);
        assert_eq!(
            lookup(&ctx, &probe(), &values()).map(|f| f[0].as_deref()),
            Some(Some(Path::new("tests/a.py")))
        );
    }
    #[test]
    fn rebase_moves_probe_and_origin_together() {
        let mut ctx = context(vec![receipt("tests/a.py")]);
        ctx.rebase(|p| Path::new("repo").join(p));
        let mut p = probe();
        p.location.file = PathBuf::from("repo/src/lib.rs");
        assert_eq!(
            lookup(&ctx, &p, &values()).map(|files| files[0].as_deref()),
            Some(Some(Path::new("repo/tests/a.py")))
        );
        assert!(lookup(&ctx, &probe(), &values()).is_none());
    }
}
