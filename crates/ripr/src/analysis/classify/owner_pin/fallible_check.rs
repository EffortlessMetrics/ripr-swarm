//! SPEC-0197 rule 7a: exact source/value binding, not oracle proximity.

use super::OwnerPinSyntax;
use crate::analysis::facts::{
    FunctionContainer, FunctionSourceRole, FunctionSummary, ModulePathTarget, OracleFact,
    RustIndex, SourceRoleProvenanceEdgeKind, TestSummary,
};
use crate::analysis::syntax::{decimal_u8, literal_check_syntax, literal_owner_syntax};
use crate::analysis::workspace::{
    AnyhowIdentity, config_snapshot_identity, verified_anyhow_identity,
};
use crate::domain::{Probe, ProbeFamily, SymbolId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct FallibleCheckReceipt {
    owner: SymbolId,
    helper: SymbolId,
    test: SymbolId,
    owner_value: u8,
    expected_value: u8,
    source_identity: String,
    source_file: PathBuf,
    source_line: usize,
    test_file: PathBuf,
    test_name: String,
    test_line: usize,
    assertion_line: usize,
    assertion_text: String,
    local: String,
    source_hashes: Vec<String>,
    spans: Vec<(u32, u32)>,
    dependency_semantic: String,
    dependency_validation: String,
    pub(in crate::analysis) semantic_digest: String,
    validation_digest: String,
}

fn hash_fields(fields: &[String]) -> String {
    let mut hash = Sha256::new();
    for field in fields {
        hash.update((field.len() as u64).to_le_bytes());
        hash.update(field.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn source_hash(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

impl FallibleCheckReceipt {
    fn semantic_fields(&self) -> Vec<String> {
        vec![
            "fallible-owner-literal-7a-v1".into(),
            self.owner.0.clone(),
            self.helper.0.clone(),
            self.test.0.clone(),
            self.test_name.clone(),
            self.owner_value.to_string(),
            self.expected_value.to_string(),
            self.source_identity.clone(),
            self.local.clone(),
            self.dependency_semantic.clone(),
        ]
    }

    fn validation_fields(&self) -> Vec<String> {
        let mut fields = self.semantic_fields();
        fields.extend(self.source_hashes.clone());
        fields.extend(
            self.spans
                .iter()
                .map(|(start, end)| format!("{start}:{end}")),
        );
        fields.extend([
            self.source_file.to_string_lossy().into_owned(),
            self.source_line.to_string(),
            self.test_file.to_string_lossy().into_owned(),
            self.test_line.to_string(),
            self.assertion_line.to_string(),
            self.assertion_text.clone(),
            self.dependency_validation.clone(),
        ]);
        fields
    }

    pub(in crate::analysis) fn valid_for(&self, probe: &Probe) -> bool {
        probe.family == ProbeFamily::ReturnValue
            && probe.owner.as_ref() == Some(&self.owner)
            && probe.expression.trim() == self.source_identity
            && probe.location.file == self.source_file
            && probe.location.line == self.source_line
            && self.owner_value == self.expected_value
            && decimal_u8(&self.source_identity) == Some(self.owner_value)
            && self.source_hashes.len() == 2
            && !self.spans.is_empty()
            && self.semantic_digest == hash_fields(&self.semantic_fields())
            && self.validation_digest == hash_fields(&self.validation_fields())
    }

    pub(in crate::analysis) fn admits(&self, test: &TestSummary, assertion: &OracleFact) -> bool {
        self.test_file == test.file
            && self.test_name == test.name
            && self.test_line == test.start_line
            && self.assertion_line == assertion.line
            && self.assertion_text == assertion.text.trim()
    }
}

impl OwnerPinSyntax {
    fn fallible_dependency(&self, root: &Path) -> Option<AnyhowIdentity> {
        let snapshot = config_snapshot_identity(root)?;
        let mut memo = self.fallible_dependencies.borrow_mut();
        if let Some((previous, identity)) = memo.get(root)
            && *previous == snapshot
        {
            return identity.clone();
        }
        let identity = verified_anyhow_identity(root);
        memo.insert(root.to_path_buf(), (snapshot, identity.clone()));
        identity
    }
}

pub(in crate::analysis) fn fallible_check_receipt(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> Option<FallibleCheckReceipt> {
    if probe.family != ProbeFamily::ReturnValue
        || probe.owner.as_ref() != Some(&owner.id)
        || owner.source_role != FunctionSourceRole::Production
        || !matches!(owner.item.container, FunctionContainer::Free)
        || !test
            .assertions
            .iter()
            .any(|a| a.text.trim().starts_with("ensure!("))
    {
        return None;
    }
    let parent = index.files().get(&owner.file)?;
    let child = index.files().get(&test.file)?;
    let analysis_root = index
        .workspace_authority
        .as_ref()?
        .root
        .canonicalize()
        .ok()?;
    let owner_path = analysis_root.join(&owner.file);
    let test_path = analysis_root.join(&test.file);
    if parent.used_lexical_fallback
        || child.used_lexical_fallback
        || child.role_provenance.earliest_unresolved_reason.is_some()
    {
        return None;
    }
    let owner_syntax = literal_owner_syntax(&parent.source, owner)?;
    let [edge] = child.role_provenance.edges.as_slice() else {
        return None;
    };
    if edge.kind != SourceRoleProvenanceEdgeKind::Module
        || !edge.requires_test
        || edge.parent != owner.file
        || edge.child != test.file
        || edge.line != owner_syntax.module_line
        || edge.declaration != format!("mod {};", owner_syntax.module_name)
    {
        return None;
    }
    let [declaration] = parent.module_declarations.as_slice() else {
        return None;
    };
    let ModulePathTarget::Literal(path) = &declaration.path_target else {
        return None;
    };
    if declaration.name != owner_syntax.module_name
        || declaration.line != edge.line
        || !declaration.requires_test
        || owner_path.parent()?.join(path).canonicalize().ok()? != test_path.canonicalize().ok()?
    {
        return None;
    }
    let literal_start = u32::from(owner_syntax.literal.start()) as usize;
    let literal_end = u32::from(owner_syntax.literal.end()) as usize;
    let source_identity = parent.source.get(literal_start..literal_end)?;
    let source_line = 1 + parent
        .source
        .get(..literal_start)?
        .bytes()
        .filter(|b| *b == b'\n')
        .count();
    if probe.expression.trim() != source_identity
        || probe.location.line != source_line
        || probe
            .location
            .file
            .canonicalize()
            .or_else(|_| analysis_root.join(&probe.location.file).canonicalize())
            .ok()?
            != owner_path.canonicalize().ok()?
    {
        return None;
    }
    let check = literal_check_syntax(&child.source, test, &owner.name)?;
    // Refuse inverted oracles before constructing any favorable evidence.
    if owner_syntax.value != check.expected {
        return None;
    }
    let helpers = index
        .functions()
        .iter()
        .filter(|f| {
            f.file == test.file && f.name == check.helper_name && f.start_line == check.helper_line
        })
        .collect::<Vec<_>>();
    let tests = index
        .functions()
        .iter()
        .filter(|f| f.file == test.file && f.name == test.name && f.start_line == test.start_line)
        .collect::<Vec<_>>();
    let ([helper], [test_function]) = (helpers.as_slice(), tests.as_slice()) else {
        return None;
    };
    if helper.source_role != FunctionSourceRole::CfgTestModule
        || index
            .functions()
            .iter()
            .filter(|f| f.name == owner.name)
            .count()
            != 1
    {
        return None;
    }
    let oracle = test
        .assertions
        .iter()
        .filter(|a| {
            a.line == check.assertion_line
                && a.text.trim() == check.assertion_text
                && a.kind == crate::domain::OracleKind::ExactValue
                && a.strength == crate::domain::OracleStrength::Strong
        })
        .collect::<Vec<_>>();
    if oracle.len() != 1 {
        return None;
    }
    let root = owner_path
        .parent()?
        .ancestors()
        .find(|p| p.join("Cargo.toml").is_file())?;
    // The initial cache/dependency scope is one standalone owning package.
    if analysis_root != root.canonicalize().ok()?
        || !test_path
            .canonicalize()
            .ok()?
            .starts_with(root.canonicalize().ok()?)
    {
        return None;
    }
    // Source re-reads bind the immutable parser index to the dependency probe.
    if std::fs::read_to_string(&owner_path).ok()?.as_str() != parent.source.as_ref()
        || std::fs::read_to_string(&test_path).ok()?.as_str() != child.source.as_ref()
    {
        return None;
    }
    let dependency = syntax.fallible_dependency(root)?;
    if owner_path.canonicalize().ok()? != dependency.library_root
        || std::fs::read_to_string(&owner_path).ok()?.as_str() != parent.source.as_ref()
        || std::fs::read_to_string(&test_path).ok()?.as_str() != child.source.as_ref()
    {
        return None;
    }
    let mut spans = vec![
        (literal_start as u32, literal_end as u32),
        (edge.line as u32, edge.line as u32),
    ];
    spans.extend(check.spans);
    let mut receipt = FallibleCheckReceipt {
        owner: owner.id.clone(),
        helper: helper.id.clone(),
        test: test_function.id.clone(),
        owner_value: owner_syntax.value,
        expected_value: check.expected,
        source_identity: source_identity.into(),
        source_file: probe.location.file.clone(),
        source_line,
        test_file: test.file.clone(),
        test_name: test.name.clone(),
        test_line: test.start_line,
        assertion_line: check.assertion_line,
        assertion_text: check.assertion_text,
        local: check.local,
        source_hashes: vec![source_hash(&parent.source), source_hash(&child.source)],
        spans,
        dependency_semantic: dependency.semantic_digest,
        dependency_validation: dependency.validation_digest,
        semantic_digest: String::new(),
        validation_digest: String::new(),
    };
    receipt.semantic_digest = hash_fields(&receipt.semantic_fields());
    receipt.validation_digest = hash_fields(&receipt.validation_fields());
    receipt.valid_for(probe).then_some(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::classify::propagation_witness::{
        complete_direct_witness, fallible_literal_witness,
    };
    use crate::domain::{DeltaKind, ProbeId, SourceLocation};
    use crate::domain::{FlowSinkFact, FlowSinkKind};

    #[test]
    fn literal_witness_rejects_receipt_and_owner_identity_mutations() -> Result<(), String> {
        let probe = Probe {
            id: ProbeId("probe:literal".into()),
            location: SourceLocation::new("src/lib.rs", 1, 1),
            owner: Some(SymbolId("owner:answer".into())),
            family: ProbeFamily::ReturnValue,
            delta: DeltaKind::Value,
            before: Some("7".into()),
            after: Some("7".into()),
            expression: "7".into(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let mut receipt = FallibleCheckReceipt {
            owner: probe.owner.clone().ok_or("owner")?,
            helper: SymbolId("helper:check_answer".into()),
            test: SymbolId("test:observes_answer".into()),
            owner_value: 7,
            expected_value: 7,
            source_identity: "7".into(),
            source_file: probe.location.file.clone(),
            source_line: 1,
            test_file: PathBuf::from("src/test_support.rs"),
            test_name: "observes_answer".into(),
            test_line: 11,
            assertion_line: 5,
            assertion_text: "ensure!(actual == 7);".into(),
            local: "actual".into(),
            source_hashes: vec!["owner-snapshot".into(), "helper-snapshot".into()],
            spans: vec![(0, 1)],
            dependency_semantic: "canonical-anyhow".into(),
            dependency_validation: "locked-config-snapshot".into(),
            semantic_digest: String::new(),
            validation_digest: String::new(),
        };
        receipt.semantic_digest = hash_fields(&receipt.semantic_fields());
        receipt.validation_digest = hash_fields(&receipt.validation_fields());
        let sink = FlowSinkFact {
            kind: FlowSinkKind::ReturnValue,
            text: "7".into(),
            line: 1,
            owner: probe.owner.clone(),
        };
        let witness = fallible_literal_witness(&probe, &[sink], &receipt)
            .ok_or("complete literal witness")?;
        assert!(complete_direct_witness(&probe, Some(&witness)));
        for field in [
            "expected",
            "source",
            "dependency",
            "coordinate",
            "span",
            "helper",
        ] {
            let mut changed = receipt.clone();
            match field {
                "expected" => changed.expected_value = 8,
                "source" => changed.source_hashes[1] = "changed".into(),
                "dependency" => changed.dependency_validation = "changed".into(),
                "coordinate" => changed.source_line = 2,
                "span" => changed.spans[0] = (1, 2),
                "helper" => changed.helper = SymbolId("other-helper".into()),
                _ => return Err("unknown mutation control".into()),
            }
            assert!(!changed.valid_for(&probe), "{field}");
            let mut forged = witness.clone();
            forged.fallible_check = Some(changed);
            forged.semantic_digest = forged.compute_semantic_digest();
            assert!(
                !complete_direct_witness(&probe, Some(&forged)),
                "{field}: outer digest alone is insufficient"
            );
        }
        let mut other = probe.clone();
        other.location.file = PathBuf::from("other/lib.rs");
        assert!(!complete_direct_witness(&other, Some(&witness)));
        other = probe;
        other.owner = Some(SymbolId("other-owner".into()));
        assert!(!complete_direct_witness(&other, Some(&witness)));
        Ok(())
    }
}
