//! Case populations for committed verdict rates. These are record comparisons,
//! not fresh analyzer or runtime mutation measurements.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(super) const SCHEMA: &str = "ripr-verdict-case-population-v1";

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Population {
    pub(super) schema_version: String,
    pub(super) rate: String,
    pub(super) cases: Vec<Contribution>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Contribution {
    pub(super) case_id: String,
    pub(super) definition_sha256: String,
    pub(super) numerator: u64,
    pub(super) denominator: u64,
}

impl Population {
    pub(super) fn totals(&self) -> Result<(u64, u64), String> {
        if self.schema_version != SCHEMA || self.cases.is_empty() {
            return Err("missing or unsupported case population evidence".to_string());
        }
        let mut ids = BTreeSet::new();
        let (mut numerator, mut denominator) = (0_u64, 0_u64);
        for case in &self.cases {
            if case.case_id.is_empty() || !ids.insert(&case.case_id) {
                return Err(format!("missing or duplicate case id `{}`", case.case_id));
            }
            if case.definition_sha256.len() != 64
                || !case.definition_sha256.bytes().all(|b| b.is_ascii_hexdigit())
                || case.numerator > case.denominator
            {
                return Err(format!("invalid definition or contribution for `{}`", case.case_id));
            }
            numerator = numerator.checked_add(case.numerator).ok_or("numerator overflow")?;
            denominator = denominator.checked_add(case.denominator).ok_or("denominator overflow")?;
        }
        Ok((numerator, denominator))
    }

    pub(super) fn value(&self) -> Option<f64> {
        let (n, d) = self.totals().ok()?;
        (d > 0).then(|| n as f64 / d as f64)
    }
}
