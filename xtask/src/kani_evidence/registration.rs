//! Pre-execution registration commitment and exact selection intent.
use super::*;
use crate::formal_registry::{self as registry, PreparedRegistry};

pub(super) fn prepare(
    root: &Path,
    args: &lane::KaniArgs,
    packages: &[String],
) -> Result<Option<PreparedRegistry>, String> {
    let Some(path) = &args.claim_registry else {
        return Ok(None);
    };
    let prepared = registry::prepare(root, Path::new(path))?;
    for id in &args.registered_claims {
        let binding = prepared
            .claims
            .iter()
            .find(|p| &p.claim.claim_id == id)
            .ok_or_else(|| format!("registered claim is absent: {id}"))?;
        let subject = binding
            .subject
            .as_ref()
            .ok_or("selected claim has no native Kani route")?;
        if !packages.contains(&subject.package)
            || args.harness.as_ref().is_some_and(|filter| {
                !subject
                    .qualified_name
                    .as_ref()
                    .is_some_and(|name| name.contains(filter))
            })
        {
            return Err(format!(
                "registered claim is outside selected package/harness scope: {id}"
            ));
        }
    }
    Ok(Some(prepared))
}

pub(super) fn claims(prepared: Option<&PreparedRegistry>) -> Vec<formal::Claim> {
    prepared.map_or_else(Vec::new, |p| {
        p.claims.iter().map(|c| c.claim.clone()).collect()
    })
}
